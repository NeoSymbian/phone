// SPDX-License-Identifier: GPL-3.0-only
//! The call screens (mockups 9-15): incoming, calling, active, call waiting, two calls, the keypad, the summary.
//! [`phone_core::calls`] picks the screen from MODEMD's call list; this module fills `CallState`, sends the buttons to
//! MODEMD, opens the screens for a new call and closes them (after the summary) when the last call ends.

use crate::ctx::{Chrome, Ctx};
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use e32::time::millis;
use modem_ipc::{Call, Ring, call as st};
use neoui_proto::ipc as proto;
use phone_core::calls::{self, Screen, Tracker};
use phone_core::page::{ContactsView, Page, PhoneView};
use phone_ui::{CallState, Party, PhoneApp};
use slint::{Color, ComponentHandle};

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
    /// the ring settings Silence replaced, restored once nothing rings
    silenced: Option<Ring>,
    /// the parties shown, by call id, number and presentation: one contact lookup per call
    who: Vec<(u8, String, u8, Party)>,
}

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
        Screen::Ended(e) => (Some(e.call), None),
    }
}

fn act(ctx: &Ctx, action: &str, value: &str) {
    let (main, other) = calls_of(screen(ctx));
    let id = |c: Option<Call>| c.map_or(0, |c| c.id);
    match action {
        "add" => return ctx.go(Page::Phone(PhoneView::Dial)),
        "contacts" => return ctx.go(Page::Contacts(ContactsView::List)),
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
            // MODEMD rejects a ringing call as busy
            "reject" | "end" => m.modem.hangup(id(main)),
            "silence" => match (m.call.silenced, m.status) {
                (None, Some(s)) => {
                    m.call.silenced = Some(s.ring);
                    m.modem.ring(s.ring.volume, true, false)
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

/// The caller line and avatar for `c` (looked up once per call).
fn party(ctx: &Ctx, c: &Call) -> Party {
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
    let name = crate::people::name_of(ctx, number);
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
        Screen::Two { active, held } => {
            let o = if held.state == st::HOLD {
                format!("On hold · {}", clock(&held))
            } else {
                clock(&held)
            };
            ("two", clock(&active), o, String::new())
        }
        Screen::Ended(e) => {
            let d = if show_duration {
                calls::length(e.seconds.unwrap_or(0))
            } else {
                String::new()
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
}

pub fn chrome(ctx: &Ctx) -> Chrome {
    match screen(ctx) {
        // full screen: the toolbar is an empty black strip
        Screen::Incoming(_) => {
            Chrome::titled("Incoming call").with(proto::TOOL_NONE, proto::TOOLBAR_EMPTY, "")
        }
        Screen::Waiting { .. } => {
            Chrome::titled("Call waiting").with(proto::TOOL_NONE, proto::TOOLBAR_EMPTY, "")
        }
        Screen::Ended(_) => Chrome::titled("Call ended"),
        _ => Chrome::titled("Call"),
    }
}

pub fn options(ctx: &Ctx) -> Vec<&'static str> {
    let screen = screen(ctx);
    if !matches!(
        screen,
        Screen::Calling(_) | Screen::Active(_) | Screen::Two { .. }
    ) {
        return Vec::new();
    }
    let audio = ctx.m.borrow().status.map(|s| s.audio).unwrap_or_default();
    let mut items = Vec::new();
    items.push(if audio.muted { "Unmute" } else { "Mute" });
    items.push(if audio.speaker {
        "Speaker off"
    } else {
        "Speaker on"
    });
    if let Screen::Active(c) = screen {
        items.push(if c.state == st::HOLD {
            "Resume"
        } else {
            "Hold"
        });
    }
    items.push("End all calls");
    items
}

pub fn option(ctx: &Ctx, item: &str) {
    match item {
        "Mute" | "Unmute" => act(ctx, "mute", ""),
        "Speaker on" | "Speaker off" => act(ctx, "speaker", ""),
        "Hold" | "Resume" => act(ctx, "hold", ""),
        "End all calls" => {
            let ids: Vec<u8> = {
                let m = ctx.m.borrow();
                m.status
                    .iter()
                    .flat_map(|s| s.calls.as_slice().iter().filter(|c| calls::shown(c)))
                    .map(|c| c.id)
                    .collect()
            };
            for id in ids {
                let r = ctx.m.borrow_mut().modem.hangup(id);
                if r < 0 {
                    ctx.note(&format!("Action failed ({r})"));
                }
            }
        }
        _ => {}
    }
}

pub fn action(_ctx: &Ctx) {}

/// Back leaves the call running (the UI server goes home; the status bar shows the call).
pub fn back(_ctx: &Ctx) -> bool {
    false
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
    let (u, restore, summary) = {
        let mut m = ctx.m.borrow_mut();
        let Some(s) = m.status else { return };
        let u = m.call.calls.update(&s, now);
        let restore = if s.calls.as_slice().iter().any(calls::ringing) {
            None
        } else {
            m.call.silenced.take()
        };
        if u.all_ended {
            m.call.keypad = false;
            m.call.tones.clear();
        }
        let ended = m.call.calls.ended.map(|e| e.call.id);
        m.call.who.retain(|(id, ..)| {
            Some(*id) == ended || s.calls.as_slice().iter().any(|c| c.id == *id)
        });
        let summary = m.prefs.summary && m.call.calls.ended.is_some_and(|e| e.summary());
        (u, restore, summary)
    };
    if let Some(r) = restore {
        let _ = ctx.m.borrow_mut().modem.ring(r.volume, r.silent, r.vibrate);
    }
    if u.appeared && ctx.page() != Page::Call {
        ctx.go(Page::Call);
    }
    // the summary shows only over the call screens (the user may be in Add call)
    if u.all_ended && (!summary || ctx.page() != Page::Call) {
        close(ctx);
    }
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
