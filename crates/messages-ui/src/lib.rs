// SPDX-License-Identifier: GPL-3.0-only
//! Messages' screens (Slint, no_std): the generated component [`MessagesApp`] and its globals (`Nav`,
//! `MessagesState`). NEOMSG.EXE (symbian/messages) and the host snapshot tests drive the same UI.
#![no_std]

extern crate alloc;

slint::include_modules!();
