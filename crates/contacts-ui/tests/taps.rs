// SPDX-License-Identifier: GPL-3.0-only
// Contacts' taps reach the app: ⋮'s items, a long press, the selection header's ⋮ ▸ Select all, the editor's ✓ and ✕,
// the Block query's red button only with a box ticked, and the card's choice sheet. What the app does with them (the Discard query when the
// editor changed: phone_core::people::edited) is tested where it lives.
mod common;
use contacts_ui::{ContactRow, ContactSection, ContactsState, Icons, Nav};
use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(v)))
}

fn strings(v: &[&str]) -> ModelRc<SharedString> {
    model(v.iter().map(|s| SharedString::from(*s)).collect())
}

type Log = Rc<RefCell<Vec<(String, String)>>>;

/// Every Nav.header and ContactsState.act the screens send, as (kind, value).
fn record(s: &common::Shot) -> Log {
    let log: Log = Rc::default();
    let l = log.clone();
    s.ui.global::<Nav>()
        .on_header(move |k, i| l.borrow_mut().push((k.into(), i.to_string())));
    let l = log.clone();
    s.ui.global::<ContactsState>()
        .on_act(move |a, v| l.borrow_mut().push((a.into(), v.into())));
    log
}

fn took(log: &Log) -> Vec<(String, String)> {
    std::mem::take(&mut *log.borrow_mut())
}

fn sent(kind: &str, value: &str) -> Vec<(String, String)> {
    vec![(kind.into(), value.into())]
}

/// The red pixels' middle (dp), and how many there are.
fn red(px: &[u32]) -> (f32, f32, usize) {
    let hits: Vec<(usize, usize)> = px
        .iter()
        .enumerate()
        .filter(|&(_, &p)| p == 0xe5484d)
        .map(|(i, _)| (i % common::W as usize, i / common::W as usize))
        .collect();
    let n = hits.len().max(1);
    let x = hits.iter().map(|h| h.0).sum::<usize>() / n;
    let y = hits.iter().map(|h| h.1).sum::<usize>() / n;
    (x as f32 / common::DP, y as f32 / common::DP, hits.len())
}

#[test]
fn taps() {
    let s = common::app();
    let log = record(&s);
    let nav = s.ui.global::<Nav>();
    let c = s.ui.global::<ContactsState>();
    let icons = s.ui.global::<Icons>();

    // the list: [Contacts · 🔍 · + · ⋮], my card, one section of two
    nav.set_page(1);
    nav.set_title("Contacts".into());
    nav.set_actions(model(vec![
        icons.invoke_act(14, "Search contacts".into()),
        icons.invoke_act(3, "New contact".into()),
    ]));
    nav.set_options(strings(&[
        "Mark several",
        "Import vCards",
        "Export vCards",
        "Settings",
    ]));
    nav.set_footer(true);
    c.set_total(2);
    let rows = |marked: bool| {
        model(vec![ContactSection {
            letter: "A".into(),
            top: 0.0,
            people: model(
                [(7, "Ava Lindqvist"), (9, "Aram Hakobyan")]
                    .iter()
                    .map(|&(id, name)| ContactRow {
                        id,
                        name: name.into(),
                        initials: "A".into(),
                        tint: Color::from_rgb_u8(0x22, 0xb4, 0x6a),
                        marked,
                    })
                    .collect(),
            ),
        }])
    };
    c.set_sections(rows(false));
    s.pixels();

    // ⋮ ▸ Export vCards (the third item: 48 header + 8 + 48 per item)
    s.press(332.0, 24.0, true);
    s.pixels();
    s.press(250.0, 48.0 + 8.0 + 48.0 * 2.0 + 24.0, true);
    assert_eq!(took(&log), sent("option", "2"));

    // a tap on a row opens it; a long press (the finger held 0.6 s) starts selection mode with it
    let row_y = 48.0 + 56.0 + 34.0 + 28.0;
    s.press(150.0, row_y, true);
    assert_eq!(took(&log), sent("open", "7"));
    s.press(150.0, row_y, false);
    s.wait(600);
    s.release(150.0, row_y);
    assert_eq!(took(&log), sent("hold", "7"));

    // the app answers with the selection view: [✕ 1 selected · 🗑 · ⋮], the footer gone
    nav.set_view("mark".into());
    nav.set_footer(false);
    nav.set_selecting(true);
    nav.set_selected(1);
    nav.set_selection_actions(model(vec![icons.invoke_act(6, "Delete marked".into())]));
    nav.set_selection_menu(strings(&["Select all", "Export"]));
    c.set_sections(rows(true));
    s.pixels();
    s.press(332.0 - 48.0, 24.0, true);
    assert_eq!(took(&log), sent("selection", "0"));
    // ⋮ ▸ Select all
    s.press(332.0, 24.0, true);
    s.pixels();
    s.press(250.0, 48.0 + 8.0 + 24.0, true);
    assert_eq!(took(&log), sent("selection-option", "0"));
    // a tap on a row marks it (no my card row in selection mode); ✕ leaves selection mode
    s.press(150.0, row_y - 56.0, true);
    assert_eq!(took(&log), sent("mark", "7"));
    s.press(28.0, 24.0, true);
    assert_eq!(took(&log), sent("selection-closed", "0"));
    nav.set_selecting(false);
    nav.set_selected(0);

    // the editor: [✕ Edit contact ✓]; ✓ saves, ✕ is Back (the app asks Discard changes? when something changed)
    nav.set_view("edit".into());
    nav.set_title("Edit contact".into());
    nav.set_form(true);
    nav.set_has_action(true);
    nav.set_action_tool(5);
    s.pixels();
    s.press(332.0, 24.0, true);
    assert_eq!(took(&log), sent("save", "0"));
    s.press(28.0, 24.0, true);
    assert_eq!(took(&log), sent("back", "0"));
    // the photo slot says what it needs (12 + 48 under the header)
    s.press(60.0, 48.0 + 60.0, true);
    assert_eq!(took(&log), sent("photo", ""));

    // the Block query: Block is off until Calls or Messages is ticked
    c.set_dialog_question("Block Ava Lindqvist?".into());
    c.set_dialog_detail("Blocked calls are rejected as busy and never ring.".into());
    c.set_dialog_confirm("Block".into());
    c.set_dialog_checks(strings(&["Calls", "Messages"]));
    c.set_dialog_checked(model(vec![true, false]));
    c.set_dialog_open(true);
    let (x, y, lit) = red(&s.pixels());
    assert!(lit > 1000, "the red Block button shows");
    c.set_dialog_checked(model(vec![false, false]));
    assert_eq!(red(&s.pixels()).2, 0, "Block is dimmed with no box ticked");
    s.press(x, y, true);
    assert_eq!(took(&log), vec![], "Block does nothing with no box ticked");
    c.set_dialog_checked(model(vec![false, true]));
    s.pixels();
    s.press(x, y, true);
    assert_eq!(took(&log), sent("confirm", ""));
    c.set_dialog_open(false);

    // the card's choice sheet (Send as vCard): its last row, Save as vCard file, is choice 1
    c.set_choice_title("Send as vCard".into());
    c.set_choice_detail("Ava Lindqvist".into());
    c.set_choices(strings(&["Message", "Save as vCard file"]));
    s.pixels();
    s.pixels();
    let h = common::H as f32 / common::DP;
    s.press(180.0, h - 44.0, true);
    assert_eq!(took(&log), sent("choice", "1"));
    c.set_choice_title("".into());
}
