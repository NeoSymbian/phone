// SPDX-License-Identifier: GPL-3.0-only
// Settings: Phone & Contacts (mockup 23) with its pickers and the voicemail editor, Blocked numbers (mockup 25) with
// the add row (Remove all is the UI server's dialog).
mod common;
use phone_ui::{BlockRow, Nav, SettingsState};
use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};
use std::rc::Rc;

fn model<T: Clone + 'static>(v: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(v)))
}

fn row(title: &str, subtitle: &str, initials: &str, tint: u32, kind: i32) -> BlockRow {
    BlockRow {
        title: title.into(),
        subtitle: subtitle.into(),
        initials: initials.into(),
        tint: Color::from_argb_encoded(0xff00_0000 | tint),
        kind,
    }
}

#[test]
fn settings() {
    let s = common::app();
    let nav = s.ui.global::<Nav>();
    let st = s.ui.global::<SettingsState>();
    nav.set_page(4);
    nav.set_view("".into());
    let labels: Vec<SharedString> = [
        "Mobile",
        "Mobile (home)",
        "Mobile (work)",
        "Telephone",
        "Telephone (home)",
        "Telephone (work)",
        "Fax",
    ]
    .map(Into::into)
    .into();
    st.set_labels(model(labels));
    st.set_clir(0);
    st.set_waiting(2);
    st.set_summary(true);
    st.set_show_duration(true);
    st.set_voicemail("+374 91 000 999".into());
    st.set_blocked_count(3);
    st.set_log_days(30);
    st.set_name_order(0);
    st.set_smart_dial(true);
    st.set_default_label(0);
    let px = s.shoot("settings-main");
    assert!(px.iter().any(|&p| p != 0), "something was drawn");

    // the host clock stands still, so a sheet's fade never runs: each picker shows on a freshly opened page
    let reopen = || {
        nav.set_page(0);
        s.shoot("settings-tmp");
        let tmp = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../snapshots/settings-tmp.png"
        );
        std::fs::remove_file(tmp).unwrap();
        nav.set_page(4);
    };
    for (picker, name) in [("clir", "settings-clir"), ("log", "settings-log")] {
        reopen();
        st.set_picker(picker.into());
        s.shoot(name);
        st.set_picker("".into());
    }
    reopen();

    st.set_waiting(1);
    st.set_editing(true);
    st.set_draft("+374 91 000 99".into());
    s.shoot("settings-voicemail");
    st.set_editing(false);

    nav.set_view("blocked".into());
    st.set_rules(model(vec![
        row(
            "Անի Պետրոսյան",
            "+374 28 123456 · Calls, messages",
            "ԱՊ",
            0xd6409f,
            0,
        ),
        row("+1 555 0199", "Calls", "", 0, 1),
        row("Starts with +1 900", "Calls, messages · prefix", "", 0, 2),
    ]));
    s.shoot("settings-blocked");

    st.set_adding(true);
    st.set_add_text("+1 900".into());
    st.set_add_prefix(true);
    s.shoot("settings-add");
    st.set_adding(false);
}
