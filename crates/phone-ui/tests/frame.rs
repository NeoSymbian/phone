// SPDX-License-Identifier: GPL-3.0-only
// The frame: the note over a page (the Options sheet is the UI server's since neoui protocol 1.7).
mod common;
use phone_ui::Nav;
use slint::ComponentHandle;

#[test]
fn frame() {
    let s = common::app();
    let nav = s.ui.global::<Nav>();
    nav.set_page(0);
    nav.set_note("Blocked +1 555 0199".into());
    let px = s.shoot("frame-note");
    assert!(px.iter().any(|&p| p != 0), "something was drawn");
}
