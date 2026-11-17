// SPDX-License-Identifier: GPL-3.0-only
// The frame (ui-shared/frame.slint): the kit header the app draws (protocol 1.8): a root page with its action, ⋮ and
// the footer that opens Contacts and Messages, a form, the dialer's Call band.
mod common;
use phone_ui::Nav;
use slint::ComponentHandle;

#[test]
fn frame() {
    let s = common::app();
    let nav = s.ui.global::<Nav>();
    nav.set_page(4);
    nav.set_title("Settings".into());
    nav.set_footer(true);
    nav.set_missed(2);
    nav.set_unread(3);
    nav.set_has_action(true);
    nav.set_action_tool(3); // person_add
    nav.set_action_label("New".into());
    let items: Vec<slint::SharedString> = vec!["Mark several".into(), "Import vCards".into()];
    nav.set_options(slint::ModelRc::from(std::rc::Rc::new(
        slint::VecModel::from(items),
    )));
    let px = s.shoot("frame-header");
    assert!(px.iter().any(|&p| p != 0), "something was drawn");
    nav.set_form(true);
    nav.set_title("New contact".into());
    s.shoot("frame-form");
    nav.set_form(false);
    // the dialer: Call in a band of its own under the keypad (D14: big actions stay in the content)
    nav.set_page(0);
    nav.set_title("Phone".into());
    nav.set_options(slint::ModelRc::default());
    nav.set_action_tool(1);
    nav.set_action_label("Call".into());
    nav.set_pill(true);
    s.shoot("frame-dialer-call");
    nav.set_pill(false);
}
