// SPDX-License-Identifier: GPL-3.0-only
// Messages (mockups 28-36), with invented people and texts: the conversations, a thread with every bubble state,
// the held bubble's sheet, the details, a new message, the blocked messages.
mod common;
use messages_ui::{BlockedMessage, Bubble, MessagesState, Nav, Suggestion, ThreadRow};
use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};
use std::rc::Rc;

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(v)))
}

fn strings(v: &[&str]) -> ModelRc<SharedString> {
    model(v.iter().map(|s| SharedString::from(*s)).collect())
}

fn tint(rgb: u32) -> Color {
    Color::from_argb_encoded(0xff00_0000 | rgb)
}

fn thread(
    name: &str,
    initials: &str,
    rgb: u32,
    snippet: &str,
    when: &str,
    unread: i32,
) -> ThreadRow {
    ThreadRow {
        name: name.into(),
        initials: initials.into(),
        tint: tint(rgb),
        known: !initials.is_empty(),
        snippet: snippet.into(),
        when: when.into(),
        unread,
        failed: false,
    }
}

fn bubble(id: i32, text: &str, mine: bool, time: &str, state: i32, band: &str) -> Bubble {
    Bubble {
        id,
        text: text.into(),
        mine,
        time: time.into(),
        state,
        band: band.into(),
    }
}

#[test]
fn messages() {
    let s = common::app();
    let nav = s.ui.global::<Nav>();
    let m = s.ui.global::<MessagesState>();
    nav.set_page(6);
    nav.set_footer(true);
    nav.set_view("".into());
    m.set_empty(false);
    let mut failed = thread("Felix Wagner", "FW", 0x14a3a3, "You: Ok", "12 Nov", 0);
    failed.failed = true;
    m.set_threads(model(vec![
        thread(
            "Ava Lindqvist",
            "AL",
            0x22b46a,
            "Running late, see you at 7?",
            "10:05",
            2,
        ),
        thread(
            "Ben Okafor",
            "BO",
            0xe8833a,
            "The build passed on the phone!",
            "09:48",
            1,
        ),
        thread("+374 91 234567", "", 0, "Your code is 4821", "Yesterday", 0),
        thread(
            "Անի Պետրոսյան",
            "ԱՊ",
            0xd6409f,
            "You: Լավ, վաղը կզանգեմ",
            "13 Nov",
            0,
        ),
        thread(
            "Dario Rossi",
            "DR",
            0x8b5cf6,
            "You: Ok, thanks",
            "13 Nov",
            0,
        ),
        failed,
    ]));
    let px = s.shoot("messages-list");
    assert!(px.iter().any(|&p| p != 0), "something was drawn");

    nav.set_view("thread".into());
    m.set_title("Ava Lindqvist".into());
    m.set_bubbles(model(vec![
        bubble(
            1,
            "Hey! Are we still on for dinner tonight?",
            false,
            "09:51",
            0,
            "Today",
        ),
        bubble(2, "Yes. 7 at the usual place?", true, "09:53", 3, ""),
        bubble(3, "Perfect. I'll book a table.", false, "09:54", 0, ""),
        bubble(4, "Running late, see you at 7?", false, "10:05", 0, ""),
        bubble(5, "No problem, take your time", true, "10:06", 4, ""),
        bubble(6, "Ok", true, "10:07", 2, ""),
    ]));
    m.set_draft("Вижу тебя в 7".into());
    m.set_counter("Unicode · 57 left · 1 SMS".into());
    m.set_can_send(true);
    s.shoot("messages-thread");

    m.set_bubble_items(strings(&["Copy text", "Forward", "Delete", "Details"]));
    m.set_bubble_open(true);
    s.shoot("messages-bubble");
    m.set_bubble_open(false);
    s.shoot("messages-tmp");

    m.set_details_keys(strings(&["To", "Written", "Status", "Delivered", "Size"]));
    m.set_details_values(strings(&[
        "Ava Lindqvist · +1 555 0142",
        "Today 09:53",
        "Delivered",
        "Today 09:53",
        "1 SMS · 26 characters · GSM",
    ]));
    m.set_details_open(true);
    s.shoot("messages-details");
    m.set_details_open(false);

    nav.set_view("new".into());
    m.set_draft("".into());
    m.set_counter("".into());
    m.set_can_send(false);
    m.set_recipients(strings(&["Ava Lindqvist"]));
    m.set_to_text("an".into());
    m.set_suggestions(model(vec![
        Suggestion {
            name: "Ani Petrosyan".into(),
            detail: "Mobile · +374 28 123456".into(),
            initials: "AP".into(),
            tint: tint(0xd6409f),
        },
        Suggestion {
            name: "Aram Hakobyan".into(),
            detail: "Work · +374 10 555 010".into(),
            initials: "AH".into(),
            tint: tint(0x14a3a3),
        },
    ]));
    s.shoot("messages-new");

    nav.set_view("blocked".into());
    m.set_blocked(model(vec![
        BlockedMessage {
            id: 7,
            band: "Today".into(),
            title: "+1 900 555 0101".into(),
            snippet: "WIN A PRIZE! Reply YES to claim…".into(),
            why: "Blocked prefix".into(),
            when: "09:12".into(),
            initials: "".into(),
            tint: tint(0),
            known: false,
        },
        BlockedMessage {
            id: 8,
            band: "Yesterday".into(),
            title: "Անի Պետրոսյան".into(),
            snippet: "Զանգիր, խնդրում եմ".into(),
            why: "Blocked contact".into(),
            when: "19:30".into(),
            initials: "ԱՊ".into(),
            tint: tint(0x8a6d80),
            known: true,
        },
    ]));
    s.shoot("messages-blocked");
    std::fs::remove_file(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../snapshots/messages-tmp.png"
    ))
    .unwrap();
}
