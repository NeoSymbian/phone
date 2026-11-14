// SPDX-License-Identifier: GPL-3.0-only
// The frame: the Options sheet and the note over a page (mockups 2, 8, 22 use them).
mod common;
use phone_ui::Nav;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::rc::Rc;

#[test]
fn frame() {
    let s = common::app();
    let nav = s.ui.global::<Nav>();
    nav.set_page(0);
    let items: Vec<SharedString> = ["Recent calls", "Speed dial", "Call voicemail", "Settings"]
        .map(Into::into)
        .into();
    nav.set_options(ModelRc::from(Rc::new(VecModel::from(items))));
    nav.set_options_open(true);
    let px = s.shoot("frame-options");
    assert!(px.iter().any(|&p| p != 0), "something was drawn");
    nav.set_options_open(false);
    nav.set_note("Blocked +1 555 0199".into());
    s.shoot("frame-note");
}
