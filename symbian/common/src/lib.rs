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

/// The green bar under the header while a call goes on and its screen isn't showing ("Ava Lindqvist · 04:12 · Return
/// to call", the prototype's hidden call). Its timer counts from MODEMD's connect time (modem-ipc 2.6 `Call::since`), so
/// it shows the call screen's time in every app; a MODEMD without it: from when this app first saw the call connected.
#[derive(Default)]
pub struct CallBar {
    since: Option<(u8, u64)>,
}

impl CallBar {
    /// The bar's text for `status` at `now_ms` ("" = no call: no bar).
    pub fn text(
        &mut self,
        status: Option<&modem_ipc::Status>,
        book: &Book,
        order: NameOrder,
        now_ms: u64,
    ) -> String {
        use modem_ipc::call as st;
        let call = status.and_then(|s| {
            s.calls
                .as_slice()
                .iter()
                .find(|c| !matches!(c.state, st::NONE | st::END | st::INCOMING | st::WAITING))
                .copied()
        });
        let Some(c) = call else {
            self.since = None;
            return String::new();
        };
        let number = c.number.as_str();
        let who = if c.multiparty {
            String::from("Conference")
        } else {
            let name = name_of(book, order, number);
            phone_rules::caller::caller_line(name.as_deref(), number, c.presentation, false).0
        };
        if !matches!(c.state, st::CONVERSATION | st::HOLD) {
            return alloc::format!("{who} · Calling…");
        }
        if c.since != 0 {
            self.since = Some((c.id, c.since));
        } else if self.since.is_none_or(|(id, _)| id != c.id) {
            self.since = Some((c.id, now_ms));
        }
        let secs = self
            .since
            .map_or(0, |(_, at)| now_ms.saturating_sub(at) / 1000);
        let clock = phone_core::calls::clock(secs as u32);
        alloc::format!("{who} · {clock}")
    }

    /// A call goes on: the bar ticks every second.
    pub fn live(&self) -> bool {
        self.since.is_some()
    }
}

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
