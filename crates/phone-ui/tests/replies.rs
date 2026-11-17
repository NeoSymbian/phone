// SPDX-License-Identifier: GPL-3.0-only
// Reply with message over an incoming call (mockup 33) and Settings ▸ Quick replies: Phone's side of Messages.
mod common;
use phone_ui::{CallState, Nav, Party, SettingsState};
use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};
use std::rc::Rc;

fn strings(v: &[&str]) -> ModelRc<SharedString> {
    ModelRc::from(Rc::new(VecModel::from(
        v.iter().map(|s| SharedString::from(*s)).collect::<Vec<_>>(),
    )))
}

fn tint(rgb: u32) -> Color {
    Color::from_argb_encoded(0xff00_0000 | rgb)
}

#[test]
fn replies() {
    let s = common::app();
    let nav = s.ui.global::<Nav>();
    // Reply with message over the incoming call (mockup 33)
    nav.set_page(3);
    let c = s.ui.global::<CallState>();
    c.set_screen("incoming".into());
    c.set_main(Party {
        name: "Ava Lindqvist".into(),
        line: "+1 555 0142".into(),
        label: "Mobile".into(),
        initials: "AL".into(),
        tint: tint(0x22b46a),
    });
    c.set_can_message(true);
    s.shoot("call-incoming-message");
    c.set_replies(strings(&[
        "Can't talk now. I'll call you later.",
        "I'm in a meeting.",
        "I'm driving. I'll call you back.",
        "Write your own…",
    ]));
    c.set_replying(true);
    s.shoot("call-reply");
    c.set_replying(false);

    // Settings ▸ Quick replies
    nav.set_page(4);
    nav.set_view("replies".into());
    let st = s.ui.global::<SettingsState>();
    st.set_replies(strings(&[
        "Can't talk now. I'll call you later.",
        "I'm in a meeting.",
        "I'm driving. I'll call you back.",
    ]));
    s.shoot("settings-replies");
}
