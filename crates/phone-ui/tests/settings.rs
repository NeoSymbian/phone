// SPDX-License-Identifier: GPL-3.0-only
// Settings (NEOPHONE v2 p2-phone-settings*, p2-msg-settings), Blocked numbers and its form, Quick replies, with the
// boards' sample data: the page, its pickers, the forms, and the page opened from Messages at its MESSAGES section.
mod common;
use common::{Header, model, strings};
use phone_ui::{BlockRow, Nav, SettingsState};
use slint::ComponentHandle;

const ADD: i32 = 2;

fn fill(s: &SettingsState) {
    s.set_clir(0);
    s.set_waiting(2);
    s.set_summary(true);
    s.set_show_duration(true);
    s.set_voicemail("+374 91 000 999".into());
    s.set_blocked_count(3);
    s.set_log_days(30);
    s.set_name_order(0);
    s.set_smart_dial(true);
    s.set_labels(strings(&[
        "Mobile",
        "Mobile (home)",
        "Mobile (work)",
        "Telephone",
    ]));
    s.set_default_label(0);
    s.set_sms_reports(true);
    s.set_smsc("+374 10 590000".into());
    s.set_sms_alert(0);
    s.set_replies(strings(&[
        "Can't talk now. I'll call you later.",
        "I'm in a meeting.",
        "I'm driving. I'll call you back.",
    ]));
    let rule = |title: &str, subtitle: &str, initials: &str, tint, kind| BlockRow {
        title: title.into(),
        subtitle: subtitle.into(),
        initials: initials.into(),
        tint,
        kind,
    };
    s.set_rules(model(vec![
        rule(
            "Անի Պետրոսյան",
            "+374 28 123456 · Calls, messages",
            "ԱՊ",
            1,
            0,
        ),
        rule("+1 555 0199", "Calls", "", -1, 1),
        rule("Starts with +1 900", "Calls, messages · prefix", "", -1, 2),
    ]));
}

#[test]
fn settings() {
    let sh = common::app();
    let nav = sh.ui.global::<Nav>();
    let s = sh.ui.global::<SettingsState>();
    fill(&s);
    nav.set_page(4);
    nav.set_view("".into());
    let main = Header {
        title: "Phone & Contacts",
        sub: true,
        ..Header::default()
    };
    sh.header(main);
    sh.shoot("p2-phone-settings");
    assert_eq!(sh.take_acts()[0].0, "opened");

    s.set_picker("clir".into());
    sh.shoot("p2-phone-settings-clir");
    sh.tap(180.0, 679.0 - 12.0 - 28.0); // Shown
    assert_eq!(sh.take_acts(), [("clir".into(), "2".into())]);
    assert_eq!(s.get_picker(), "");
    s.set_picker("log".into());
    sh.shoot("p2-phone-settings-log");
    s.set_picker("".into());

    // the voicemail number's form
    s.set_form("voicemail".into());
    s.set_draft("+374 91 000 99".into());
    sh.header(Header {
        title: "Voicemail number",
        form: true,
        ..Header::default()
    });
    sh.shoot("p2-phone-settings-voicemail");
    s.set_form("".into());

    // Quick replies
    nav.set_view("replies".into());
    sh.header(Header {
        title: "Quick replies",
        sub: true,
        actions: &[(ADD, "Add")],
        ..Header::default()
    });
    sh.shoot("p2-phone-settings-replies");

    // Blocked numbers, and its Block a number form
    nav.set_view("blocked".into());
    sh.header(Header {
        title: "Blocked numbers",
        sub: true,
        actions: &[(ADD, "Add")],
        options: &["Remove all"],
        ..Header::default()
    });
    sh.shoot("p2-phone-blocked");
    sh.take_acts();
    sh.tap(330.0, 48.0 + 34.0 + 36.0); // Private numbers' switch
    assert_eq!(sh.take_acts(), [("private".into(), "1".into())]);
    s.set_form("block".into());
    s.set_draft("+1 900".into());
    s.set_add_prefix(true);
    sh.header(Header {
        title: "Block a number",
        form: true,
        ..Header::default()
    });
    sh.shoot("p2-phone-blocked-add");
    s.set_form("".into());
}
