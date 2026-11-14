// SPDX-License-Identifier: GPL-3.0-only
// The call screens (mockups 9-15), with invented callers: incoming, calling, active, call waiting, two calls, the
// keypad, the summary.
mod common;
use phone_ui::{CallState, Nav, Party};
use slint::{Color, ComponentHandle};

fn party(name: &str, line: &str, label: &str, initials: &str, tint: u32) -> Party {
    Party {
        name: name.into(),
        line: line.into(),
        label: label.into(),
        initials: initials.into(),
        tint: Color::from_argb_encoded(0xff00_0000 | tint),
    }
}

#[test]
fn call() {
    let s = common::app();
    s.ui.global::<Nav>().set_page(3);
    let c = s.ui.global::<CallState>();
    let ava = party("Ava Lindqvist", "+1 555 0142", "Mobile", "AL", 0x22b46a);
    let ani = party(
        "Անի Պետրոսյան",
        "+1 555 0168",
        "Mobile (work)",
        "ԱՊ",
        0xd6409f,
    );

    c.set_screen("incoming".into());
    c.set_main(ava.clone());
    s.shoot("call-incoming");
    c.set_main(party("+1 555 0199", "", "", "", 0));
    c.set_silenced(true);
    s.shoot("call-incoming-unknown");
    c.set_silenced(false);
    c.set_main(party("Private number", "", "", "", 0));
    s.shoot("call-incoming-private");

    c.set_screen("calling".into());
    c.set_main(ava.clone());
    c.set_main_time("Calling…".into());
    s.shoot("call-calling");
    c.set_main(ani.clone());
    c.set_main_time("Ringing…".into());
    s.shoot("call-ringing");

    c.set_screen("active".into());
    c.set_main(ava.clone());
    c.set_main_time("04:12".into());
    c.set_speaker(true);
    s.shoot("call-active");
    c.set_speaker(false);
    c.set_muted(true);
    c.set_held(true);
    c.set_main_time("On hold".into());
    s.shoot("call-active-held");
    c.set_muted(false);
    c.set_held(false);

    c.set_screen("waiting".into());
    c.set_main_time("04:12".into());
    c.set_other(ani.clone());
    s.shoot("call-waiting");

    c.set_screen("two".into());
    c.set_main(ani);
    c.set_main_time("00:37".into());
    c.set_other(ava.clone());
    c.set_other_time("On hold · 04:40".into());
    s.shoot("call-two");

    c.set_screen("active".into());
    c.set_main(ava.clone());
    c.set_main_time("04:31".into());
    c.set_keypad(true);
    c.set_tones("12#".into());
    s.shoot("call-keypad");
    c.set_keypad(false);

    c.set_screen("ended".into());
    c.set_duration("4:31".into());
    s.shoot("call-summary");
}
