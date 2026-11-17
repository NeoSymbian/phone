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
    let main = || Header {
        title: "Settings",
        sub: true,
        ..Header::default()
    };
    sh.header(main());
    sh.shoot("p2-phone-settings");
    assert_eq!(sh.take_acts()[0].0, "opened");

    // every row and switch asks the app (rows: Calls at 82 dp, then 72 dp each, Blocked numbers 56)
    sh.tap(180.0, 190.0); // Call waiting, on
    s.set_waiting(0);
    sh.tap(180.0, 190.0); // unknown: a tap checks
    s.set_waiting(1);
    sh.tap(180.0, 190.0); // checking: no tap
    s.set_waiting(4);
    sh.tap(180.0, 190.0); // couldn't check: again
    sh.tap(180.0, 262.0); // Summary after call, on
    sh.tap(180.0, 334.0); // Voicemail number
    sh.tap(180.0, 398.0); // Blocked numbers
    assert_eq!(
        sh.take_acts(),
        [
            ("waiting".into(), "0".into()),
            ("waiting-check".into(), String::new()),
            ("waiting-check".into(), String::new()),
            ("summary".into(), "0".into()),
            ("form".into(), "voicemail".into()),
            ("blocked".into(), String::new()),
        ]
    );
    s.set_waiting(0);
    sh.shoot("p2-phone-settings-waiting");
    s.set_waiting(2);

    s.set_picker("clir".into());
    sh.shoot("p2-phone-settings-clir");
    sh.tap(180.0, 679.0 - 12.0 - 28.0); // Shown
    assert_eq!(sh.take_acts(), [("clir".into(), "2".into())]);
    assert_eq!(s.get_picker(), "");
    sh.tap(180.0, 496.0); // Keep call log
    assert_eq!(s.get_picker(), "log");
    sh.shoot("p2-phone-settings-log");
    sh.tap(180.0, 679.0 - 12.0 - 28.0); // 30 days
    assert_eq!(sh.take_acts(), [("log".into(), "3".into())]);
    assert_eq!(s.get_picker(), "");

    // the voicemail number's form
    s.set_form("voicemail".into());
    s.set_draft("+374 91 000 99".into());
    sh.header(Header {
        title: "Voicemail number",
        form: true,
        ..Header::default()
    });
    sh.shoot("p2-phone-settings-voicemail");
    // each form's ✓ is the view's action: the app saves the draft
    for form in ["voicemail", "smsc", "block", "reply"] {
        s.set_form(form.into());
        sh.tap(360.0 - 24.0, 24.0);
        assert_eq!(
            sh.take_acts(),
            [("header-save".into(), "0".into())],
            "{form}'s ✓"
        );
    }
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
