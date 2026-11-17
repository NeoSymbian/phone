// SPDX-License-Identifier: GPL-3.0-only
// Phone ▸ ⋮ (p2-phone-menu): the kit header's menu over Recents. Its own binary: the menu is a popup the tap opens.
mod common;
use common::{Header, model};
use phone_ui::{DialState, LogRow, Nav};
use slint::ComponentHandle;

#[test]
fn phone_menu() {
    let s = common::app();
    let d = s.ui.global::<DialState>();
    let nav = s.ui.global::<Nav>();
    nav.set_page(0);
    nav.set_view("".into());
    s.header(Header {
        title: "Phone",
        search: true,
        options: &["Speed dial", "Call voicemail", "Clear list", "Settings"],
        footer: true,
        ..Header::default()
    });
    let row =
        |id: i32, section: &str, title: &str, count, kind, line: &str, initials: &str, tint| {
            LogRow {
                id,
                section: section.into(),
                title: title.into(),
                count,
                kind,
                line: line.into(),
                initials: initials.into(),
                tint,
                ..LogRow::default()
            }
        };
    d.set_missed_new(2);
    d.set_has_blocked(true);
    d.set_blocked_count(3);
    d.set_rows(model(vec![
        row(9, "Today", "Ava Lindqvist", 2, 2, "Mobile · 21:03", "AL", 0),
        row(8, "Today", "Ani Petrosyan", 2, 3, "Missed · 20:41", "AP", 1),
        row(
            7,
            "Yesterday",
            "+1 555 0123",
            1,
            1,
            "Dialled · 09:15",
            "",
            -1,
        ),
        row(5, "Earlier", "Ben Okafor", 1, 2, "Work · 12 Nov", "BO", 2),
        row(
            4,
            "Earlier",
            "Борис Окафор",
            1,
            1,
            "Mobile · 10 Nov",
            "БО",
            3,
        ),
    ]));
    s.shoot("p2-phone-recents");
    // what the frame reports to the app's Rust (symbian/phone lib.rs on_header)
    let taps: std::rc::Rc<std::cell::RefCell<Vec<(String, i32)>>> = Default::default();
    let sink = taps.clone();
    nav.on_header(move |kind, i| sink.borrow_mut().push((kind.into(), i)));
    s.tap(360.0 - 4.0 - 24.0, 24.0); // ⋮
    s.shoot("p2-phone-menu");
    // ⋮ ▸ Settings (the fourth item: 8 dp + 3.5 rows of 48 dp under the header) reaches the app
    s.tap(360.0 - 4.0 - 110.0, 48.0 + 8.0 + 3.5 * 48.0);
    assert_eq!(
        taps.borrow().last(),
        Some(&("option".into(), 3)),
        "⋮ ▸ Settings"
    );
    // a form's ✓ is the view's action ("save"), whatever its style
    nav.set_form(true);
    nav.set_title("Voicemail number".into());
    nav.set_has_action(true);
    nav.set_action_tool(5);
    s.tap(360.0 - 24.0, 24.0);
    assert_eq!(
        taps.borrow().last(),
        Some(&("save".into(), 0)),
        "the form's ✓"
    );
    nav.set_form(false);
}
