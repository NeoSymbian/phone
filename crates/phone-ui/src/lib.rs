// SPDX-License-Identifier: GPL-3.0-only
//! Phone's screens (Slint, no_std): the generated component [`PhoneApp`] and its globals. NEOPHONE.EXE
//! (symbian/phone) and the host snapshot tests drive the same UI; neither the platform nor any state lives here.
//! Contacts and Messages are apps of their own (crates/contacts-ui, crates/messages-ui).
//!
//! Globals, one per area, each with an `act(action, value)` callback the app turns into behaviour:
//! - `Nav` (ui-shared/): which page and view shows, the header, the footer;
//! - `DialState` (ui/phone/): the dialer, Recents, call details, speed dial;
//! - `CallState` (ui/call/): every call screen;
//! - `SettingsState` (ui/settings/): Phone & Contacts settings, Blocked numbers, Quick replies.
#![no_std]

extern crate alloc;

slint::include_modules!();
