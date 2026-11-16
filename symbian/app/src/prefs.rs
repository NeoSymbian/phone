// SPDX-License-Identifier: GPL-3.0-only
//! Settings: Phone & Contacts (caller ID, call waiting and the message settings go to MODEMD, the rest are
//! `phone.*`/`contacts.*` settings), Quick replies (a file of ours, crate::store), Blocked numbers (MODEMD's blocklist, edited through [`phone_core::blocking`]), and the blocking calls
//! the other areas make.
//!
//! The screens keep their own open/closed state in `SettingsState` (the picker, the voicemail editor, the add row,
//! the Remove all query); this module reads it for the toolbar and Back.

use crate::ctx::{Chrome, Ctx, model, strings};
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use contacts::{Label, NameOrder};
use neoui_proto::ipc as proto;
use neoui_proto::settings::{Bytes, Key, Setting, keys};
use phone_core::blocking::{self, Refusal};
use phone_core::page::{Page, SettingsView};
use phone_rules::block::List;
use phone_ui::{BlockRow, KeyboardFocus, PhoneApp, SettingsState};
use slint::{Color, ComponentHandle, SharedString};

/// The `phone.*` and `contacts.*` settings every area reads (owned by this app since neoui 0bd19fa).
pub struct State {
    /// modem_ipc::clir: 0 network, 1 hide, 2 show
    pub clir: i32,
    pub show_duration: bool,
    pub summary: bool,
    /// days the call log keeps calls: 0, 1, 10, 30
    pub log_days: i32,
    pub smart_dial: bool,
    pub voicemail: String,
    /// the numbers of speed-dial keys 2..=9 ("" = unassigned)
    pub speed: [String; 8],
    /// the newest call-log id seen in Recents (missed calls after it are new)
    pub missed_seen: u32,
    /// the number type a new contact's first number gets (contacts::Label as u8)
    pub default_label: u8,
}

impl Default for State {
    fn default() -> Self {
        Self {
            clir: 0,
            show_duration: false,
            summary: false,
            log_days: 30,
            smart_dial: true,
            voicemail: String::new(),
            speed: Default::default(),
            missed_seen: 0,
            default_label: 0,
        }
    }
}

/// The days "Keep call log" offers, in the picker's order.
const LOG_DAYS: [i32; 4] = [0, 1, 10, 30];

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
            close(ctx);
            let _ = ctx.m.borrow_mut().modem.waiting(None);
            // the message centre may have come from the SIM since
            crate::msgs::load(ctx);
            return;
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
        "summary" => save(ctx, keys::PHONE_SUMMARY, on),
        "duration" => save(ctx, keys::PHONE_SHOW_DURATION, on),
        "smart-dial" => save(ctx, keys::PHONE_SMART_DIAL, on),
        "waiting" => {
            let r = ctx.m.borrow_mut().modem.waiting(Some(on));
            if r != 0 {
                ctx.note(&format!("Call waiting: not available ({r})"));
            }
        }
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
    match crate::store::save_replies(&replies) {
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

/// Edit a copy of the blocklist and send it to MODEMD; the model keeps what MODEMD has (re-read after a failure).
/// `done` is the note on success ("" none). False when refused or not sent.
fn edit_block(ctx: &Ctx, edit: impl FnOnce(&mut List) -> Result<(), Refusal>, done: &str) -> bool {
    let mut list = ctx.m.borrow().block.clone();
    if let Err(r) = edit(&mut list) {
        ctx.note(r.text());
        return false;
    }
    let r = ctx.m.borrow_mut().modem.set_block_list(&list);
    if r != 0 {
        let now = ctx.m.borrow_mut().modem.block_list();
        if let Ok(l) = now {
            ctx.m.borrow_mut().block = l;
        }
        ctx.note(&format!("Not saved ({r})"));
        return false;
    }
    ctx.m.borrow_mut().block = list;
    if !done.is_empty() {
        ctx.note(done);
    }
    true
}

/// "Ani Petrosyan" -> "AP".
fn initials(name: &str) -> String {
    name.split_whitespace()
        .take(2)
        .filter_map(|w| w.chars().next())
        .flat_map(char::to_uppercase)
        .collect()
}

// shortcut: a fixed palette by contact id (the mockups' colours); use the Contacts area's avatar colours once it has
// them.
const TINTS: [u32; 5] = [0x22b46a, 0xd6409f, 0xe8833a, 0x8b5cf6, 0x14a3a3];

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
                    let t = TINTS[c.id as usize % TINTS.len()];
                    BlockRow {
                        initials: initials(&name).into(),
                        title: name.into(),
                        subtitle: format!("{} · {what}", r.number).into(),
                        tint: Color::from_rgb_u8((t >> 16) as u8, (t >> 8) as u8, t as u8),
                        kind: 0,
                    }
                }
                None => BlockRow {
                    title: blocking::title(r).into(),
                    subtitle: what.into(),
                    initials: SharedString::new(),
                    tint: Color::default(),
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
    let m = ctx.m.borrow();
    let p = &m.prefs;
    s.set_clir(p.clir.clamp(0, 2));
    s.set_waiting(m.status.as_ref().map_or(0, |st| i32::from(st.waiting)));
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
    let page = ctx.page();
    if matches!(
        page,
        Page::Settings(SettingsView::Blocked | SettingsView::Replies)
    ) {
        let c = Chrome::titled(if page == Page::Settings(SettingsView::Replies) {
            "Quick replies"
        } else {
            "Blocked numbers"
        });
        let full = page == Page::Settings(SettingsView::Replies)
            && ctx.m.borrow().msgs.replies.len() >= crate::store::MAX_REPLIES;
        if s.get_adding() {
            c.with(proto::TOOL_CHECK, proto::TOOLBAR_CANCEL, "Add")
        } else if full {
            c
        } else {
            c.with(proto::TOOL_ADD, 0, "Add")
        }
    } else {
        let c = Chrome::titled("Phone & Contacts");
        if s.get_editing() || s.get_editing_smsc() {
            c.with(proto::TOOL_CHECK, proto::TOOLBAR_CANCEL, "Save")
        } else {
            c
        }
    }
}

pub fn options(ctx: &Ctx) -> Vec<&'static str> {
    let blocked = ctx.page() == Page::Settings(SettingsView::Blocked);
    if blocked && !ctx.m.borrow().block.rules.is_empty() {
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

/// The toolbar's middle button: Save (the voicemail number), Add (open the add row), Add (the typed number).
pub fn action(ctx: &Ctx) {
    let ui = ctx.ui();
    let s = ui.global::<SettingsState>();
    if ctx.page() == Page::Settings(SettingsView::Replies) {
        if !s.get_adding() {
            s.set_add_text(SharedString::new());
            s.set_adding(true);
            return;
        }
        let text = String::from(s.get_add_text().trim());
        if !text.is_empty() {
            let mut replies = ctx.m.borrow().msgs.replies.clone();
            replies.truncate(crate::store::MAX_REPLIES - 1);
            replies.push(text.replace('\n', " "));
            save_replies(ctx, replies);
        }
        close(ctx);
    } else if s.get_editing_smsc() {
        let v = String::from(s.get_draft().trim());
        if v.is_empty() || !v.chars().all(|c| c.is_ascii_digit() || " +-".contains(c)) {
            ctx.note("Use digits and +");
            return;
        }
        let mut sms = ctx.m.borrow().msgs.settings;
        sms.smsc = modem_ipc::Text::new(&v);
        send_sms_settings(ctx, sms);
        close(ctx);
        ctx.note("Saved");
    } else if ctx.page() == Page::Settings(SettingsView::Blocked) {
        if !s.get_adding() {
            s.set_add_text(SharedString::new());
            s.set_add_prefix(false);
            s.set_adding(true);
            return;
        }
        let text = s.get_add_text();
        let prefix = s.get_add_prefix();
        let done = format!("Blocked {}", text.trim());
        if edit_block(ctx, |l| blocking::add(l, &text, prefix, true, true), &done) {
            close(ctx);
        }
    } else if s.get_editing() {
        let v = s.get_draft();
        let v = v.trim();
        if !v
            .chars()
            .all(|c| c.is_ascii_digit() || " +*#pw-".contains(c))
        {
            ctx.note("Use digits and + * #");
            return;
        }
        match ctx.app.set_setting(keys::PHONE_VOICEMAIL, String::from(v)) {
            Ok(()) => {
                close(ctx);
                ctx.note("Saved");
            }
            Err(e) => ctx.note(&format!("Not saved ({e})")),
        }
        reload(ctx);
    }
    ctx.show();
}

/// Close the voicemail editor and the add row (and the keyboard they opened).
fn close(ctx: &Ctx) {
    let ui = ctx.ui();
    let s = ui.global::<SettingsState>();
    s.set_editing(false);
    s.set_editing_smsc(false);
    s.set_adding(false);
    ui.global::<KeyboardFocus>().set_open(false);
}

/// Remove all, asked with the UI server's dialog.
pub fn query(ctx: &Ctx) -> Option<neoapp::Query> {
    ctx.ui().global::<SettingsState>().get_query().then(|| {
        crate::ctx::ask(
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

/// Back closes a picker, the query, the editor or the add row first.
pub fn back(ctx: &Ctx) -> bool {
    let ui = ctx.ui();
    let s = ui.global::<SettingsState>();
    if !s.get_picker().is_empty() {
        s.set_picker(SharedString::new());
    } else if s.get_query() {
        s.set_query(false);
    } else if s.get_editing() || s.get_editing_smsc() || s.get_adding() {
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
    !s.get_picker().is_empty()
        || s.get_query()
        || s.get_editing()
        || s.get_editing_smsc()
        || s.get_adding()
}

/// Read the settings (`phone.*`, `contacts.*`) into the model.
pub fn reload(ctx: &Ctx) {
    let a = &ctx.app;
    let d = State::default();
    let speed = a
        .setting(keys::PHONE_SPEED)
        .map(|b| b.0)
        .unwrap_or_default();
    let mut cells: [String; 8] = Default::default();
    for (i, n) in String::from_utf8_lossy(&speed)
        .split('\n')
        .take(8)
        .enumerate()
    {
        cells[i] = n.into();
    }
    let state = State {
        clir: a.setting(keys::PHONE_CLIR).unwrap_or(d.clir),
        show_duration: a
            .setting(keys::PHONE_SHOW_DURATION)
            .unwrap_or(d.show_duration),
        summary: a.setting(keys::PHONE_SUMMARY).unwrap_or(d.summary),
        log_days: a.setting(keys::PHONE_LOG_DAYS).unwrap_or(d.log_days),
        smart_dial: a.setting(keys::PHONE_SMART_DIAL).unwrap_or(d.smart_dial),
        voicemail: a.setting(keys::PHONE_VOICEMAIL).unwrap_or_default(),
        speed: cells,
        missed_seen: a.setting(keys::PHONE_MISSED_SEEN).unwrap_or(0).max(0) as u32,
        default_label: a
            .setting(keys::CONTACTS_DEFAULT_LABEL)
            .unwrap_or(0)
            .clamp(0, 15) as u8,
    };
    let order =
        contacts::NameOrder::from_setting(a.setting(keys::CONTACTS_NAME_ORDER).unwrap_or(0));
    let mut m = ctx.m.borrow_mut();
    m.prefs = state;
    m.order = order;
}

/// Save speed-dial keys 2..=9 (`phone.speed`).
pub fn save_speed(ctx: &Ctx, speed: &[String; 8]) -> Result<(), i32> {
    let blob = speed.join("\n").into_bytes();
    ctx.app.set_setting(keys::PHONE_SPEED, Bytes(blob))?;
    ctx.m.borrow_mut().prefs.speed = speed.clone();
    Ok(())
}

/// MODEMD's status changed: the Call waiting row follows the network's answer.
pub fn on_status(ctx: &Ctx) {
    let w = ctx
        .m
        .borrow()
        .status
        .as_ref()
        .map_or(0, |s| i32::from(s.waiting));
    ctx.ui().global::<SettingsState>().set_waiting(w);
}

/// Block `number` (calls and/or messages) in MODEMD's blocklist, with a note.
pub fn block_number(ctx: &Ctx, number: &str, calls: bool, messages: bool) {
    edit_block(
        ctx,
        |l| blocking::block(l, number, calls, messages),
        &format!("Blocked {number}"),
    );
}

/// Remove every rule matching `number` exactly (Unblock on a card or a Recents row).
pub fn unblock_number(ctx: &Ctx, number: &str) {
    if blocked(ctx, number).is_some() {
        edit_block(
            ctx,
            |l| {
                blocking::remove_number(l, number);
                Ok(())
            },
            "Unblocked",
        );
    }
}

/// What blocks `number` (calls, messages), if a number rule does.
pub fn blocked(ctx: &Ctx, number: &str) -> Option<(bool, bool)> {
    blocking::lookup(&ctx.m.borrow().block, number)
}
