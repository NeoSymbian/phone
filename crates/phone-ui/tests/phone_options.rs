// SPDX-License-Identifier: GPL-3.0-only
// Phone (mockup 8): the Options sheet over the dialer. Its own binary: the sheet slides in, and the harness's clock
// stands still, so it is drawn open from the first frame.
mod common;
use phone_ui::{DialState, LogRow, Nav};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::rc::Rc;

#[test]
fn phone_options() {
    let s = common::app();
    let nav = s.ui.global::<Nav>();
    let recent = |id: i32, title: &str, kind: i32, when: &str, initials: &str, tint: i32| LogRow {
        id,
        title: title.into(),
        count: 1,
        kind,
        when: when.into(),
        initials: initials.into(),
        tint,
        ..LogRow::default()
    };
    let rows = vec![
        recent(9, "Ava Lindqvist", 2, "21:03", "AL", 0),
        recent(8, "Анна Смирнова", 3, "20:41", "АС", 3),
    ];
    s.ui.global::<DialState>()
        .set_recent(ModelRc::from(Rc::new(VecModel::from(rows))));
    let items: Vec<SharedString> = ["Recent calls", "Speed dial", "Call voicemail", "Settings"]
        .map(Into::into)
        .into();
    nav.set_options(ModelRc::from(Rc::new(VecModel::from(items))));
    nav.set_options_open(true);
    s.shoot("phone-options");
}
