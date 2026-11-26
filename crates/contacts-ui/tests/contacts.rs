// SPDX-License-Identifier: GPL-3.0-only
// Contacts v2 (NEOPHONE v2 boards p2-contacts-*): the list with the letter strip, search, the index bubble, the card
// (blocked too), the editor and its number-type sheet, the Delete and Block queries, ⋮, selection mode, the empty
// book, and the list and editor in Buttons mode. Each snapshot is named like its board, filled with its sample data.
mod common;
use contacts_ui::{
    ContactEditNumber, ContactHit, ContactMark, ContactNumber, ContactRow, ContactSection,
    ContactsState, Icons, Nav,
};
use slint::platform::WindowEvent;
use slint::{Color, ComponentHandle, Model, ModelRc, SharedString, VecModel};
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

/// The boards' book: every letter the strip shows has someone (the names the boards show, and more).
const BOOK: &[(&str, &[Person])] = &[
    (
        "A",
        &[
            ("Aram Hakobyan", "AH", 0x14a3a3),
            ("Ava Lindqvist", "AL", 0x22b46a),
        ],
    ),
    ("B", &[("Ben Okafor", "BO", 0xe8833a)]),
    ("D", &[("Dario Rossi", "DR", 0xc2410c)]),
    ("E", &[("Elen Sargsyan", "ES", 0x0e7490)]),
    ("F", &[("Felix Brandt", "FB", 0x2f80ed)]),
    ("G", &[("Grace Nakamura", "GN", 0xe5484d)]),
    ("H", &[("Hana Kim", "HK", 0x8b5cf6)]),
    ("I", &[("Ivan Novak", "IN", 0x14a37f)]),
    ("J", &[("Jonas Weber", "JW", 0xd6409f)]),
    ("K", &[("Kofi Mensah", "KM", 0xe8833a)]),
    ("L", &[("Lena Fischer", "LF", 0x22b46a)]),
    ("M", &[("Marco Bianchi", "MB", 0x2f80ed)]),
    ("N", &[("Nora Hansen", "NH", 0x14a3a3)]),
    ("O", &[("Omar Haddad", "OH", 0xc2410c)]),
    ("P", &[("Priya Shah", "PS", 0x8b5cf6)]),
    ("R", &[("Rosa Diaz", "RD", 0xe5484d)]),
    ("S", &[("Sofia Russo", "SR", 0xe5484d)]),
    ("T", &[("Tomas Berg", "TB", 0x0e7490)]),
    ("Z", &[("Zara Ali", "ZA", 0x14a37f)]),
    (
        "Ա",
        &[
            ("Անի Պետրոսյան", "ԱՊ", 0xd6409f),
            ("Աննա Մելքոնյան", "ԱՄ", 0x8656e8),
            ("Արամ Սարգսյան", "ԱՍ", 0x14a37f),
        ],
    ),
    ("Գ", &[("Գոռ Սարգսյան", "ԳՍ", 0x0f9f8f)]),
    ("Ե", &[("Երանուհի Ավագյան", "ԵԱ", 0xc2410c)]),
    ("Լ", &[("Լիլիթ Հովհաննիսյան", "ԼՀ", 0x2f80ed)]),
    ("Հ", &[("Հովիկ Մարտիրոսյան", "ՀՄ", 0x4b5563)]),
    ("Մ", &[("Մարիամ Գրիգորյան", "ՄԳ", 0xd6409f)]),
    ("Ն", &[("Նարեկ Դավթյան", "ՆԴ", 0x14a3a3)]),
    ("Ս", &[("Սոնա Հարությունյան", "ՍՀ", 0xe8833a)]),
    ("А", &[("Анна Смирнова", "АС", 0x8b5cf6)]),
    ("Д", &[("Дмитрий Орлов", "ДО", 0x2f80ed)]),
    ("И", &[("Иван Петров", "ИП", 0x14a37f)]),
    ("К", &[("Ксения Волкова", "КВ", 0xe5484d)]),
    ("М", &[("Мария Ковалёва", "МК", 0x8b5cf6)]),
    ("О", &[("Ольга Соколова", "ОС", 0x22b46a)]),
    ("С", &[("Сергей Иванов", "СИ", 0xe8833a)]),
    ("#", &[("555-0100", "#", 0x2f80ed)]),
];

/// The sections and each one's top (band 34 + rows 56 dp, as phone_core::people::tops).
fn sections(marked: &[&str]) -> (Vec<ContactSection>, Vec<f32>) {
    let mut top = 0.0;
    let mut tops = Vec::new();
    let s = BOOK
        .iter()
        .enumerate()
        .map(|(k, (l, people))| {
            tops.push(top);
            let s = ContactSection {
                letter: (*l).into(),
                top,
                people: model(
                    people
                        .iter()
                        .enumerate()
                        .map(|(i, (n, a, t))| ContactRow {
                            marked: marked.contains(n),
                            ..row((k * 10 + i) as i32, n, a, *t)
                        })
                        .collect(),
                ),
            };
            top += 34.0 + 56.0 * people.len() as f32;
            s
        })
        .collect();
    (s, tops)
}

#[derive(Clone, Copy, PartialEq)]
enum Script {
    Latin,
    Armenian,
    Cyrillic,
    Other,
}

fn script(letter: &str) -> Script {
    match letter.chars().next().unwrap_or('#') {
        'A'..='Z' => Script::Latin,
        'Ա'..='Ֆ' => Script::Armenian,
        'А'..='Я' => Script::Cyrillic,
        _ => Script::Other,
    }
}

/// The strip as the device builds it for an app `h` dp high: every section's letter and a dash between scripts, in
/// the places phone_core::people::index_room gives the list (the app less the header and footer, less the strip's
/// margins), a script that does not fit folding its middle into "•" (contacts::index::side_index).
// shortcut: a copy of index_room and side_index (contacts-ui has no dev-dependency on contacts or phone-core); call
// them once it does
fn index(tops: &[f32], h: f32) -> Vec<ContactMark> {
    let room = ((h - 48.0 - 48.0 - 24.0) / 15.0).max(3.0) as usize;
    let mut runs: Vec<Vec<usize>> = Vec::new();
    for (i, (l, _)) in BOOK.iter().enumerate() {
        match runs.last_mut() {
            Some(r) if script(BOOK[r[0]].0) == script(l) => r.push(i),
            _ => runs.push(vec![i]),
        }
    }
    let lettered = |r: &Vec<usize>| script(BOOK[r[0]].0) != Script::Other;
    let room = room - (runs.iter().filter(|r| lettered(r)).count() - 1);
    let mut budget: Vec<usize> = runs.iter().map(Vec::len).collect();
    while budget.iter().sum::<usize>() > room {
        let (i, _) = budget
            .iter()
            .enumerate()
            .filter(|&(_, &b)| b > 1)
            .rev()
            .max_by_key(|&(_, &b)| b)
            .unwrap();
        budget[i] -= 1;
    }
    // (text, the sections it stands for)
    let mut marks: Vec<(String, Vec<usize>)> = Vec::new();
    for (k, (r, &b)) in runs.iter().zip(&budget).enumerate() {
        if k > 0 && lettered(r) {
            marks.push(("–".into(), vec![]));
        }
        let one = |i: usize| (BOOK[i].0.to_string(), vec![i]);
        if b >= r.len() {
            marks.extend(r.iter().map(|&i| one(i)));
        } else {
            let head = b / 2;
            let tail = b - 1 - head;
            marks.extend(r[..head].iter().map(|&i| one(i)));
            marks.push(("•".into(), r[head..r.len() - tail].to_vec()));
            marks.extend(r[r.len() - tail..].iter().map(|&i| one(i)));
        }
    }
    let start = |m: &(String, Vec<usize>)| m.1.first().map(|&i| tops[i]);
    marks
        .iter()
        .enumerate()
        .map(|(n, m)| {
            let (start, end) = match start(m) {
                Some(s) => (
                    if n == 0 { -1e9 } else { s },
                    marks[n + 1..].iter().find_map(start).unwrap_or(1e9),
                ),
                None => (0.0, 0.0),
            };
            ContactMark {
                text: m.0.as_str().into(),
                letters: model(m.1.iter().map(|&i| BOOK[i].0.into()).collect()),
                tops: model(m.1.iter().map(|&i| tops[i]).collect()),
                start,
                end,
            }
        })
        .collect()
}

/// The header as symbian/contacts ctx.rs sets it from the view's Chrome.
fn header(s: &common::Shot, title: &str, sub: bool, acts: &[(i32, &str)], options: &[&str]) {
    let nav = s.ui.global::<Nav>();
    let icons = s.ui.global::<Icons>();
    nav.set_title(title.into());
    nav.set_sub(sub);
    nav.set_form(false);
    nav.set_actions(model(
        acts.iter()
            .map(|(t, l)| icons.invoke_act(*t, (*l).into()))
            .collect(),
    ));
    nav.set_options(strings(options));
}

const LIST_OPTIONS: &[&str] = &["Mark several", "Import vCards", "Export vCards", "Settings"];

fn list_header(s: &common::Shot) {
    header(
        s,
        "Contacts",
        false,
        &[(14, "Search contacts"), (3, "New contact")],
        LIST_OPTIONS,
    );
    s.ui.global::<Nav>().set_footer(true);
}

fn editor(s: &common::Shot) {
    let nav = s.ui.global::<Nav>();
    let c = s.ui.global::<ContactsState>();
    nav.set_view("edit".into());
    nav.set_title("Edit contact".into());
    nav.set_form(true);
    nav.set_has_action(true);
    // the ✓ (Chrome's tool::CHECK)
    nav.set_action_tool(5);
    nav.set_footer(false);
    c.set_edit_first("Ava".into());
    c.set_edit_last("Lindqvist".into());
    c.set_edit_company("Example Co".into());
    c.set_edit_email("ava@example.org".into());
    c.set_edit_note("Met at the Yerevan meetup".into());
    c.set_edit_bday_day(14);
    c.set_edit_bday_month(3);
    c.set_edit_bday_year(1990);
    c.set_edit_numbers(model(vec![
        ContactEditNumber {
            kind: 0,
            number: "+1 555 0142".into(),
        },
        ContactEditNumber {
            kind: 5,
            number: "+1 555 0177".into(),
        },
    ]));
}

#[test]
fn contacts() {
    let s = common::app();
    let nav = s.ui.global::<Nav>();
    let c = s.ui.global::<ContactsState>();
    nav.set_page(1);
    nav.set_missed(2);
    nav.set_unread(3);

    // the list
    list_header(&s);
    c.set_total(42);
    c.set_favourites(model(vec![
        row(1, "Ava", "AL", 0x22b46a),
        row(2, "Անի", "ԱՊ", 0xd6409f),
        row(3, "Ben", "BO", 0xe8833a),
        row(4, "Мария", "МК", 0x8b5cf6),
    ]));
    let (secs, tops) = sections(&[]);
    c.set_sections(model(secs));
    c.set_index(model(index(&tops, 679.0)));
    c.set_labels(strings(&[
        "Mobile",
        "Mobile (home)",
        "Mobile (work)",
        "Telephone",
        "Telephone (home)",
        "Telephone (work)",
        "Fax",
    ]));
    let px = s.shoot("p2-contacts-list");
    assert!(px.iter().any(|&p| p != 0));

    // ⋮: the kit's menu under it
    s.press(360.0 - 4.0 - 24.0, 24.0, true);
    s.shoot("p2-contacts-menu");
    s.press(10.0, 600.0, true); // a tap outside closes it
    s.shoot("p2-contacts-list");

    // a finger on Ա in the strip (item 20: 19 Latin letters, the dash): the list jumps there, the bubble shows it
    s.press(360.0 - 16.0 - 12.0, 48.0 + 12.0 + 20.0 * 15.0 + 7.0, false);
    s.shoot("p2-contacts-index");
    s.ui.window().dispatch_event(WindowEvent::PointerExited);

    // the header's search
    nav.set_footer(false);
    nav.set_searching(true);
    nav.set_search_hint("Search 42 contacts".into());
    nav.set_query("an".into());
    c.set_has_query(true);
    c.set_results(model(vec![
        ContactHit {
            id: 1,
            initials: "ԱՊ".into(),
            tint: rgb(0xd6409f),
            name_pre: "".into(),
            name_hit: "An".into(),
            name_post: "i Petrosyan".into(),
            detail_pre: "Mobile · ".into(),
            detail_hit: "".into(),
            detail_post: "+374 28 123456".into(),
        },
        ContactHit {
            id: 2,
            initials: "AH".into(),
            tint: rgb(0x14a3a3),
            name_pre: "Aram Hakobyan".into(),
            name_hit: "".into(),
            name_post: "".into(),
            detail_pre: "Company: Vah".into(),
            detail_hit: "an".into(),
            detail_post: " & Co".into(),
        },
    ]));
    s.shoot("p2-contacts-search");
    // nothing matches
    nav.set_query("xq".into());
    c.set_results(model(vec![]));
    s.shoot("p2-contacts-search-none");
    nav.set_searching(false);
    nav.set_query("".into());
    c.set_has_query(false);

    // selection mode: three marked, the footer hidden; [✕ 3 selected · 🗑 · ⋮]
    nav.set_view("mark".into());
    nav.set_footer(false);
    nav.set_selecting(true);
    nav.set_selected(3);
    nav.set_selection_actions(model(vec![
        s.ui.global::<Icons>().invoke_act(6, "Delete marked".into()),
    ]));
    nav.set_selection_menu(strings(&["Select all", "Export"]));
    let (marked, _) = sections(&["Ava Lindqvist", "Ben Okafor", "Felix Brandt"]);
    c.set_sections(model(marked));
    s.shoot("p2-contacts-select");
    nav.set_selected(0);
    nav.set_selecting(false);

    // the card
    nav.set_view("card".into());
    header(
        &s,
        "Ava Lindqvist",
        true,
        &[(8, "Remove from favourites"), (4, "Edit")],
        &["Delete", "Speed dial", "Block contact", "Send as vCard"],
    );
    c.set_card_name("Ava Lindqvist".into());
    c.set_card_company("Example Co".into());
    c.set_card_initials("AL".into());
    c.set_card_tint(rgb(0x22b46a));
    c.set_card_numbers(model(vec![
        ContactNumber {
            label: "Mobile".into(),
            number: "+1 555 0142".into(),
        },
        ContactNumber {
            label: "Telephone (work)".into(),
            number: "+1 555 0177".into(),
        },
    ]));
    c.set_card_email("ava@example.org".into());
    c.set_card_note("Met at the Yerevan meetup".into());
    c.set_card_speed(2);
    c.set_card_birthday("14 March · 36 years".into());
    s.shoot("p2-contacts-card");
    c.set_card_birthday("".into());

    // Delete: on the device the shell's QUERY asks; the app's query draws the same box here
    c.set_dialog_question("Delete Ava Lindqvist?".into());
    c.set_dialog_detail("Its speed dial key 2 is cleared too.".into());
    c.set_dialog_confirm("Delete".into());
    c.set_dialog_checks(strings(&[]));
    c.set_dialog_checked(model(vec![]));
    c.set_dialog_open(true);
    s.shoot("p2-contacts-delete");
    c.set_dialog_open(false);

    // a blocked contact
    header(
        &s,
        "Անի Պետրոսյան",
        true,
        &[(7, "Add to favourites"), (4, "Edit")],
        &["Delete", "Speed dial", "Unblock contact", "Send as vCard"],
    );
    c.set_card_name("Անի Պետրոսյան".into());
    c.set_card_company("".into());
    c.set_card_initials("ԱՊ".into());
    c.set_card_tint(rgb(0xd6409f));
    c.set_card_numbers(model(vec![ContactNumber {
        label: "Mobile".into(),
        number: "+374 28 123456".into(),
    }]));
    c.set_card_email("ani@example.am".into());
    c.set_card_note("".into());
    c.set_card_speed(0);
    c.set_card_blocked("Calls and messages".into());
    s.shoot("p2-contacts-blocked-card");

    // the block query, over the card before it was blocked
    c.set_card_blocked("".into());
    c.set_dialog_question("Block Անի Պետրոսյան?".into());
    c.set_dialog_detail(
        "Blocked calls are rejected as busy and never ring. You can still call them.".into(),
    );
    c.set_dialog_confirm("Block".into());
    c.set_dialog_checks(strings(&["Calls", "Messages"]));
    c.set_dialog_checked(model(vec![true, true]));
    c.set_dialog_open(true);
    s.shoot("p2-contacts-block");
    c.set_dialog_open(false);

    // the editor and its number-type sheet
    editor(&s);
    s.shoot("p2-contacts-editor");
    c.set_picking(1);
    s.shoot("p2-contacts-type");
    c.set_picking(-1);

    // the birthday sheet: › on the day, ‹ twice on the month, Set
    c.set_dating(true);
    s.shoot("p2-contacts-birthday");
    // its rows from the bottom: the buttons (8 + 44 + 12), then 56 per stepper (year, month, day)
    let at = |k: f32| 679.0 - 64.0 - 56.0 * k - 28.0;
    let (prev, next) = (360.0 - 12.0 - 48.0 - 124.0 - 24.0, 360.0 - 12.0 - 24.0);
    s.press(next, at(2.0), true);
    s.press(prev, at(1.0), true);
    s.press(prev, at(1.0), true);
    s.press(360.0 - 16.0 - 30.0, 679.0 - 12.0 - 22.0, true);
    s.pixels();
    assert!(!c.get_dating());
    assert_eq!(
        (
            c.get_edit_bday_day(),
            c.get_edit_bday_month(),
            c.get_edit_bday_year()
        ),
        (15, 1, 1990)
    );
    c.set_edit_bday_day(14);
    c.set_edit_bday_month(3);

    // an empty book
    nav.set_view("".into());
    list_header(&s);
    c.set_total(0);
    c.set_favourites(model(vec![]));
    c.set_sections(model(vec![]));
    c.set_index(model(vec![]));
    s.shoot("p2-contacts-empty");

    // Buttons mode: the app is 639 dp high, the strip has 34 places
    s.resize(1917);
    c.set_total(42);
    c.set_favourites(model(vec![
        row(1, "Ava", "AL", 0x22b46a),
        row(2, "Անի", "ԱՊ", 0xd6409f),
        row(3, "Ben", "BO", 0xe8833a),
        row(4, "Мария", "МК", 0x8b5cf6),
    ]));
    let (secs, tops) = sections(&[]);
    c.set_sections(model(secs));
    let strip = index(&tops, 639.0);
    assert_eq!(strip.len(), 34);
    assert!(strip.iter().any(|m| m.text == "•"));
    c.set_index(model(strip));
    s.shoot("p2-contacts-list-buttons");
    editor(&s);
    s.shoot("p2-contacts-editor-buttons");

    // typing into a number writes it into the model the app reads on Save
    c.set_edit_numbers(model(vec![
        ContactEditNumber {
            kind: 0,
            number: "+1 555 0142".into(),
        },
        ContactEditNumber {
            kind: 2,
            number: "".into(),
        },
    ]));
    // the second number: 12 + 96 + 8 (names) + 44 (company) + 34 (band) + 44 + 8 + 22 under the header
    s.press(200.0, 48.0 + 268.0, true);
    for ch in "5550199".chars() {
        let text = SharedString::from(ch.encode_utf8(&mut [0; 4]) as &str);
        s.ui.window()
            .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        s.ui.window()
            .dispatch_event(WindowEvent::KeyReleased { text });
    }
    assert_eq!(c.get_edit_numbers().row_data(1).unwrap().number, "5550199");
}
