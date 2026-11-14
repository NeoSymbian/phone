// SPDX-License-Identifier: GPL-3.0-only
// Phone: a Recents row held (Block / Delete). Its own binary: the sheet slides in, and the harness's clock stands
// still, so it is drawn open from the first frame.
mod common;
use phone_ui::{DialState, LogRow, Nav};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::rc::Rc;

#[test]
fn phone_hold() {
    let s = common::app();
    let d = s.ui.global::<DialState>();
    s.ui.global::<Nav>().set_view("recents".into());
    let row = |id: i32, title: &str, kind: i32, line: &str, initials: &str, tint: i32| LogRow {
        id,
        section: "Today".into(),
        title: title.into(),
        count: 1,
        kind,
        line: line.into(),
        initials: initials.into(),
        tint,
        ..LogRow::default()
    };
    let rows = vec![
        row(9, "Ava Lindqvist", 2, "Mobile · 21:03", "AL", 0),
        row(8, "+1 555 0123", 1, "Dialled · 09:15", "", -1),
    ];
    d.set_rows(ModelRc::from(Rc::new(VecModel::from(rows))));
    let items: Vec<SharedString> = ["Block", "Delete"].map(Into::into).into();
    d.set_sheet_heading("+1 555 0123".into());
    d.set_sheet_items(ModelRc::from(Rc::new(VecModel::from(items))));
    d.set_sheet_open(true);
    s.shoot("phone-recents-hold");
}
