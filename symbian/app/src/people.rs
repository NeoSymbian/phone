// SPDX-License-Identifier: GPL-3.0-only
//! The Contacts page: the list, search, the card, the editor, the queries (to be written).

use crate::ctx::{Chrome, Ctx};
use alloc::rc::Rc;
use alloc::vec::Vec;
use phone_ui::{ContactsState, PhoneApp};
use slint::ComponentHandle;

#[derive(Default)]
pub struct State {}

pub fn bind(ctx: &Rc<Ctx>, ui: &PhoneApp) {
    let ctx = Rc::downgrade(ctx);
    ui.global::<ContactsState>().on_act(move |_action, _value| {
        let Some(_ctx) = ctx.upgrade() else { return };
    });
}

pub fn show(_ctx: &Ctx) {}

pub fn chrome(_ctx: &Ctx) -> Chrome {
    Chrome::titled("Contacts")
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

/// Send MODEMD the contacts' known-number hashes (for "block unknown numbers").
pub fn send_known(_ctx: &Ctx) {}

/// Show the contact whose number is `number` (call details -> Open contact); false when none matches.
pub fn open_for_number(_ctx: &Ctx, _number: &str) -> bool {
    false
}

/// A new contact with `number` filled in (Recents / call details -> Save as contact, dialer -> Add to contacts).
pub fn new_with_number(ctx: &Ctx, _number: &str) {
    ctx.go(phone_core::page::Page::Contacts(phone_core::page::ContactsView::Edit));
}

/// The name `number` shows under (exactly one contact matches), for Recents and the call screens.
pub fn name_of(ctx: &Ctx, number: &str) -> Option<alloc::string::String> {
    let m = ctx.m.borrow();
    m.book.lookup(number).map(|c| c.title(m.order))
}
