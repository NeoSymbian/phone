// SPDX-License-Identifier: GPL-3.0-only
// Messages v2, one snapshot per design board (NEOPHONE v2 p2-msg-*), with the boards' sample people and texts: the
// conversations (Gestures and Buttons), a swipe with its Undo toast, selection mode, a thread (both modes), the
// menu at the finger, a message's details, a new message, the blocked messages and the shade's entries.
mod common;
use messages_ui::{
    BlockedMessage, Bubble, Icons, KeyboardFocus, MessagesState, Nav, ShadeNote, Suggestion,
    ThreadRow,
};
use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};
use std::rc::Rc;

const DP: f32 = 3.0;

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
        checked: false,
    }
}

fn threads() -> Vec<ThreadRow> {
    let mut felix = thread("Felix Wagner", "FW", 0x2f80ed, "You: Ok", "Mon", 0);
    felix.failed = true;
    vec![
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
            "Wed",
            0,
        ),
        thread("Dario Rossi", "DR", 0xc2410c, "You: Ok, thanks", "Tue", 0),
        felix,
    ]
}

fn bubble(id: i32, text: &str, mine: bool, meta: &str, band: &str) -> Bubble {
    Bubble {
        id,
        text: text.into(),
        meta: meta.into(),
        mine,
        failed: false,
        band: band.into(),
    }
}

fn bubbles() -> Vec<Bubble> {
    let mut failed = bubble(5, "No problem, take your time", true, "", "");
    failed.failed = true;
    vec![
        bubble(
            1,
            "Hey! Are we still on for dinner tonight?",
            false,
            "09:51",
            "Today",
        ),
        bubble(2, "Yes. 7 at the usual place?", true, "09:53 ✓✓", ""),
        bubble(3, "Perfect. I'll book a table.", false, "09:54", ""),
        bubble(4, "Running late, see you at 7?", false, "10:05", ""),
        failed,
    ]
}

#[test]
fn boards() {
    let s = common::app();
    let nav = s.ui.global::<Nav>();
    let m = s.ui.global::<MessagesState>();
    let icons = s.ui.global::<Icons>();
    let acts = |v: &[(i32, &str)]| {
        model(
            v.iter()
                .map(|(t, l)| icons.invoke_act(*t, (*l).into()))
                .collect(),
        )
    };
    let options = strings(&["Mark all as read", "Select", "Archived", "Settings"]);

    // the conversations: [Messages · 🔍 ✎ ⋮], the footer with its badges
    nav.set_page(6);
    nav.set_view("".into());
    nav.set_title("Messages".into());
    nav.set_actions(acts(&[(14, "Search"), (4, "New")]));
    nav.set_options(options.clone());
    nav.set_search(false);
    nav.set_footer(true);
    nav.set_missed(2);
    nav.set_unread(3);
    m.set_empty("".into());
    m.set_threads(model(threads()));
    let px = s.shoot("p2-msg-list");
    assert!(px.iter().any(|&p| p != 0), "something was drawn");
    s.resize(1917);
    s.shoot("p2-msg-list-buttons");
    s.resize(common::H);

    // a swipe left on Dario Rossi after Felix Wagner's was deleted, with the toast's Undo
    let mut rows = threads();
    rows.pop();
    m.set_threads(model(rows));
    s.ui.set_toast("Conversation deleted".into());
    s.ui.set_toast_action("Undo".into());
    let y = (4.0 + 4.0 * 72.0 + 36.0 + 48.0) * DP;
    s.drag(300.0 * DP, 140.0 * DP, y);
    s.shoot("p2-msg-swipe");
    s.ui.set_toast("".into());

    // selection mode: [✕ 2 selected · select all, delete], no footer
    let mut rows = threads();
    rows[4].checked = true;
    rows[5].checked = true;
    m.set_threads(model(rows));
    m.set_selecting(true);
    nav.set_selected(2);
    nav.set_selection_actions(acts(&[(11, "Select all"), (6, "Delete")]));
    nav.set_footer(false);
    s.shoot("p2-msg-select");
    nav.set_selected(0);
    m.set_selecting(false);

    // a thread: [← Ava Lindqvist · 📞 ⋮], the composer focused with Cyrillic text and its counter
    nav.set_view("thread".into());
    nav.set_title("Ava Lindqvist".into());
    nav.set_sub(true);
    nav.set_actions(acts(&[(1, "Call")]));
    nav.set_options(strings(&[
        "Paste",
        "Contact card",
        "Archive",
        "Delete conversation",
        "Block number",
    ]));
    m.set_bubbles(model(bubbles()));
    m.set_draft("Вижу тебя в 7".into());
    m.set_counter("Unicode · 57 left · 1 SMS".into());
    m.set_can_send(true);
    m.set_autofocus(true);
    s.shoot("p2-msg-thread");
    s.resize(1917);
    s.shoot("p2-msg-thread-buttons");
    s.resize(common::H);

    // a hold on "Yes. 7 at the usual place?": the menu at the finger
    m.set_menu_items(strings(&["Copy text", "Forward", "Delete", "Details"]));
    m.set_menu_destructive(2);
    m.set_menu_x(282.0 * DP);
    m.set_menu_y((400.0 - 25.0) * DP);
    m.set_menu_open(true);
    s.shoot("p2-msg-options");
    m.set_menu_open(false);
    s.shoot("tmp");

    // a message's details: [← Message details]
    nav.set_view("details".into());
    nav.set_title("Message details".into());
    nav.set_actions(acts(&[]));
    nav.set_options(strings(&[]));
    m.set_details(bubble(
        2,
        "Yes. 7 at the usual place?",
        true,
        "09:53 ✓✓",
        "",
    ));
    m.set_details_keys(strings(&["To", "Written", "Status", "Delivered", "Size"]));
    m.set_details_values(strings(&[
        "Ava Lindqvist · +1 555 0142",
        "Today 09:53",
        "Delivered",
        "Today 09:53",
        "1 SMS · 26 characters · GSM",
    ]));
    s.shoot("p2-msg-details");

    // a new message: [✕ New message], To with a chip and "an" typed, the keyboard up (the shell's, 276 dp)
    nav.set_view("new".into());
    nav.set_title("New message".into());
    nav.set_form(true);
    nav.set_has_action(false);
    m.set_draft("".into());
    m.set_counter("".into());
    m.set_can_send(false);
    m.set_recipients(strings(&["Ava Lindqvist"]));
    m.set_to_text("an".into());
    m.set_suggestions(model(vec![
        Suggestion {
            pre: "".into(),
            hit: "An".into(),
            post: "i Petrosyan".into(),
            detail: "Mobile · +374 28 123456".into(),
            initials: "AP".into(),
            tint: tint(0xd6409f),
        },
        Suggestion {
            pre: "Aram Hakobyan".into(),
            hit: "".into(),
            post: "".into(),
            detail: "Work · +374 10 555 010".into(),
            initials: "AH".into(),
            tint: tint(0x14a3a3),
        },
    ]));
    // the view change closes the keyboard (the frame leaves the fields): open it after
    s.shoot("tmp");
    let kf = s.ui.global::<KeyboardFocus>();
    kf.set_open(true);
    kf.set_text_h(276.0 * DP);
    s.shoot("p2-msg-new");
    kf.set_open(false);
    nav.set_form(false);

    // the blocked messages: [← Blocked messages · 🗑]
    nav.set_view("blocked".into());
    nav.set_title("Blocked messages".into());
    nav.set_actions(acts(&[(6, "Clear")]));
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
            tint: tint(0xd6409f),
            known: true,
        },
    ]));
    s.shoot("p2-msg-blocked");

    // the shade over the conversations: one entry per conversation, with Reply and Call
    nav.set_view("".into());
    nav.set_title("Messages".into());
    nav.set_sub(false);
    nav.set_footer(true);
    nav.set_actions(acts(&[(14, "Search"), (4, "New")]));
    nav.set_options(options);
    m.set_threads(model(threads()));
    let note = |title: &str, text: &str| ShadeNote {
        title: title.into(),
        text: text.into(),
        actions: strings(&["Reply", "Call"]),
    };
    s.ui.set_missed(2);
    s.ui.set_notes(model(vec![
        note("Ava Lindqvist", "Running late, see you at 7?"),
        note("Ben Okafor", "The build passed on the phone!"),
    ]));
    s.shoot("p2-msg-notify");

    std::fs::remove_file(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../snapshots/tmp.png"
    ))
    .unwrap();
}
