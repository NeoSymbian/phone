// SPDX-License-Identifier: GPL-3.0-only
// Contacts (mockups 16-22, 24, 26): the list with the per-script index, search, the card (blocked too), the editor and
// its number-type picker, the queries, Options and Mark several.
mod common;
use phone_ui::{
    ContactEditNumber, ContactHit, ContactMark, ContactNumber, ContactRow, ContactSection,
    ContactsState, Nav,
};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{Color, ComponentHandle, LogicalPosition, Model, ModelRc, SharedString, VecModel};
use std::rc::Rc;

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(v)))
}

fn strings(v: &[&str]) -> ModelRc<SharedString> {
    model(v.iter().map(|s| SharedString::from(*s)).collect())
}

fn rgb(c: u32) -> Color {
    Color::from_rgb_u8((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

fn row(id: i32, name: &str, initials: &str, tint: u32) -> ContactRow {
    ContactRow {
        id,
        name: name.into(),
        initials: initials.into(),
        tint: rgb(tint),
        marked: false,
    }
}

/// (name, initials, tint)
type Person = (&'static str, &'static str, u32);

fn sections() -> Vec<ContactSection> {
    let mut top = 0.0;
    let groups: [(&str, &[Person]); 9] = [
        (
            "A",
            &[
                ("Aram Hakobyan", "AH", 0x14a3a3),
                ("Ava Lindqvist", "AL", 0x22b46a),
            ],
        ),
        ("B", &[("Ben Okafor", "BO", 0xe8833a)]),
        ("F", &[("Felix Brandt", "FB", 0x2f80ed)]),
        ("S", &[("Sofia Russo", "SR", 0xe5484d)]),
        ("Ա", &[("Անի Պետրոսյան", "ԱՊ", 0xd6409f)]),
        ("Գ", &[("Գոռ Սարգսյան", "ԳՍ", 0x14a37f)]),
        ("А", &[("Анна Смирнова", "АС", 0x8b5cf6)]),
        ("М", &[("Мария Ковалёва", "МК", 0x8b5cf6)]),
        ("#", &[("555-0100", "#", 0x2f80ed)]),
    ];
    groups
        .iter()
        .enumerate()
        .map(|(k, (l, people))| {
            let s = ContactSection {
                letter: (*l).into(),
                top,
                people: model(
                    people
                        .iter()
                        .enumerate()
                        .map(|(i, (n, a, t))| row((k * 10 + i) as i32, n, a, *t))
                        .collect(),
                ),
            };
            top += 30.0 + 56.0 * people.len() as f32;
            s
        })
        .collect()
}

fn index() -> Vec<ContactMark> {
    let mark = |t: &str, letters: &[&str]| ContactMark {
        text: t.into(),
        letters: strings(letters),
        tops: model(letters.iter().map(|_| 0.0).collect()),
    };
    let mut v = Vec::new();
    for t in ["A", "B", "F"] {
        v.push(mark(t, &[t]));
    }
    v.push(mark("•", &["G", "H", "K", "L", "M", "N", "P"]));
    v.push(mark("S", &["S"]));
    v.push(mark("Z", &["Z"]));
    v.push(mark("–", &[]));
    for t in ["Ա", "Գ"] {
        v.push(mark(t, &[t]));
    }
    v.push(mark("•", &["Դ", "Ե", "Կ", "Հ", "Ղ"]));
    v.push(mark("Մ", &["Մ"]));
    v.push(mark("Ս", &["Ս"]));
    v.push(mark("–", &[]));
    for t in ["А", "Д"] {
        v.push(mark(t, &[t]));
    }
    v.push(mark("•", &["Е", "И", "К", "Л"]));
    v.push(mark("М", &["М"]));
    v.push(mark("Я", &["Я"]));
    v.push(mark("#", &["#"]));
    v
}

fn tap(s: &common::Shot, x: f32, y: f32, release: bool) {
    let w = s.ui.window();
    let position = LogicalPosition::new(x, y);
    w.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    if release {
        w.dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
    }
}

#[test]
fn contacts() {
    let s = common::app();
    let nav = s.ui.global::<Nav>();
    let c = s.ui.global::<ContactsState>();
    nav.set_page(1);

    // 16: the list
    c.set_total(128);
    c.set_has_my_card(true);
    c.set_favourites(model(vec![
        row(1, "Ava", "AL", 0x22b46a),
        row(2, "Անի", "ԱՊ", 0xd6409f),
        row(3, "Ben", "BO", 0xe8833a),
        row(4, "Мария", "МК", 0x8b5cf6),
    ]));
    c.set_sections(model(sections()));
    c.set_index(model(index()));
    let px = s.shoot("contacts-list");
    assert!(px.iter().any(|&p| p != 0));

    // 22: Options over the list
    nav.set_options(strings(&[
        "New contact",
        "Mark several",
        "Import vCards",
        "Export vCards",
        "Settings",
    ]));
    nav.set_options_open(true);
    s.shoot("contacts-options");
    nav.set_options_open(false);

    // a finger on the Latin "•": its letters in a bubble
    let dp = 3.0;
    // the index starts beside the first section: below my card (52) and the favourites (30 + 94)
    let list_top = (52.0 + 64.0 + 52.0 + 30.0 + 94.0) * dp;
    let pitch = (1902.0 - list_top - 8.0 * dp) / 21.0;
    tap(
        &s,
        1080.0 - 11.0 * dp,
        list_top + 4.0 * dp + 3.5 * pitch,
        false,
    );
    s.shoot("contacts-index");
    s.ui.window().dispatch_event(WindowEvent::PointerExited);

    // 17: search, the field focused
    c.set_search("an".into());
    c.set_results(model(vec![
        ContactHit {
            id: 1,
            initials: "AP".into(),
            tint: rgb(0xd6409f),
            name_pre: "".into(),
            name_hit: "An".into(),
            name_post: "i Petrosyan".into(),
            detail_pre: "Mobile · ".into(),
            detail_hit: "".into(),
            detail_post: "+37428123456".into(),
        },
        ContactHit {
            id: 2,
            initials: "AH".into(),
            tint: rgb(0x14a3a3),
            name_pre: "Aram Hakobyan".into(),
            name_hit: "".into(),
            name_post: "".into(),
            detail_pre: "Company: ".into(),
            detail_hit: "An".into(),
            detail_post: "ahit & Co".into(),
        },
    ]));
    tap(&s, 200.0 * dp, (52.0 + 32.0) * dp, true);
    s.shoot("contacts-search");
    c.set_search("".into());

    // Mark several
    nav.set_view("mark".into());
    let marked = sections();
    for (k, i) in [(0, 1), (4, 0)] {
        let p = marked[k].people.row_data(i).unwrap();
        marked[k]
            .people
            .set_row_data(i, ContactRow { marked: true, ..p });
    }
    c.set_sections(model(marked));
    c.set_marked(2);
    s.shoot("contacts-mark");

    // 18: the card
    nav.set_view("card".into());
    c.set_card_name("Ava Lindqvist".into());
    c.set_card_company("Example Co".into());
    c.set_card_initials("AL".into());
    c.set_card_tint(rgb(0x22b46a));
    c.set_card_favourite(true);
    c.set_card_numbers(model(vec![
        ContactNumber {
            label: "Mobile".into(),
            number: "+15550142".into(),
        },
        ContactNumber {
            label: "Telephone (work)".into(),
            number: "+15550177".into(),
        },
    ]));
    c.set_card_email("ava@example.org".into());
    c.set_card_note("Met at the Yerevan meetup".into());
    c.set_card_speed(2);
    c.set_can_message(true);
    c.set_can_email(true);
    s.shoot("contacts-card");

    // 21: the delete query
    c.set_dialog_question("Delete Ava Lindqvist?".into());
    c.set_dialog_detail("Its speed dial key 2 is cleared too.".into());
    c.set_dialog_confirm("Delete".into());
    c.set_dialog_open(true);
    s.shoot("contacts-delete");
    c.set_dialog_open(false);

    // 26: a blocked contact (no Message app yet)
    c.set_can_message(false);
    c.set_can_email(false);
    c.set_card_name("Անի Պետրոսյան".into());
    c.set_card_company("".into());
    c.set_card_initials("ԱՊ".into());
    c.set_card_tint(rgb(phone_core_desaturated()));
    c.set_card_favourite(false);
    c.set_card_numbers(model(vec![ContactNumber {
        label: "Mobile".into(),
        number: "+37428123456".into(),
    }]));
    c.set_card_email("ani@example.am".into());
    c.set_card_note("".into());
    c.set_card_speed(0);
    c.set_card_blocked("Calls and messages".into());
    s.shoot("contacts-blocked");

    // 24: the block query
    c.set_card_blocked("".into());
    c.set_card_tint(rgb(0xd6409f));
    c.set_dialog_question("Block Անի Պետրոսյան?".into());
    c.set_dialog_detail(
        "Blocked calls are rejected as busy and never ring. You can still call them.".into(),
    );
    c.set_dialog_confirm("Block".into());
    c.set_dialog_checks(strings(&["Calls", "Messages"]));
    c.set_dialog_checked(model(vec![true, true]));
    c.set_dialog_open(true);
    s.shoot("contacts-block");
    c.set_dialog_open(false);

    // 19: the editor
    nav.set_view("edit".into());
    c.set_edit_title("New contact".into());
    c.set_labels(strings(&[
        "Mobile",
        "Mobile (home)",
        "Mobile (work)",
        "Telephone",
        "Telephone (home)",
        "Telephone (work)",
        "Fax",
    ]));
    c.set_edit_first("Ava".into());
    c.set_edit_last("Lindqvist".into());
    c.set_edit_numbers(model(vec![
        ContactEditNumber {
            kind: 0,
            number: "+15550142".into(),
        },
        ContactEditNumber {
            kind: 2,
            number: "".into(),
        },
    ]));
    tap(&s, 200.0 * dp, (52.0 + 14.0 + 22.0) * dp, true);
    s.shoot("contacts-edit");

    // 20: the number-type picker
    c.set_picking(0);
    s.shoot("contacts-edit-type");
    c.set_picking(-1);

    // an empty book
    nav.set_view("".into());
    c.set_total(0);
    c.set_favourites(model(vec![]));
    c.set_sections(model(vec![]));
    c.set_index(model(vec![]));
    s.shoot("contacts-empty");
}

/// The blocked card's avatar: 0xd6409f at 45 % saturation (phone_core::people::desaturate).
fn phone_core_desaturated() -> u32 {
    let (r, g, b) = (0xd6 as f32, 0x40 as f32, 0x9f as f32);
    let grey = 0.299 * r + 0.587 * g + 0.114 * b;
    let m = |c: f32| (grey + (c - grey) * 0.45) as u32;
    m(r) << 16 | m(g) << 8 | m(b)
}
