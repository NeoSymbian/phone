// SPDX-License-Identifier: GPL-3.0-only
//! Settings: Phone & Contacts, Blocked numbers (to be written).

use crate::ctx::{Chrome, Ctx};
use alloc::rc::Rc;
use alloc::vec::Vec;
use phone_ui::{PhoneApp, SettingsState};
use slint::ComponentHandle;

#[derive(Default)]
pub struct State {}

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
pub fn reload(_ctx: &Ctx) {}

/// MODEMD's status changed (call waiting's answer).
pub fn on_status(_ctx: &Ctx) {}
