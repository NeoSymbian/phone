// SPDX-License-Identifier: Apache-2.0
//! NeoPhone's behaviour, without UI or OS (no_std + alloc), host-tested. The app (symbian/app) feeds it MODEMD's
//! status and call log, the contacts book and the settings, and shows what it computes; phone-ui draws it.
//!
//! - [`page`]: the pages and views, and the UI server's page numbers (neoui `host::PAGES`);
//! - [`dial`]: the dialer: typed number, smart matches, long presses (`+`, voicemail, speed dial);
//! - [`recents`]: Recents rows: filters, day sections, folded repeats, call details;
//! - [`calls`]: which call screen shows, from MODEMD's call list;
//! - [`speed`]: speed dial keys 2..=9 (`phone.speed`);
//! - [`people`]: the contact list's rows, sections and side index, the card, the editor;
//! - [`blocking`]: editing the blocklist MODEMD applies, and the contacts' known-number hashes;
//! - [`messages`]: the copy of MODEMD's messages, the conversations, a thread's bubbles, the composer's counter.
#![no_std]

extern crate alloc;

pub mod blocking;
pub mod calls;
pub mod dial;
pub mod messages;
pub mod page;
pub mod people;
pub mod recents;
pub mod speed;
