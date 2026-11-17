// SPDX-License-Identifier: GPL-3.0-only
// Phone (NEOPHONE v2 p2-phone-*): Recents (All, Blocked, selection, Buttons mode), the keypad sheet empty and typing,
// call details, speed dial and its contact sheet, filled with the boards' sample data; and what taps send to the app.
mod common;
use common::{BUTTONS, Header, model};
use phone_ui::{DetailCall, DialState, LogRow, MatchRow, Nav, PickRow, SpeedTile};
use slint::ComponentHandle;

// ui-shared/icons.slint ids (symbian/common ui.rs `tool`)
const DELETE: i32 = 6;
const BLOCK: i32 = 10;

#[allow(clippy::too_many_arguments)]
fn row(
    id: i32,
    section: &str,
    title: &str,
    count: i32,
    kind: i32,
    line: &str,
    initials: &str,
    tint: i32,
) -> LogRow {
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
}

fn recents() -> Vec<LogRow> {
    vec![
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
    ]
}

fn root_header(s: &common::Shot, footer: bool) {
    s.header(Header {
        title: "Phone",
        search: true,
        options: &["Speed dial", "Call voicemail", "Clear list", "Settings"],
        footer,
        ..Header::default()
    });
}

fn split(m: &mut MatchRow, name: [&str; 3], num: [&str; 3]) {
    m.name_pre = name[0].into();
    m.name_hit = name[1].into();
    m.name_post = name[2].into();
    m.num_pre = num[0].into();
    m.num_hit = num[1].into();
    m.num_post = num[2].into();
}

#[test]
fn phone() {
    let s = common::app();
    let nav = s.ui.global::<Nav>();
    let d = s.ui.global::<DialState>();
    nav.set_page(0);
    nav.set_view("".into());
    root_header(&s, true);
    d.set_missed_new(2);
    d.set_has_blocked(true);
    d.set_blocked_count(3);
    d.set_rows(model(recents()));
    s.shoot("p2-phone-recents");

    // the chips and the Fab ask the app
    s.take_acts();
    s.tap(360.0 - 16.0 - 28.0, 679.0 - 48.0 - 16.0 - 28.0); // the Fab, above the footer
    s.tap(100.0, 48.0 + 26.0); // Missed
    assert_eq!(
        s.take_acts(),
        [
            ("keypad".into(), String::new()),
            ("filter".into(), "1".into())
        ]
    );

    s.shoot_at("p2-phone-recents-buttons", BUTTONS);
    s.shoot("p2-phone-recents");

    // Blocked: the chips scroll so it shows; why each call was blocked
    d.set_filter(4);
    d.set_rows(model(vec![
        row(
            6,
            "Today",
            "+1 900 555 0101",
            2,
            5,
            "Prefix +1 900 · 10:12",
            "",
            -1,
        ),
        LogRow {
            private: true,
            ..row(
                3,
                "Today",
                "Private number",
                1,
                5,
                "Private numbers · 08:47",
                "",
                -1,
            )
        },
        row(
            2,
            "Yesterday",
            "Անի Պետրոսյան",
            1,
            5,
            "Blocked contact · 19:30",
            "ԱՊ",
            1,
        ),
    ]));
    s.shoot("p2-phone-recents-blocked");

    // selection mode: two rows ticked, [✕ 2 selected · block · delete], no footer, no Fab
    d.set_filter(0);
    let mut rows = recents();
    rows[0].checked = true;
    rows[2].checked = true;
    d.set_rows(model(rows));
    d.set_selecting(true);
    s.header(Header {
        title: "Phone",
        selected: 2,
        selection: &[(BLOCK, "Block"), (DELETE, "Delete")],
        ..Header::default()
    });
    s.shoot("p2-phone-select");
    d.set_selecting(false);
    d.set_rows(model(recents()));

    // the keypad sheet over Recents, empty, then typing 28 (Ava by name, Ani by number)
    root_header(&s, false);
    nav.set_view("dial".into());
    s.shoot("p2-phone-keypad");
    s.take_acts();
    s.tap(180.0 - 115.0, 679.0 - 428.0 + 16.0 + 56.0 + 31.0); // key 1
    s.tap(180.0, 679.0 - 50.0); // Call
    assert_eq!(
        s.take_acts(),
        [("key".into(), "1".into()), ("call".into(), String::new())]
    );
    let mut a = MatchRow {
        id: 1,
        label: "Mobile".into(),
        initials: "AL".into(),
        tint: 0,
        ..MatchRow::default()
    };
    split(&mut a, ["", "Av", "a Lindqvist"], ["+1 555 0142", "", ""]);
    let mut b = MatchRow {
        id: 2,
        label: "Mobile".into(),
        initials: "AP".into(),
        tint: 1,
        ..MatchRow::default()
    };
    split(
        &mut b,
        ["Ani Petrosyan", "", ""],
        ["+374", " 28", " 123456"],
    );
    d.set_matches(model(vec![a, b]));
    d.set_number("28".into());
    s.shoot("p2-phone-typing");
    d.set_number("".into());

    // call details
    nav.set_view("detail".into());
    s.header(Header {
        title: "Ava Lindqvist",
        sub: true,
        options: &["Delete from log"],
        ..Header::default()
    });
    d.set_detail_number("+1 555 0142 · Mobile".into());
    d.set_detail_initials("AL".into());
    d.set_detail_tint(0);
    d.set_detail_known(true);
    d.set_detail_can_call(true);
    let call = |kind, when: &str, duration: &str| DetailCall {
        kind,
        when: when.into(),
        duration: duration.into(),
    };
    d.set_detail_calls(model(vec![
        call(2, "Today 21:03", "4:12"),
        call(3, "Today 20:41", ""),
        call(1, "Yesterday 09:15", "0:42"),
    ]));
    s.shoot("p2-phone-details");
    s.shoot_at("p2-phone-details-buttons", BUTTONS);

    // speed dial, then Assign on key 3: the contacts' sheet
    nav.set_view("speed".into());
    s.header(Header {
        title: "Speed dial",
        sub: true,
        options: &["Settings"],
        ..Header::default()
    });
    let tile = |key, name: &str, initials: &str, tint| SpeedTile {
        key,
        name: name.into(),
        initials: initials.into(),
        tint,
    };
    d.set_speed(model(vec![
        tile(1, "", "", -1),
        tile(2, "Ava L.", "AL", 0),
        tile(3, "", "", -1),
        tile(4, "", "", -1),
        tile(5, "Անի Պ.", "ԱՊ", 1),
        tile(6, "", "", -1),
        tile(7, "", "", -1),
        tile(8, "Борис О.", "БО", 3),
        tile(9, "", "", -1),
    ]));
    s.shoot("p2-phone-speed");
    s.take_acts();
    s.tap(300.0, 48.0 + 70.0); // key 3
    assert_eq!(s.take_acts(), [("speed".into(), "3".into())]);
    let pick = |title: &str, number: &str, initials: &str, tint| PickRow {
        title: title.into(),
        number: number.into(),
        initials: initials.into(),
        tint,
    };
    d.set_pick(model(vec![
        pick("Ava Lindqvist", "+1 555 0142", "AL", 0),
        pick("Ben Okafor", "+1 555 0188", "BO", 2),
        pick("Борис Окафор", "+1 555 0177", "БО", 3),
        pick("Անի Պետրոսյան", "+374 28 123456", "ԱՊ", 1),
    ]));
    d.set_pick_heading("Key 3".into());
    d.set_pick_open(true);
    s.shoot("p2-phone-speed-pick");
}
