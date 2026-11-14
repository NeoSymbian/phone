// SPDX-License-Identifier: GPL-3.0-only
//! The Phone page: the dialer, Recents, call details, speed dial (to be written).

use crate::ctx::{Chrome, Ctx};
use alloc::rc::Rc;
use alloc::vec::Vec;
use phone_ui::{DialState, PhoneApp};
use slint::ComponentHandle;

#[derive(Default)]
pub struct State {}

pub fn bind(ctx: &Rc<Ctx>, ui: &PhoneApp) {
    let ctx = Rc::downgrade(ctx);
    ui.global::<DialState>().on_act(move |_action, _value| {
        let Some(_ctx) = ctx.upgrade() else { return };
    });
}

pub fn show(_ctx: &Ctx) {}

pub fn chrome(_ctx: &Ctx) -> Chrome {
    Chrome::titled("Phone")
}

pub fn options(_ctx: &Ctx) -> Vec<&'static str> {
    Vec::new()
}

pub fn option(_ctx: &Ctx, _item: &str) {}

pub fn action(_ctx: &Ctx) {}

pub fn back(_ctx: &Ctx) -> bool {
    false
}

pub fn wants_back(_ctx: &Ctx) -> bool {
    false
}

pub fn tick(_ctx: &Ctx) {}

/// When the dialer needs the next tick (ms, `e32::time::millis`).
pub fn next_wake(_ctx: &Ctx) -> Option<u64> {
    None
}

/// Call `number` from anywhere in the app (a contact's number, a Recents row, speed dial): dial strings and service
/// codes go through `phone_rules::dial::classify`, a number is dialled with the caller-ID setting, and the call
/// screens open.
// shortcut: the skeleton's version dials directly; the Phone area replaces it with the full rules.
pub fn place_call(ctx: &Ctx, number: &str) {
    let r = ctx.m.borrow_mut().modem.dial(number);
    if r == 0 {
        ctx.go(phone_core::page::Page::Call);
    } else {
        ctx.note(&alloc::format!("Not dialled ({r})"));
    }
}

/// Contacts -> "Speed dial" on a card: Speed dial opens with `number` waiting for a key.
pub fn start_assign(ctx: &Ctx, number: &str) {
    let _ = number;
    ctx.go(phone_core::page::Page::Phone(
        phone_core::page::PhoneView::Speed,
    ));
}
