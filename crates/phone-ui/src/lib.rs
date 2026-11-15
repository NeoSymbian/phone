// SPDX-License-Identifier: GPL-3.0-only
//! NeoPhone's screens (Slint, no_std): the generated component [`PhoneApp`] and its globals. The app process
//! (symbian/app) and the host snapshot tests (tests/snapshots.rs) drive the same UI; neither the platform nor any
//! state lives here.
//!
//! Globals, one per area, each with an `act(action, value)` callback the app turns into behaviour:
//! - `Nav`: which page and view shows, the Options sheet;
//! - `DialState` (ui/phone/): the dialer, Recents, call details, speed dial;
//! - `ContactsState` (ui/contacts/): the list, search, the card, the editor, the queries;
//! - `CallState` (ui/call/): every call screen;
//! - `SettingsState` (ui/settings/): Phone & Contacts settings, Blocked numbers, Quick replies;
//! - `MessagesState` (ui/messages/): the conversations, a thread, a new message, the blocked messages.
#![no_std]

extern crate alloc;

slint::include_modules!();
