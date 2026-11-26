// SPDX-License-Identifier: GPL-3.0-only
//! The notification channels Phone and Messages declare (top-panel TP4, protocol 1.21 `CHANNEL`): the shell applies
//! the user's choices per channel (Settings → Notifications → the app), and each card names its channel.

use e32::dprintln;
use neoapp::notification_policy::{Channel, importance};

/// Messages' channels (docs neoui-shell/protocol.md "TP4 policy": 1 Conversations, 2 Failed to send, 3 Delivery
/// reports, 4 Blocked messages). Only Conversations posts today; blocked senders never post.
pub mod messages {
    pub const CONVERSATIONS: u8 = 1;
    pub const FAILED: u8 = 2;
    pub const DELIVERY: u8 = 3;
    pub const BLOCKED: u8 = 4;
}

/// Phone's channels: the ongoing call's card and the missed calls'.
pub mod phone {
    pub const CALLS: u8 = 1;
    pub const MISSED: u8 = 2;
}

const MESSAGES: [(u8, u8, &str, &str); 4] = [
    (
        messages::CONVERSATIONS,
        importance::URGENT,
        "Conversations",
        "New messages",
    ),
    (
        messages::FAILED,
        importance::DEFAULT,
        "Failed to send",
        "A message that didn't go",
    ),
    (
        messages::DELIVERY,
        importance::LOW,
        "Delivery reports",
        "A message arrived",
    ),
    (
        messages::BLOCKED,
        importance::MIN,
        "Blocked messages",
        "From blocked senders",
    ),
];

const PHONE: [(u8, u8, &str, &str); 2] = [
    (
        phone::CALLS,
        importance::DEFAULT,
        "Calls",
        "The call in progress",
    ),
    (
        phone::MISSED,
        importance::DEFAULT,
        "Missed calls",
        "Calls you didn't answer",
    ),
];

/// Declare `list` after a connect (the same declarations again change nothing). A 1.20 shell refuses them
/// (`KErrNotSupported`): the cards still show, as its channel 0.
fn declare(app: &neoapp::App, list: &[(u8, u8, &str, &str)], who: &str) {
    for &(id, importance, name, description) in list {
        let c = Channel {
            id,
            importance,
            name: name.into(),
            description: description.into(),
        };
        if let Err(e) = app.channel(&c) {
            dprintln!("{}: channel {} not declared ({})", who, id, e);
            return;
        }
    }
}

pub fn declare_messages(app: &neoapp::App) {
    declare(app, &MESSAGES, "neomsg");
}

pub fn declare_phone(app: &neoapp::App) {
    declare(app, &PHONE, "neophone");
}
