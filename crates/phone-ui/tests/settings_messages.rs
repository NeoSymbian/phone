// SPDX-License-Identifier: GPL-3.0-only
// Settings opened from Messages (p2-msg-settings): the shared page, at its MESSAGES section. Its own binary: the
// page scrolls there as it is made.
mod common;
use common::{Header, strings};
use phone_ui::{Nav, SettingsState};
use slint::ComponentHandle;

#[test]
fn settings_messages() {
    let sh = common::app();
    let nav = sh.ui.global::<Nav>();
    let s = sh.ui.global::<SettingsState>();
    s.set_section("messages".into());
    s.set_sms_reports(true);
    s.set_smsc("+374 10 590000".into());
    s.set_replies(strings(&["a", "b", "c"]));
    s.set_blocked_count(3);
    s.set_labels(strings(&["Mobile"]));
    sh.header(Header {
        title: "Settings",
        sub: true,
        ..Header::default()
    });
    nav.set_view("".into());
    nav.set_page(4);
    sh.shoot("p2-msg-settings");
    assert_eq!(sh.take_acts()[0].0, "opened");

    // Messages, Blocking, then the caller ID: each row asks the app
    sh.tap(180.0, 118.0); // Delivery reports, on
    sh.tap(180.0, 190.0); // Message centre
    sh.tap(180.0, 254.0); // Quick replies
    sh.tap(180.0, 400.0); // Blocked numbers
    assert_eq!(
        sh.take_acts(),
        [
            ("reports".into(), "0".into()),
            ("form".into(), "smsc".into()),
            ("replies".into(), String::new()),
            ("blocked".into(), String::new()),
        ]
    );
    sh.tap(180.0, 310.0); // New message alert
    assert_eq!(s.get_picker(), "alert");
    sh.tap(180.0, 679.0 - 12.0 - 28.0); // Silent
    assert_eq!(sh.take_acts(), [("alert".into(), "2".into())]);
    sh.settle();
    sh.tap(180.0, 542.0); // Send my caller ID
    assert_eq!(s.get_picker(), "clir");
}
