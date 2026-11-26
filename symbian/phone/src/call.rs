// SPDX-License-Identifier: GPL-3.0-only
//! The call screens (NEOPHONE v2 p2-call-*, full screen): incoming, calling, active, call waiting, two calls, a
//! conference, the keypad, the summary.
//! [`phone_core::calls`] picks the screen from MODEMD's call list; this module fills `CallState`, sends the buttons to
//! MODEMD, opens the screens for a new call and closes them (after the summary) when the last call ends.

use crate::ctx::Ctx;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use e32::time::millis;
use modem_ipc::{Call, call as st};
use neoapp::card;
use phone_common::ui::{Chrome, tool};
use phone_core::calls::{self, Screen, Tracker};
use phone_core::page::{ContactsView, Page, PhoneView};
use phone_ui::{CallState, Party, PhoneApp};
use slint::{Color, ComponentHandle, SharedString};

/// How long the summary stays.
const SUMMARY_MS: u64 = 2500;
/// The pulsing ring's frame time while a call rings (the loop only wakes for it then).
const PULSE_MS: u64 = 50;
/// The tones the keypad shows (the latest).
const TONES_MAX: usize = 24;

#[derive(Default)]
pub struct State {
    pub calls: Tracker,
    keypad: bool,
    tones: String,
    /// the call Silence quietened (MODEMD op SILENCE; the ring settings stay), until nothing rings
    silenced: Option<u8>,
    /// the parties shown, by call id, number and presentation: one contact lookup per call
    who: Vec<(u8, String, u8, Party)>,
    /// the shade's ongoing call card as posted: the call and its line (None: no card)
    card: Option<(u8, &'static str)>,
}

/// The ongoing call card's id (the missed-calls card is [`crate::ctx::MISSED_CARD`]).
pub const CALL_CARD: u32 = 2;

pub fn bind(ctx: &Rc<Ctx>, ui: &PhoneApp) {
    let ctx = Rc::downgrade(ctx);
    ui.global::<CallState>().on_act(move |action, value| {
        let Some(ctx) = ctx.upgrade() else { return };
        act(&ctx, &action, &value);
    });
}

fn screen(ctx: &Ctx) -> Screen {
    let m = ctx.m.borrow();
    m.call.calls.screen(m.status.as_ref())
}

/// The call a screen is about, and the second one (the waiting caller, the held call).
fn calls_of(screen: Screen) -> (Option<Call>, Option<Call>) {
    match screen {
        Screen::None => (None, None),
        Screen::Incoming(c) | Screen::Calling(c) | Screen::Active(c) => (Some(c), None),
        Screen::Waiting { active, waiting } => (Some(active), Some(waiting)),
        Screen::Two { active, held } => (Some(active), Some(held)),
        Screen::Conference { first, .. } => (Some(first), None),
        Screen::Ended(e) => (Some(e.call), None),
    }
}

fn act(ctx: &Ctx, action: &str, value: &str) {
    let (main, other) = calls_of(screen(ctx));
    let id = |c: Option<Call>| c.map_or(0, |c| c.id);
    match action {
        "add" => return ctx.go(Page::Phone(PhoneView::Dial)),
        "contacts" => return ctx.go(Page::Contacts(ContactsView::List)),
        // Reply with message (mockup 33) on the incoming call; Message on the summary opens the thread
        "message" => {
            let number = main.map(|c| String::from(c.number.as_str()));
            let Some(n) = number.filter(|n| !n.is_empty()) else {
                return ctx.note("No number to message");
            };
            if matches!(screen(ctx), Screen::Incoming(_)) {
                ctx.ui().global::<CallState>().set_replying(true);
                return show(ctx);
            }
            close(ctx);
            return ctx.message(&n, true);
        }
        "reply-cancel" => {
            ctx.ui().global::<CallState>().set_replying(false);
            return;
        }
        "reply" => {
            ctx.ui().global::<CallState>().set_replying(false);
            let Some(c) = main.filter(|c| !c.number.as_str().is_empty()) else {
                return;
            };
            let n = String::from(c.number.as_str());
            // reject as busy first, so the caller hears busy before the text arrives
            let r = ctx.m.borrow_mut().modem.hangup(c.id);
            if r < 0 {
                ctx.note(&format!("Action failed ({r})"));
            }
            let replies = ctx.m.borrow().msgs.replies.clone();
            let i: usize = value.parse().unwrap_or(usize::MAX);
            match replies.get(i) {
                Some(text) => quick_reply(ctx, &n, text),
                None => ctx.message(&n, true),
            }
            return;
        }
        "call-again" => {
            let number = main.map(|c| String::from(c.number.as_str()));
            close(ctx);
            match number.filter(|n| !n.is_empty()) {
                Some(n) => crate::dial::place_call(ctx, &n),
                None => ctx.note("No number to call"),
            }
            return;
        }
        _ => {}
    }
    let r = {
        let mut m = ctx.m.borrow_mut();
        let audio = m.status.map(|s| s.audio).unwrap_or_default();
        match action {
            "answer" => m.modem.answer(id(main)),
            // a conference ends with all its calls
            "end" if main.is_some_and(|c| c.multiparty) => {
                let ids: Vec<u8> = m.status.map_or_else(Vec::new, |s| {
                    s.calls
                        .as_slice()
                        .iter()
                        .filter(|c| c.multiparty && calls::connected(c))
                        .map(|c| c.id)
                        .collect()
                });
                ids.iter()
                    .map(|i| m.modem.hangup(*i))
                    .find(|r| *r < 0)
                    .unwrap_or(0)
            }
            // MODEMD rejects a ringing call as busy
            "reject" | "end" => m.modem.hangup(id(main)),
            "silence" => match (m.call.silenced, main) {
                (None, Some(c)) => {
                    m.call.silenced = Some(c.id);
                    m.modem.silence(c.id)
                }
                _ => 0,
            },
            "mute" => m.modem.mute(!audio.muted),
            "speaker" => m.modem.speaker(!audio.speaker),
            "hold" => match main {
                Some(c) if c.state == st::HOLD => m.modem.resume(c.id),
                Some(c) => m.modem.hold(c.id),
                None => 0,
            },
            "keypad" => {
                m.call.keypad = !m.call.keypad;
                0
            }
            "dtmf" => {
                let tones = &mut m.call.tones;
                if tones.len() >= TONES_MAX {
                    tones.remove(0);
                }
                tones.push_str(value);
                m.modem.dtmf(value.bytes().next().unwrap_or(b'0'))
            }
            "hold-answer" => m.modem.answer(id(other)),
            "end-answer" => m.modem.end_and_answer(),
            "reject-waiting" => m.modem.hangup(id(other)),
            "swap" => m.modem.swap(),
            "merge" => m.modem.merge(),
            _ => 0,
        }
    };
    if r < 0 {
        ctx.note(&format!("Action failed ({r})"));
    }
    show(ctx);
}

/// The shade card's End (an in-place action: the user stays where they are).
pub fn end(ctx: &Ctx) {
    act(ctx, "end", "");
}

/// The shade's ongoing call card (top-panel TP2b, Live section): the caller, "Calling…" or "Call in progress" since
/// the call connected, and End; it goes when the call ends or rings (the incoming screen shows then).
fn sync_card(ctx: &Ctx) {
    let (main, _) = calls_of(screen(ctx));
    let want = main.and_then(|c| match screen(ctx) {
        Screen::Calling(_) => Some((c, "Calling…")),
        Screen::Active(_)
        | Screen::Waiting { .. }
        | Screen::Two { .. }
        | Screen::Conference { .. } => Some((c, "Call in progress")),
        _ => None,
    });
    if want.map(|(c, t)| (c.id, t)) == ctx.m.borrow().call.card {
        return;
    }
    let r = match want {
        Some((c, text)) => {
            let p = party(ctx, &c);
            let since = ctx
                .m
                .borrow()
                .call
                .calls
                .seconds(c.id, millis())
                .unwrap_or(0);
            ctx.app.notify(&neoapp::Notification {
                id: CALL_CARD,
                title: phone_core::messages::cut(&p.name, neoui_proto::ipc::TITLE_MAX).into(),
                text: text.into(),
                page: phone_core::page::number::phone::CALL,
                actions: alloc::vec!["End".into()],
                initials: p.initials.as_str().into(),
                tint: if p.initials.is_empty() {
                    0
                } else {
                    p.tint.as_argb_encoded() & 0xff_ffff
                },
                flags: card::NOTIFY_ONGOING,
                style: card::style::CALL,
                channel: phone_common::channels::phone::CALLS,
                when: (neoapp::wall_ms() - i64::from(since) * 1000).max(0) as u64,
                action_kinds: alloc::vec![card::action::IN_PLACE],
                ..Default::default()
            })
        }
        None => ctx.app.cancel_notify(CALL_CARD),
    };
    match r {
        Ok(()) => ctx.m.borrow_mut().call.card = want.map(|(c, t)| (c.id, t)),
        Err(e) => e32::dprintln!("neophone: call card not posted ({})", e),
    }
}

/// The caller line and avatar for `c` (looked up once per call); a conference's call: "Conference" over its
/// participants' names (no initials: the screen shows the group glyph).
fn party(ctx: &Ctx, c: &Call) -> Party {
    if c.multiparty {
        let calls: Vec<Call> = ctx.m.borrow().status.map_or_else(Vec::new, |s| {
            s.calls
                .as_slice()
                .iter()
                .filter(|x| x.multiparty && calls::connected(x))
                .copied()
                .collect()
        });
        let names: Vec<String> = calls
            .iter()
            .map(|x| String::from(party_of(ctx, x).name.as_str()))
            .collect();
        return Party {
            name: "Conference".into(),
            line: names.join(", ").into(),
            group: true,
            ..Party::default()
        };
    }
    party_of(ctx, c)
}

fn party_of(ctx: &Ctx, c: &Call) -> Party {
    let number = c.number.as_str();
    let cached = ctx
        .m
        .borrow()
        .call
        .who
        .iter()
        .find_map(|(id, n, p, party)| {
            (*id == c.id && n == number && *p == c.presentation).then(|| party.clone())
        });
    if let Some(p) = cached {
        return p;
    }
    let name = ctx.name_of(number);
    let (contact, label) = {
        let m = ctx.m.borrow();
        m.book
            .lookup(number)
            .map(|ct| {
                let label = ct
                    .numbers
                    .iter()
                    .find(|(_, n)| phone_rules::number::matches(n, number))
                    .map_or("", |(l, _)| l.text());
                (ct.id, label)
            })
            .unzip()
    };
    let emergency = !c.incoming && phone_rules::emergency::check(number, None).is_some();
    let (main, line) =
        phone_rules::caller::caller_line(name.as_deref(), number, c.presentation, emergency);
    // a second line means a contact's name shows: its initials and colour, else a grey #
    let known = !line.is_empty();
    let p = Party {
        initials: if known {
            calls::initials(&main).into()
        } else {
            "".into()
        },
        tint: Color::from_argb_encoded(0xff00_0000 | calls::tint(contact.unwrap_or(0))),
        label: if known { label.unwrap_or("") } else { "" }.into(),
        name: main.into(),
        line: line.into(),
        group: false,
    };
    ctx.m
        .borrow_mut()
        .call
        .who
        .push((c.id, number.into(), c.presentation, p.clone()));
    p
}

pub fn show(ctx: &Ctx) {
    let now = millis();
    let screen = screen(ctx);
    if screen == Screen::None {
        // opened with no call (a stale notification, no MODEMD): back where the app was
        let before = ctx.m.borrow().before_call;
        if ctx.page() == Page::Call {
            ctx.go(before);
        }
        return;
    }
    let (main, other) = calls_of(screen);
    let (audio, keypad, tones, silenced, show_duration) = {
        let m = ctx.m.borrow();
        (
            m.status.map(|s| s.audio).unwrap_or_default(),
            m.call.keypad,
            m.call.tones.clone(),
            m.call.silenced.is_some(),
            m.prefs.show_duration,
        )
    };
    let clock = |c: &Call| {
        let t = ctx.m.borrow().call.calls.seconds(c.id, now).unwrap_or(0);
        calls::clock(t)
    };
    let (name, main_time, other_time, duration) = match screen {
        Screen::None => return,
        Screen::Incoming(_) => ("incoming", String::new(), String::new(), String::new()),
        Screen::Calling(c) => {
            let t = if c.state == st::ALERTING {
                "Ringing…"
            } else {
                "Calling…"
            };
            ("calling", t.into(), String::new(), String::new())
        }
        Screen::Active(c) | Screen::Waiting { active: c, .. } => {
            let t = if c.state == st::HOLD {
                "On hold".into()
            } else {
                clock(&c)
            };
            let name = if matches!(screen, Screen::Active(_)) {
                "active"
            } else {
                "waiting"
            };
            (name, t, String::new(), String::new())
        }
        Screen::Conference { first, .. } => {
            let t = if first.state == st::HOLD {
                "On hold".into()
            } else {
                clock(&first)
            };
            ("conference", t, String::new(), String::new())
        }
        Screen::Two { active, held } => {
            let o = if held.state == st::HOLD {
                format!("On hold · {}", clock(&held))
            } else {
                clock(&held)
            };
            ("two", clock(&active), o, String::new())
        }
        Screen::Ended(e) => {
            // a call that never connected has no length to show (p2-call-summary-missed)
            let d = match e.seconds {
                Some(t) if show_duration => calls::length(t),
                _ => String::new(),
            };
            ("ended", String::new(), String::new(), d)
        }
    };
    let main_party = main.map(|c| party(ctx, &c)).unwrap_or_default();
    let other_party = other.map(|c| party(ctx, &c)).unwrap_or_default();
    let ui = ctx.ui();
    let g = ui.global::<CallState>();
    g.set_screen(name.into());
    g.set_main(main_party);
    g.set_other(other_party);
    g.set_main_time(main_time.into());
    g.set_other_time(other_time.into());
    g.set_muted(audio.muted);
    g.set_speaker(audio.speaker);
    g.set_held(main.is_some_and(|c| c.state == st::HOLD));
    g.set_silenced(silenced);
    g.set_keypad(keypad);
    g.set_tones(tones.into());
    g.set_duration(duration.into());
    // a number to text back: not a private caller's
    g.set_can_message(main.is_some_and(|c| {
        !c.number.as_str().is_empty()
            && c.presentation != phone_rules::caller::presentation::RESTRICTED
    }));
    let mut replies: Vec<SharedString> = ctx
        .m
        .borrow()
        .msgs
        .replies
        .iter()
        .map(|r| r.as_str().into())
        .collect();
    replies.push("Write your own…".into());
    g.set_replies(phone_common::ui::model(replies));
    if name != "incoming" {
        g.set_replying(false);
    }
}

pub fn chrome(ctx: &Ctx) -> Chrome {
    match screen(ctx) {
        // full screen: the toolbar is an empty black strip
        Screen::Incoming(_) => Chrome::titled("Incoming call").with(tool::NONE, tool::EMPTY, ""),
        Screen::Waiting { .. } => Chrome::titled("Call waiting").with(tool::NONE, tool::EMPTY, ""),
        Screen::Ended(_) => Chrome::titled("Call ended"),
        _ => Chrome::titled("Call"),
    }
}

pub fn action(_ctx: &Ctx) {}

/// Back on the call screen hides it: the call goes on and the green bar under the header returns to it.
pub fn back(ctx: &Ctx) -> bool {
    let before = ctx.m.borrow().before_call;
    if before == Page::Call {
        return false;
    }
    ctx.go(before);
    true
}

/// The summary is over (or off): back where the app was before the call.
fn close(ctx: &Ctx) {
    let before = {
        let mut m = ctx.m.borrow_mut();
        m.call.calls.close();
        m.before_call
    };
    if ctx.page() == Page::Call {
        ctx.go(before);
    }
}

/// MODEMD's status changed: open the call screens for a new call, close them when the last call ends.
pub fn on_status(ctx: &Ctx) {
    let now = millis();
    let (u, summary) = {
        let mut m = ctx.m.borrow_mut();
        let Some(s) = m.status else { return };
        let u = m.call.calls.update(&s, now);
        if !s.calls.as_slice().iter().any(calls::ringing) {
            m.call.silenced = None;
        }
        if u.all_ended {
            m.call.keypad = false;
            m.call.tones.clear();
        }
        let ended = m.call.calls.ended.map(|e| e.call.id);
        m.call.who.retain(|(id, ..)| {
            Some(*id) == ended || s.calls.as_slice().iter().any(|c| c.id == *id)
        });
        let summary = m.prefs.summary && m.call.calls.ended.is_some_and(|e| e.summary());
        (u, summary)
    };
    if u.appeared && ctx.page() != Page::Call {
        ctx.go(Page::Call);
    }
    // the summary shows only over the call screens (the user may be in Add call)
    if u.all_ended && (!summary || ctx.page() != Page::Call) {
        close(ctx);
    }
    sync_card(ctx);
}

pub fn tick(ctx: &Ctx) {
    let over = {
        let m = ctx.m.borrow();
        m.call
            .calls
            .ended
            .is_some_and(|e| millis() >= e.at + SUMMARY_MS)
    };
    if over {
        close(ctx);
    } else if ctx.page() == Page::Call {
        show(ctx);
    }
}

/// The summary's end; while the call screens show, the next timer second or the ring's next frame.
pub fn next_wake(ctx: &Ctx) -> Option<u64> {
    let m = ctx.m.borrow();
    if let Some(e) = m.call.calls.ended {
        return Some(e.at + SUMMARY_MS);
    }
    if m.page != Page::Call || !m.visible {
        return None;
    }
    let now = millis();
    match m.call.calls.screen(m.status.as_ref()) {
        Screen::Incoming(_) | Screen::Waiting { .. } => Some(now + PULSE_MS),
        _ => m.call.calls.next_tick(now),
    }
}

/// Send quick reply `text` to `number` (Reply with message): the note says to whom.
fn quick_reply(ctx: &Ctx, number: &str, text: &str) {
    let r = ctx.m.borrow_mut().modem.sms_send(number, text);
    if r < 0 {
        ctx.note(&format!("Message not sent ({r})"));
        return;
    }
    let who = ctx.name_of(number).unwrap_or_else(|| String::from(number));
    let first = String::from(who.split_whitespace().next().unwrap_or(&who));
    ctx.note(&format!("Message sent to {first}"));
}
