// SPDX-License-Identifier: GPL-3.0-only
// Tap-throughs: a finger on the drawn screens reaches the app's callbacks (MessagesState.act, Nav.header), as on the
// phone. One test: Slint's platform is one per process.
mod common;
use messages_ui::{Bubble, Icons, KeyboardFocus, MessagesState, Nav, ThreadRow};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

const DP: f32 = 3.0;

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(v)))
}

fn strings(v: &[&str]) -> ModelRc<SharedString> {
    model(v.iter().map(|s| SharedString::from(*s)).collect())
}

fn row(name: &str) -> ThreadRow {
    ThreadRow {
        name: name.into(),
        snippet: "Hi".into(),
        when: "10:05".into(),
        ..ThreadRow::default()
    }
}

type Log = Rc<RefCell<Vec<(String, String)>>>;

/// What the screens asked for since the last call.
fn take(log: &Log) -> Vec<(String, String)> {
    std::mem::take(&mut *log.borrow_mut())
}

fn pair(a: &str, b: &str) -> (String, String) {
    (a.into(), b.into())
}

#[test]
fn taps() {
    let s = common::app();
    let nav = s.ui.global::<Nav>();
    let m = s.ui.global::<MessagesState>();
    let icons = s.ui.global::<Icons>();
    let log: Log = Rc::default();
    let l = log.clone();
    let ui = s.ui.as_weak();
    m.on_act(move |a, v| {
        // as the app does: a pick closes the menu
        if a == "menu"
            && let Some(ui) = ui.upgrade()
        {
            ui.global::<MessagesState>().set_menu_open(false);
        }
        l.borrow_mut().push((a.into(), v.into()));
    });
    let l = log.clone();
    nav.on_header(move |k, i| l.borrow_mut().push((k.into(), i.to_string())));

    // the conversations: [Messages · 🔍 ✎ ⋮]
    nav.set_page(6);
    nav.set_view("".into());
    nav.set_title("Messages".into());
    nav.set_actions(model(vec![
        icons.invoke_act(14, "Search".into()),
        icons.invoke_act(4, "New".into()),
    ]));
    nav.set_options(strings(&[
        "Mark all as read",
        "Select",
        "Archived",
        "Delete conversations",
        "Blocked messages",
        "Settings",
    ]));
    nav.set_footer(true);
    m.set_threads(model(vec![row("Ava Lindqvist"), row("Ben Okafor")]));
    s.tick();

    // ⋮ (the header's right end), then its fourth item: the menu hangs under the header, 48 dp a row after 8 dp
    s.press(332.0 * DP, 24.0 * DP, 0);
    s.tick();
    s.press(200.0 * DP, (48.0 + 8.0 + 3.0 * 48.0 + 24.0) * DP, 0);
    s.tick();
    assert_eq!(
        take(&log),
        [pair("option", "3")],
        "⋮ ▸ Delete conversations"
    );

    // a long-press on the first row: selection mode
    let row0 = (48.0 + 4.0 + 36.0) * DP;
    s.press(180.0 * DP, row0, 700);
    s.tick();
    assert_eq!(take(&log), [pair("select", "0")]);

    // a long swipe left on the second row deletes it (the app holds the delete for the toast's Undo)
    // shortcut: a short swipe's revealed Delete isn't tapped here: the kit's RowActions lets its swipe handler grab
    // that tap (neoui rows.slint), so it doesn't reach the button; test it once the kit is fixed
    let row1 = row0 + 72.0 * DP;
    s.drag(340.0 * DP, 60.0 * DP, row1);
    s.tick();
    assert_eq!(take(&log), [pair("delete", "1")]);

    // a thread: a long-press on a bubble asks for its menu at the finger; Copy text is picked there
    nav.set_view("thread".into());
    nav.set_footer(false);
    let bubble = |id: i32, text: &str, mine| Bubble {
        id,
        text: text.into(),
        meta: "09:53".into(),
        mine,
        ..Bubble::default()
    };
    m.set_bubbles(model(vec![
        bubble(1, "Hey! Dinner tonight?", false),
        bubble(2, "Yes. 7 at the usual place?", true),
    ]));
    s.tick();
    // the newest bubble sits just above the composer (68 dp: its field and the hairline)
    let page_h = common::H as f32 / DP - 48.0;
    let held_y = 48.0 + page_h - 68.0 - 10.0 - 20.0;
    s.press(300.0 * DP, held_y * DP, 700);
    s.tick();
    assert_eq!(take(&log), [pair("hold", "2")]);
    // the app opens the menu with the bubble's items; the menu sits above the finger in the lower half
    m.set_menu_items(strings(&["Copy text", "Forward", "Delete", "Details"]));
    m.set_menu_destructive(2);
    m.set_menu_open(true);
    m.set_held(2);
    s.tick();
    let fy = m.get_menu_y() / DP - 48.0;
    let menu_h = 4.0 * 48.0 + 16.0;
    let at_y = (fy - menu_h - 24.0).max(8.0);
    let fx = m.get_menu_x() / DP;
    let at_x = (fx - 220.0 + 54.0).clamp(12.0, 360.0 - 220.0 - 24.0);
    s.press((at_x + 100.0) * DP, (48.0 + at_y + 8.0 + 24.0) * DP, 0);
    s.tick();
    assert_eq!(take(&log), [pair("menu", "Copy text")]);

    // ⋮ ▸ Block number: the query with Calls and Messages; tick Messages off, then Block
    m.set_block_question("Block +374 91 234567?".into());
    m.set_block_calls(true);
    m.set_block_messages(true);
    m.set_block_open(true);
    s.tick();
    s.press(80.0 * DP, 388.0 * DP, 0);
    s.press(187.0 * DP, 446.0 * DP, 0);
    s.tick();
    assert_eq!(
        take(&log),
        [pair("block-check", "1"), pair("block-confirm", "")]
    );
    m.set_block_open(false);

    // a new message: To takes the focus, and the keyboard is asked for, once the view is up
    s.ui.global::<KeyboardFocus>().set_open(false);
    nav.set_view("new".into());
    nav.set_form(true);
    s.tick();
    s.tick();
    assert!(
        s.ui.global::<KeyboardFocus>().get_open(),
        "To has the focus"
    );
    // three recipients wrap To onto a second line
    m.set_recipients(strings(&["Ava Lindqvist", "Ben Okafor", "Անի Պետրոսյան"]));
    let px = s.shoot("tmp-msg-to");
    std::fs::remove_file(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../snapshots/tmp-msg-to.png"
    ))
    .unwrap();
    // the third chip starts the second line: 8 dp, a 40 dp line and 8 dp down, at the left
    let at = |x: f32, y: f32| px[(y * DP) as usize * common::W as usize + (x * DP) as usize];
    assert_eq!(
        at(40.0, 48.0 + 8.0 + 48.0 + 20.0),
        0x1f3a66,
        "a chip on the second line"
    );
}
