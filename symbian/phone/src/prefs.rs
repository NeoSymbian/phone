// SPDX-License-Identifier: GPL-3.0-only
//! Settings (NEOPHONE v2 p2-phone-settings, p2-msg-settings): Calls, Log, Contacts and Messages on one page (caller
//! ID, call waiting and the message settings go to MODEMD, the rest are `phone.*`/`contacts.*` settings), opened at a
//! section by Messages and Contacts; Quick replies (a file of ours, crate::store); Blocked numbers (MODEMD's
//! blocklist, edited through [`phone_core::blocking`]); the forms under [✕ title ✓]; and the blocking calls the
//! other areas make.
//!
//! The screens keep their own open/closed state in `SettingsState` (the picker, the form, the Remove all query); this
//! module reads it for the header and Back.

use crate::ctx::Ctx;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use contacts::{Label, NameOrder};
use neoui_proto::settings::{Key, Setting, keys};
use phone_common::ui::{Chrome, model, strings, tool};
use phone_core::blocking::{self, Refusal};
use phone_core::page::{Page, SettingsView};
use phone_core::{calls, recents};
use phone_rules::block::List;
use phone_ui::{BlockRow, KeyboardFocus, PhoneApp, SettingsState};
use slint::{ComponentHandle, SharedString};

/// The `phone.*` and `contacts.*` settings (read by all three apps: phone_common::prefs).
pub use phone_common::prefs::State;

/// The days "Keep call log" offers, in the picker's order.
const LOG_DAYS: [i32; 4] = [0, 1, 10, 30];
/// How long Call waiting shows "Checking with network…" before "Couldn't check".
const WAITING_MS: u64 = 15_000;

pub fn bind(ctx: &Rc<Ctx>, ui: &PhoneApp) {
    let ctx = Rc::downgrade(ctx);
    ui.global::<SettingsState>().on_act(move |action, value| {
        let Some(ctx) = ctx.upgrade() else { return };
        act(&ctx, &action, &value);
    });
}

fn act(ctx: &Ctx, action: &str, value: &str) {
    let on = value == "1";
    let i: usize = value.parse().unwrap_or(0);
    match action {
        // the main view appeared (while it is drawn): nothing left open from the last visit, and ask the network
        // about call waiting (the answer comes as Status::waiting)
        "opened" => {
            let ui = ctx.ui();
            let s = ui.global::<SettingsState>();
            s.set_picker(SharedString::new());
            s.set_query(false);
            ask_waiting(ctx, None);
            // the message centre may have come from the SIM since
            crate::ctx::load_sms(ctx);
        }
        "clir" => {
            save(ctx, keys::PHONE_CLIR, i.min(2) as i32);
            send_call_settings(ctx);
        }
        "log" => {
            save(ctx, keys::PHONE_LOG_DAYS, LOG_DAYS[i.min(3)]);
            send_call_settings(ctx);
        }
        "order" => save(ctx, keys::CONTACTS_NAME_ORDER, i.min(1) as i32),
        "label" => save(
            ctx,
            keys::CONTACTS_DEFAULT_LABEL,
            i.min(Label::ALL.len() - 1) as i32,
        ),
        // one switch on the v2 page: the summary shows the call's duration
        "summary" => {
            save(ctx, keys::PHONE_SUMMARY, on);
            save(ctx, keys::PHONE_SHOW_DURATION, on);
        }
        "form" => return open_form(ctx, value),
        "smart-dial" => save(ctx, keys::PHONE_SMART_DIAL, on),
        "waiting" => ask_waiting(ctx, Some(on)),
        "waiting-check" => ask_waiting(ctx, None),
        "blocked" => ctx.go(Page::Settings(SettingsView::Blocked)),
        "replies" => ctx.go(Page::Settings(SettingsView::Replies)),
        "reports" | "alert" => {
            let mut s = ctx.m.borrow().msgs.settings;
            if action == "reports" {
                s.reports = on;
            } else {
                s.alert = i.min(2) as u8;
            }
            send_sms_settings(ctx, s);
        }
        "remove-reply" => {
            let mut replies = ctx.m.borrow().msgs.replies.clone();
            if i < replies.len() {
                replies.remove(i);
                save_replies(ctx, replies);
            }
        }
        "private" | "unknown" => {
            edit_block(
                ctx,
                |l| {
                    *(if action == "private" {
                        &mut l.private
                    } else {
                        &mut l.unknown
                    }) = on;
                    Ok(())
                },
                "",
            );
        }
        "remove" => {
            edit_block(
                ctx,
                |l| {
                    if i < l.rules.len() {
                        l.rules.remove(i);
                    }
                    Ok(())
                },
                "Unblocked",
            );
        }
        "remove-all" => {
            edit_block(
                ctx,
                |l| {
                    l.rules.clear();
                    Ok(())
                },
                "Removed all",
            );
        }
        _ => {}
    }
    ctx.show();
}

/// Ask the network about call waiting (or set it): the row shows "Checking…" until the answer comes as
/// `Status::waiting`, or "Couldn't check" after [`WAITING_MS`].
fn ask_waiting(ctx: &Ctx, set: Option<bool>) {
    let r = ctx.m.borrow_mut().modem.waiting(set);
    let now = e32::time::millis();
    // not sent: "Couldn't check" at once
    ctx.m.borrow_mut().waiting_until = Some(if r == 0 { now + WAITING_MS } else { now });
}

/// Call waiting as the row shows it (modem_ipc::waiting, 1 while asking, 4 when no answer came in time).
fn waiting_shown(ctx: &Ctx) -> i32 {
    let m = ctx.m.borrow();
    let w = m.status.as_ref().map_or(0, |s| i32::from(s.waiting));
    match m.waiting_until {
        Some(t) if matches!(w, 0 | 1) => {
            if e32::time::millis() < t {
                1
            } else {
                4
            }
        }
        _ => w,
    }
}

/// Write a setting and read them all back (a note if the UI server refused it).
fn save<T: Setting>(ctx: &Ctx, key: Key<T>, value: T) {
    if let Err(e) = ctx.app.set_setting(key, value) {
        ctx.note(&format!("Not saved ({e})"));
    }
    reload(ctx);
}

/// The message settings go to MODEMD (it keeps them); the model keeps what it took.
fn send_sms_settings(ctx: &Ctx, s: modem_ipc::SmsSettings) {
    let r = ctx.m.borrow_mut().modem.set_sms_settings(&s);
    if r != 0 {
        ctx.note(&format!("Not saved ({r})"));
        return;
    }
    ctx.m.borrow_mut().msgs.settings = s;
}

fn save_replies(ctx: &Ctx, replies: Vec<String>) {
    match phone_common::store::save_replies(&replies) {
        Ok(()) => ctx.m.borrow_mut().msgs.replies = replies,
        Err(e) => ctx.note(&format!("Not saved ({e})")),
    }
}

/// Caller ID and the log's days go to MODEMD as they change.
fn send_call_settings(ctx: &Ctx) {
    let r = {
        let mut m = ctx.m.borrow_mut();
        let (clir, days) = (m.prefs.clir, m.prefs.log_days);
        m.modem.call_settings(clir, days)
    };
    if r != 0 {
        ctx.note(&format!("Saved; the modem didn't take it ({r})"));
    }
}

/// Edit a copy of the blocklist and send it to MODEMD (phone_common::block); `done` is the note on success ("" none).
/// False when refused or not sent.
fn edit_block(ctx: &Ctx, edit: impl FnOnce(&mut List) -> Result<(), Refusal>, done: &str) -> bool {
    let r = {
        let mut m = ctx.m.borrow_mut();
        let m = &mut *m;
        phone_common::block::edit(&mut m.modem, &mut m.block, edit, done)
    };
    note_result(ctx, r)
}

/// Show a blocklist edit's note; true when it was saved.
fn note_result(ctx: &Ctx, r: Result<String, String>) -> bool {
    match r {
        Ok(n) => {
            if !n.is_empty() {
                ctx.note(&n);
            }
            true
        }
        Err(n) => {
            ctx.note(&n);
            false
        }
    }
}

fn rows(ctx: &Ctx) -> Vec<BlockRow> {
    let m = ctx.m.borrow();
    m.block
        .rules
        .iter()
        .map(|r| {
            let what = blocking::summary(r);
            let owner = if r.prefix {
                None
            } else {
                m.book.lookup(&r.number)
            };
            match owner {
                Some(c) => {
                    let name = c.title(m.order);
                    BlockRow {
                        initials: recents::initials(&name).into(),
                        title: name.into(),
                        subtitle: format!("{} · {what}", r.number).into(),
                        tint: recents::tint(Some(c.id), &r.number, calls::TINTS.len() as u32),
                        kind: 0,
                    }
                }
                None => BlockRow {
                    title: blocking::title(r).into(),
                    subtitle: what.into(),
                    initials: SharedString::new(),
                    tint: -1,
                    kind: if r.prefix { 2 } else { 1 },
                },
            }
        })
        .collect()
}

pub fn show(ctx: &Ctx) {
    let ui = ctx.ui();
    let s = ui.global::<SettingsState>();
    let rows = rows(ctx);
    s.set_waiting(waiting_shown(ctx));
    let m = ctx.m.borrow();
    let p = &m.prefs;
    s.set_clir(p.clir.clamp(0, 2));
    s.set_summary(p.summary);
    s.set_show_duration(p.show_duration);
    s.set_voicemail(p.voicemail.as_str().into());
    s.set_blocked_count(m.block.rules.len() as i32);
    s.set_log_days(p.log_days);
    s.set_name_order(i32::from(m.order == NameOrder::LastFirst));
    s.set_smart_dial(p.smart_dial);
    let labels: Vec<&str> = Label::ALL.iter().map(|l| l.text()).collect();
    s.set_labels(strings(&labels));
    s.set_default_label(i32::from(p.default_label).min(labels.len() as i32 - 1));
    s.set_block_private(m.block.private);
    s.set_block_unknown(m.block.unknown);
    s.set_rules(model(rows));
    let sms = &m.msgs.settings;
    s.set_sms_reports(sms.reports);
    s.set_sms_alert(i32::from(sms.alert.min(2)));
    s.set_smsc(sms.smsc.as_str().into());
    let replies: Vec<&str> = m.msgs.replies.iter().map(String::as_str).collect();
    s.set_replies(strings(&replies));
}

pub fn chrome(ctx: &Ctx) -> Chrome {
    let ui = ctx.ui();
    let s = ui.global::<SettingsState>();
    let form = s.get_form();
    if !form.is_empty() {
        let title = match form.as_str() {
            "voicemail" => "Voicemail number",
            "smsc" => "Message centre",
            "block" => "Block a number",
            _ => "New quick reply",
        };
        return Chrome::titled(title).with(tool::CHECK, tool::CANCEL, "Save");
    }
    match ctx.page() {
        Page::Settings(SettingsView::Replies) => {
            let c = Chrome::titled("Quick replies");
            if ctx.m.borrow().msgs.replies.len() >= crate::store::MAX_REPLIES {
                c
            } else {
                c.with(tool::ADD, 0, "Add")
            }
        }
        Page::Settings(SettingsView::Blocked) => {
            Chrome::titled("Blocked numbers").with(tool::ADD, 0, "Add")
        }
        // one name from every route (the system Settings row is "Calls & Messages")
        _ => Chrome::titled("Settings"),
    }
}

pub fn options(ctx: &Ctx) -> Vec<&'static str> {
    let blocked = ctx.page() == Page::Settings(SettingsView::Blocked);
    let form = !ctx.ui().global::<SettingsState>().get_form().is_empty();
    if blocked && !form && !ctx.m.borrow().block.rules.is_empty() {
        alloc::vec!["Remove all"]
    } else {
        Vec::new()
    }
}

pub fn option(ctx: &Ctx, item: &str) {
    if item == "Remove all" {
        ctx.ui().global::<SettingsState>().set_query(true);
    }
}

/// Settings at `section` ("messages", "contacts"; "" the top).
pub fn open_at(ctx: &Ctx, section: &str) {
    let ui = ctx.ui();
    let s = ui.global::<SettingsState>();
    // "blocked": straight to Blocked numbers (a blocked message's note), with Messages' page under it
    let blocked = section == "blocked";
    s.set_section(if blocked { "messages" } else { section }.into());
    s.set_opens(s.get_opens().wrapping_add(1));
    s.set_form(SharedString::new());
    ctx.go(Page::Settings(SettingsView::Main));
    if blocked {
        ctx.go(Page::Settings(SettingsView::Blocked));
    }
}

/// Open a form: "voicemail" and "smsc" start from the number set, "block" and "reply" empty.
pub fn open_form(ctx: &Ctx, form: &str) {
    let draft = {
        let m = ctx.m.borrow();
        match form {
            "voicemail" => m.prefs.voicemail.clone(),
            "smsc" => String::from(m.msgs.settings.smsc.as_str()),
            _ => String::new(),
        }
    };
    let ui = ctx.ui();
    let s = ui.global::<SettingsState>();
    s.set_draft(draft.into());
    s.set_add_prefix(false);
    s.set_form(form.into());
}

/// The form's ✓, or the header's + (Blocked numbers, Quick replies: their form).
pub fn action(ctx: &Ctx) {
    let ui = ctx.ui();
    let s = ui.global::<SettingsState>();
    let draft = String::from(s.get_draft().trim());
    match s.get_form().as_str() {
        "" => match ctx.page() {
            Page::Settings(SettingsView::Replies) => open_form(ctx, "reply"),
            Page::Settings(SettingsView::Blocked) => open_form(ctx, "block"),
            _ => {}
        },
        "reply" => {
            if !draft.is_empty() {
                let mut replies = ctx.m.borrow().msgs.replies.clone();
                replies.truncate(crate::store::MAX_REPLIES - 1);
                replies.push(draft.replace('\n', " "));
                save_replies(ctx, replies);
            }
            close(ctx);
        }
        "smsc" => {
            if draft.is_empty()
                || !draft
                    .chars()
                    .all(|c| c.is_ascii_digit() || " +-".contains(c))
            {
                ctx.note("Use digits and +");
                return;
            }
            let mut sms = ctx.m.borrow().msgs.settings;
            sms.smsc = modem_ipc::Text::new(&draft);
            send_sms_settings(ctx, sms);
            close(ctx);
            ctx.note("Saved");
        }
        "block" => {
            let prefix = s.get_add_prefix();
            let done = format!("Blocked {draft}");
            if edit_block(ctx, |l| blocking::add(l, &draft, prefix, true, true), &done) {
                close(ctx);
            }
        }
        _ => {
            if !draft
                .chars()
                .all(|c| c.is_ascii_digit() || " +*#pw-".contains(c))
            {
                ctx.note("Use digits and + * #");
                return;
            }
            match ctx.app.set_setting(keys::PHONE_VOICEMAIL, draft) {
                Ok(()) => {
                    close(ctx);
                    ctx.note("Saved");
                }
                Err(e) => ctx.note(&format!("Not saved ({e})")),
            }
            reload(ctx);
        }
    }
    ctx.show();
}

/// Close the form (and the keyboard it opened).
fn close(ctx: &Ctx) {
    let ui = ctx.ui();
    ui.global::<SettingsState>().set_form(SharedString::new());
    ui.global::<KeyboardFocus>().set_open(false);
}

/// Remove all, asked with the UI server's dialog.
pub fn query(ctx: &Ctx) -> Option<neoapp::Query> {
    ctx.ui().global::<SettingsState>().get_query().then(|| {
        phone_common::ui::ask(
            "Remove all blocked numbers?",
            "Calls and messages from them come through again.",
            "Remove all",
        )
    })
}

pub fn answer(ctx: &Ctx, yes: bool) {
    ctx.ui().global::<SettingsState>().set_query(false);
    if yes {
        act(ctx, "remove-all", "");
    } else {
        ctx.show();
    }
}

/// Back closes a picker, the query or the form first.
pub fn back(ctx: &Ctx) -> bool {
    let ui = ctx.ui();
    let s = ui.global::<SettingsState>();
    if !s.get_picker().is_empty() {
        s.set_picker(SharedString::new());
    } else if s.get_query() {
        s.set_query(false);
    } else if !s.get_form().is_empty() {
        close(ctx);
    } else {
        return false;
    }
    true
}

/// Something is open that Back closes (Back is claimed even on the main view).
pub fn wants_back(ctx: &Ctx) -> bool {
    let ui = ctx.ui();
    let s = ui.global::<SettingsState>();
    !s.get_picker().is_empty() || s.get_query() || !s.get_form().is_empty()
}

/// Read the settings (`phone.*`, `contacts.*`) into the model.
pub fn reload(ctx: &Ctx) {
    // Summary after call is on until it is switched off (neoui's registry default)
    let (state, order) = phone_common::prefs::read(&ctx.app);
    let mut m = ctx.m.borrow_mut();
    m.prefs = state;
    m.order = order;
}

/// Save speed-dial keys 2..=9 (`phone.speed`).
pub fn save_speed(ctx: &Ctx, speed: &[String; 8]) -> Result<(), i32> {
    phone_common::prefs::save_speed(&ctx.app, speed)?;
    ctx.m.borrow_mut().prefs.speed = speed.clone();
    Ok(())
}

/// MODEMD's status changed: the Call waiting row follows the network's answer (an answer ends the wait; MODEMD
/// asking on its own starts one).
pub fn on_status(ctx: &Ctx) {
    {
        let mut m = ctx.m.borrow_mut();
        let w = m.status.as_ref().map_or(0, |s| s.waiting);
        match w {
            1 if m.waiting_until.is_none() => {
                m.waiting_until = Some(e32::time::millis() + WAITING_MS)
            }
            2..=4 => m.waiting_until = None,
            _ => {}
        }
    }
    ctx.ui()
        .global::<SettingsState>()
        .set_waiting(waiting_shown(ctx));
}

/// The Call waiting check's deadline, while Settings shows.
pub fn next_wake(ctx: &Ctx) -> Option<u64> {
    let m = ctx.m.borrow();
    let t = m.waiting_until?;
    (matches!(m.page, Page::Settings(_)) && t > e32::time::millis()).then_some(t)
}

pub fn tick(ctx: &Ctx) {
    if let Page::Settings(_) = ctx.page() {
        ctx.ui()
            .global::<SettingsState>()
            .set_waiting(waiting_shown(ctx));
    }
}

/// Block `number` (calls and/or messages) in MODEMD's blocklist, with a note.
pub fn block_number(ctx: &Ctx, number: &str, calls: bool, messages: bool) {
    let r = {
        let mut m = ctx.m.borrow_mut();
        let m = &mut *m;
        phone_common::block::block_number(&mut m.modem, &mut m.block, number, calls, messages)
    };
    note_result(ctx, r);
}

/// Remove every rule matching `number` exactly (Unblock on a Recents row).
pub fn unblock_number(ctx: &Ctx, number: &str) {
    let r = {
        let mut m = ctx.m.borrow_mut();
        let m = &mut *m;
        phone_common::block::unblock_number(&mut m.modem, &mut m.block, number)
    };
    note_result(ctx, r);
}

/// What blocks `number` (calls, messages), if a number rule does.
pub fn blocked(ctx: &Ctx, number: &str) -> Option<(bool, bool)> {
    phone_common::block::blocked(&ctx.m.borrow().block, number)
}
