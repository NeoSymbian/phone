// SPDX-License-Identifier: GPL-3.0-only
//! Settings: Phone & Contacts, Blocked numbers (to be written).

use crate::ctx::{Chrome, Ctx};
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use neoui_proto::settings::{Bytes, keys};
use phone_ui::{PhoneApp, SettingsState};
use slint::ComponentHandle;

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

pub fn bind(ctx: &Rc<Ctx>, ui: &PhoneApp) {
    let ctx = Rc::downgrade(ctx);
    ui.global::<SettingsState>().on_act(move |_action, _value| {
        let Some(_ctx) = ctx.upgrade() else { return };
    });
}

pub fn show(_ctx: &Ctx) {}

pub fn chrome(_ctx: &Ctx) -> Chrome {
    Chrome::titled("Phone & Contacts")
}

pub fn options(_ctx: &Ctx) -> Vec<&'static str> {
    Vec::new()
}

pub fn option(_ctx: &Ctx, _item: &str) {}

pub fn action(_ctx: &Ctx) {}

pub fn back(_ctx: &Ctx) -> bool {
    false
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

/// MODEMD's status changed (call waiting's answer).
pub fn on_status(_ctx: &Ctx) {}

/// Block `number` (calls and/or messages) in MODEMD's blocklist, with a note.
pub fn block_number(_ctx: &Ctx, _number: &str, _calls: bool, _messages: bool) {}

/// Remove every rule matching `number` exactly (Unblock on a card or a Recents row).
pub fn unblock_number(_ctx: &Ctx, _number: &str) {}

/// What blocks `number` (calls, messages), if a number rule does.
pub fn blocked(_ctx: &Ctx, _number: &str) -> Option<(bool, bool)> {
    None
}
