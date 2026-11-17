// SPDX-License-Identifier: GPL-3.0-only
//! The `phone.*` and `contacts.*` settings the three apps read (Phone's Settings page writes most; a contact card
//! writes speed dial). Each app watches `phone.` and `contacts.` and reads them again on `AppEvent::Settings`.

use alloc::string::String;
use contacts::NameOrder;
use neoui_proto::settings::{Bytes, keys};

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

/// Read the settings (and the name order).
pub fn read(a: &neoapp::App) -> (State, NameOrder) {
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
    let order = NameOrder::from_setting(a.setting(keys::CONTACTS_NAME_ORDER).unwrap_or(0));
    (state, order)
}

/// Save speed-dial keys 2..=9 (`phone.speed`).
pub fn save_speed(a: &neoapp::App, speed: &[String; 8]) -> Result<(), i32> {
    a.set_setting(keys::PHONE_SPEED, Bytes(speed.join("\n").into_bytes()))
}
