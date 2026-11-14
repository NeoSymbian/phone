// SPDX-License-Identifier: GPL-3.0-only
//! The call screens: incoming, calling, active, waiting, two calls, the keypad, the summary (to be written).

use crate::ctx::{Chrome, Ctx};
use alloc::rc::Rc;
use alloc::vec::Vec;
use phone_ui::{CallState, PhoneApp};
use slint::ComponentHandle;

#[derive(Default)]
pub struct State {}

pub fn bind(ctx: &Rc<Ctx>, ui: &PhoneApp) {
    let ctx = Rc::downgrade(ctx);
    ui.global::<CallState>().on_act(move |_action, _value| {
        let Some(_ctx) = ctx.upgrade() else { return };
    });
}

pub fn show(_ctx: &Ctx) {}

pub fn chrome(_ctx: &Ctx) -> Chrome {
    Chrome::titled("Call")
}

pub fn options(_ctx: &Ctx) -> Vec<&'static str> {
    Vec::new()
}

pub fn option(_ctx: &Ctx, _item: &str) {}

pub fn action(_ctx: &Ctx) {}

pub fn back(_ctx: &Ctx) -> bool {
    false
}

/// MODEMD's status changed: open or close the call screens.
pub fn on_status(_ctx: &Ctx) {}

pub fn tick(_ctx: &Ctx) {}

pub fn next_wake(_ctx: &Ctx) -> Option<u64> {
    None
}
