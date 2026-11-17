// SPDX-License-Identifier: GPL-3.0-only
//! What Phone (NEOPHONE.EXE), Contacts (NEOCONT.EXE) and Messages (NEOMSG.EXE) share on the device. They are three
//! apps that open each other's pages (neoui ipc OPEN, [`open`]); each reads the contacts book ([`store`], written
//! only by Contacts, which bumps `contacts.rev` so the others reload), MODEMD ([`modem`]), the `phone.*` and
//! `contacts.*` settings ([`prefs`]) and the blocklist ([`block`]).
#![no_std]

extern crate alloc;

pub mod block;
pub mod modem;
pub mod prefs;
pub mod store;
pub mod ui;

use alloc::string::String;
use contacts::{Book, NameOrder};
use phone_core::page::Page;

/// The setting Contacts bumps after each save of the book: the other apps watch `contacts.` and reload it.
pub const CONTACTS_REV: &str = "contacts.rev";

/// The name `number` shows under (exactly one contact matches), for Recents, the call screens and Messages.
pub fn name_of(book: &Book, order: NameOrder, number: &str) -> Option<String> {
    book.lookup(number).map(|c| c.title(order))
}

/// How long an app's loop may sleep (µs): its own next wake `own_us`, or sooner when Slint needs it (a timer, an
/// animation: neoapp `App::wake_us`). On waking the loop draws, so switches flip and long-presses fire.
pub fn wait_us(app: &neoapp::App, own_us: i32) -> i32 {
    let slint = app
        .wake_us()
        .map_or(i32::MAX, |us| i32::try_from(us).unwrap_or(i32::MAX));
    own_us.min(slint).max(1000)
}

/// Open `page` in the app it belongs to (neoui ipc OPEN) with `arg` (a number, a settings section).
pub fn open(app: &neoapp::App, page: Page, arg: &str) -> Result<(), i32> {
    open_number(app, page.app(), page.number(), arg)
}

/// Open app `key` on its page `number` (see `phone_core::page::number`) with `arg`.
pub fn open_number(app: &neoapp::App, key: &str, number: u16, arg: &str) -> Result<(), i32> {
    let mut a = String::from(arg);
    while a.len() > neoui_proto::ipc::ARG_MAX {
        a.pop();
    }
    app.open(key, number, &a)
}
