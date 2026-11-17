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
}
