// SPDX-License-Identifier: GPL-3.0-only
// ⋮ ▸ Call voicemail (or holding 1) with no number set: the app moves from Recents to Settings and opens the voicemail
// form in one step; the frame's page change must not take the form's keyboard away. Its own binary: one window.
mod common;
use common::Header;
use phone_ui::{KeyboardFocus, Nav, SettingsState};
use slint::ComponentHandle;

#[test]
fn voicemail_form_keeps_its_keyboard() {
    let s = common::app();
    let nav = s.ui.global::<Nav>();
    nav.set_page(0);
    nav.set_view("".into());
    s.draw();
    assert!(!s.ui.global::<KeyboardFocus>().get_open());
    // what dial.rs call_voicemail does in one act: Settings, then the form
    nav.set_page(4);
    s.ui.global::<SettingsState>().set_form("voicemail".into());
    s.header(Header {
        title: "Voicemail number",
        form: true,
        ..Header::default()
    });
    s.draw();
    s.draw();
    assert!(
        s.ui.global::<KeyboardFocus>().get_open(),
        "the form's field keeps the focus and asks for the keyboard"
    );
    // (the harness does run the frame's page-change handler: a later move closes the keyboard)
    nav.set_view("blocked".into());
    s.draw();
    assert!(!s.ui.global::<KeyboardFocus>().get_open());
}
