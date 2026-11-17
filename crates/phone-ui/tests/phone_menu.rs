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
    s.tap(360.0 - 4.0 - 24.0, 24.0); // ⋮
    s.shoot("p2-phone-menu");
}
