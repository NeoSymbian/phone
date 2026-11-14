// SPDX-License-Identifier: GPL-3.0-only
// Phone (mockups 3-7, 27): the dialer empty and typing, Recents (All, Blocked), call details, speed dial and its
// picker. The popups that slide in (Options, a row's sheet) are in phone_popups.rs: the harness's clock stands
// still, so they are drawn open from the first frame there.
mod common;
use phone_ui::{DetailCall, DialState, LogRow, MatchRow, Nav, PickRow, SpeedTile};
use slint::{ComponentHandle, ModelRc, VecModel};
use std::rc::Rc;

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(v)))
}

#[allow(clippy::too_many_arguments)]
fn row(
    id: i32,
    section: &str,
    title: &str,
    count: i32,
    kind: i32,
    line: &str,
    when: &str,
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
        when: when.into(),
        initials: initials.into(),
        tint,
        private: false,
    }
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

    // 3: the dialer, empty: recent calls above the keypad
    d.set_recent(model(vec![
        row(9, "", "Ava Lindqvist", 2, 2, "", "21:03", "AL", 0),
        row(7, "", "+1 555 0123", 1, 1, "", "Yesterday", "", -1),
        row(6, "", "Անի Պետրոսյան", 1, 3, "", "14 Nov", "ԱՊ", 1),
        row(5, "", "Борис Окафор", 1, 2, "", "12 Nov", "БО", 2),
    ]));
    s.shoot("phone-dialer");

    // 4: typing 28: Ava by name, Ani by number
    let mut a = MatchRow {
        id: 1,
        label: "Mobile".into(),
        initials: "AL".into(),
        tint: 0,
        ..MatchRow::default()
    };
    split(&mut a, ["", "Av", "a Lindqvist"], ["+15550142", "", ""]);
    let mut b = MatchRow {
        id: 2,
        label: "Mobile".into(),
        initials: "AP".into(),
        tint: 1,
        ..MatchRow::default()
    };
    split(&mut b, ["Ani Petrosyan", "", ""], ["+374", "28", "123456"]);
    d.set_matches(model(vec![a, b]));
    d.set_number("28".into());
    s.shoot("phone-dialer-typing");

    // 5: Recent calls, All
    nav.set_view("recents".into());
    d.set_missed_new(1);
    d.set_has_blocked(true);
    d.set_blocked_count(3);
    d.set_rows(model(vec![
        row(
            9,
            "Today",
            "Ava Lindqvist",
            2,
            2,
            "Mobile · 21:03",
            "",
            "AL",
            0,
        ),
        row(
            8,
            "Today",
            "Ani Petrosyan",
            1,
            3,
            "Missed · 20:41",
            "",
            "AP",
            1,
        ),
        row(
            7,
            "Yesterday",
            "+1 555 0123",
            1,
            1,
            "Dialled · 09:15",
            "",
            "",
            -1,
        ),
        row(
            5,
            "Earlier",
            "Борис Окафор",
            1,
            2,
            "Work · 12 Nov",
            "",
            "БО",
            2,
        ),
    ]));
    s.shoot("phone-recents");

    // 27: Blocked
    d.set_filter(4);
    let mut private = row(
        3,
        "Today",
        "Private number",
        1,
        5,
        "Private numbers · 08:47",
        "",
        "",
        -1,
    );
    private.private = true;
    d.set_rows(model(vec![
        row(
            4,
            "Today",
            "+1 900 555 0101",
            2,
            5,
            "Prefix +1 900 · 10:12",
            "",
            "",
            -1,
        ),
        private,
        row(
            2,
            "Yesterday",
            "Անի Պետրոսյան",
            1,
            5,
            "Blocked contact · 19:30",
            "",
            "ԱՊ",
            1,
        ),
    ]));
    s.shoot("phone-recents-blocked");

    // 6: call details
    nav.set_view("detail".into());
    d.set_detail_title("Ava Lindqvist".into());
    d.set_detail_number("+15550142 · Mobile".into());
    d.set_detail_initials("AL".into());
    d.set_detail_tint(0);
    d.set_detail_known(true);
    d.set_detail_can_call(true);
    let call = |kind: i32, when: &str, duration: &str| DetailCall {
        kind,
        when: when.into(),
        duration: duration.into(),
    };
    d.set_detail_calls(model(vec![
        call(2, "Today 21:03", "4:12"),
        call(3, "Today 20:41", ""),
        call(1, "Yesterday 09:15", "0:42"),
    ]));
    s.shoot("phone-detail");

    // 7: speed dial
    nav.set_view("speed".into());
    let tile = |key: i32, name: &str, initials: &str, tint: i32| SpeedTile {
        key,
        name: name.into(),
        initials: initials.into(),
        tint,
    };
    d.set_speed(model(vec![
        tile(1, "", "", 0),
        tile(2, "Ava L.", "AL", 0),
        tile(3, "", "", 0),
        tile(4, "", "", 0),
        tile(5, "Անի Պ.", "ԱՊ", 1),
        tile(6, "", "", 0),
        tile(7, "", "", 0),
        tile(8, "Борис О.", "БО", 2),
        tile(9, "", "", 0),
    ]));
    s.shoot("phone-speed");

    // the picker for an empty key
    let pick = |title: &str, number: &str| PickRow {
        title: title.into(),
        number: number.into(),
    };
    d.set_pick_heading("Key 3".into());
    d.set_pick(model(vec![
        pick("Ava Lindqvist", "+15550142"),
        pick("Ben Okafor", "+15550188"),
        pick("Борис Окафор", "+15550177"),
        pick("Անի Պետրոսյան", "+37428123456"),
    ]));
    d.set_pick_open(true);
    s.shoot("phone-speed-pick");
}
