// SPDX-License-Identifier: GPL-3.0-only
// The call screens (NEOPHONE v2 p2-call-*), full screen with the boards' callers: incoming (a contact, an unknown
// number silenced, a private number) and its Reply with message sheet, calling, ringing, active, held and muted,
// call waiting, two calls, a conference, the keypad, the summary (with and without a duration); what their buttons send.
mod common;
use common::{FULL, strings};
use phone_ui::{CallState, Nav, Party};
use slint::{Color, ComponentHandle};

fn party(name: &str, line: &str, label: &str, initials: &str, tint: u32) -> Party {
    Party {
        name: name.into(),
        line: line.into(),
        label: label.into(),
        initials: initials.into(),
        tint: Color::from_argb_encoded(0xff00_0000 | tint),
        group: false,
    }
}

#[test]
fn call() {
    let s = common::app();
    let nav = s.ui.global::<Nav>();
    nav.set_page(3);
    nav.set_full(true);
    let c = s.ui.global::<CallState>();
    let ava = party("Ava Lindqvist", "+1 555 0142", "Mobile", "AL", 0x15803d);
    let ani = party("Ani Petrosyan", "+374 28 123456", "Mobile", "AP", 0xbe185d);
    let shot = |name: &str| s.shoot_at(name, FULL);

    c.set_can_message(true);
    c.set_replies(strings(&[
        "Can't talk now. I'll call you later.",
        "I'm in a meeting.",
        "I'm driving. I'll call you back.",
        "Write your own…",
    ]));
    c.set_screen("incoming".into());
    c.set_main(ava.clone());
    shot("p2-call-incoming");
    s.take_acts();
    s.tap(82.0, 589.0); // Reject
    s.tap(278.0, 589.0); // Answer
    assert_eq!(
        s.take_acts(),
        [
            ("reject".into(), String::new()),
            ("answer".into(), String::new())
        ]
    );
    c.set_replying(true);
    shot("p2-call-reply");
    c.set_replying(false);
    c.set_main(party("+1 555 0199", "", "", "", 0));
    c.set_silenced(true);
    shot("p2-call-incoming-unknown");
    c.set_silenced(false);
    // a private number can't be messaged
    c.set_can_message(false);
    c.set_main(party("Private number", "", "", "", 0));
    shot("p2-call-incoming-private");
    c.set_can_message(true);

    c.set_screen("calling".into());
    c.set_main(ava.clone());
    c.set_main_time("Calling…".into());
    shot("p2-call-calling");
    c.set_main(party(
        "Անի Պետրոսյան",
        "+1 555 0168",
        "Mobile (work)",
        "ԱՊ",
        0xbe185d,
    ));
    c.set_main_time("Ringing…".into());
    shot("p2-call-ringing");

    c.set_screen("active".into());
    c.set_main(ava.clone());
    c.set_main_time("04:12".into());
    shot("p2-call-active");
    // Buttons mode: the same screen; the shell's key row peeks over it for 3 s
    shot("p2-call-buttons");
    s.take_acts();
    s.tap(76.0, 398.0); // Mute
    s.tap(76.0, 496.0); // Hold
    s.tap(180.0, 612.0); // End
    assert_eq!(
        s.take_acts(),
        [
            ("mute".into(), String::new()),
            ("hold".into(), String::new()),
            ("end".into(), String::new())
        ]
    );
    c.set_muted(true);
    c.set_held(true);
    c.set_main_time("On hold".into());
    shot("p2-call-held");
    c.set_muted(false);
    c.set_held(false);

    c.set_screen("waiting".into());
    c.set_main_time("04:12".into());
    c.set_other(ani.clone());
    shot("p2-call-waiting");

    c.set_screen("two".into());
    c.set_main(ani);
    c.set_main_time("00:37".into());
    c.set_other(ava.clone());
    c.set_other_time("On hold · 04:40".into());
    shot("p2-call-two");

    // after Conference (Merge): the active call's screen, "Conference" over the callers, the group glyph
    c.set_screen("conference".into());
    c.set_main(Party {
        name: "Conference".into(),
        line: "Ani Petrosyan, Ava Lindqvist".into(),
        group: true,
        ..Party::default()
    });
    c.set_main_time("04:40".into());
    c.set_other(Party::default());
    shot("p2-call-conference");

    c.set_screen("active".into());
    c.set_main(ava.clone());
    c.set_main_time("04:31".into());
    c.set_keypad(true);
    c.set_tones("12#".into());
    shot("p2-call-keypad");
    // one tone per press, tapped or held
    s.take_acts();
    s.tap(180.0, 418.0); // 5
    s.hold(180.0, 534.0); // 0
    assert_eq!(
        s.take_acts(),
        [("dtmf".into(), "5".into()), ("dtmf".into(), "0".into())]
    );
    c.set_keypad(false);

    c.set_screen("ended".into());
    c.set_duration("4:31".into());
    shot("p2-call-summary");
    // a call that never connected: no duration
    c.set_duration("".into());
    shot("p2-call-summary-missed");
}
