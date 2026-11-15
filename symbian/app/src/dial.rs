// SPDX-License-Identifier: GPL-3.0-only
//! The Phone page (mockups 3-8, 27): the dialer (recent calls or smart matches above the keypad, long presses for
//! `+`, voicemail and speed dial, the `* + p w` cycle), Recents (filter chips, day bands, folded repeats, Block /
//! Delete on a held row), call details, speed dial (with a contact picker); and [`place_call`], which every area
//! dials through.

use crate::ctx::{Chrome, Ctx, Model, model, strings};
use crate::{people, prefs};
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use contacts::recents::{duration, when, when_long};
use modem_ipc::{LogEntry, call, log};
use neoui_proto::ipc as proto;
use neoui_proto::settings::keys;
use phone_core::dial::{Dialer, Long, Tones, matches};
use phone_core::page::{Page, PhoneView, SettingsView};
use phone_core::recents::{self, Filter};
use phone_core::speed;
use phone_rules::caller::{caller_line, presentation};
use phone_rules::dial::{Action, Flags, Kind, classify};
use phone_ui::{DetailCall, DialState, LogRow, MatchRow, PhoneApp, PickRow, SpeedTile};
use slint::{ComponentHandle, SharedString};

/// The avatar palette's size (`Tints` in ui/phone/pages.slint).
const TINTS: u32 = 8;
/// How long typed tones wait for their call to connect.
const TONES_WAIT_MS: u64 = 60_000;
/// How often the tones check whether their call connected.
const TONES_POLL_MS: u64 = 250;
const IMEI: &str = "*#06#";

/// A popup list over a view, and what it acts on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sheet {
    /// a held Recents row (log id)
    Row(u32),
    /// an assigned speed-dial key
    Speed(usize),
}

#[derive(Default)]
pub struct State {
    dialer: Dialer,
    /// the numbers of the smart-dial rows shown
    matched: Vec<String>,
    filter: Filter,
    /// the call log entry the details show
    detail: u32,
    sheet: Option<Sheet>,
    /// the speed-dial key the picker assigns, and the numbers of its rows
    pick: Option<usize>,
    picked: Vec<String>,
    /// a number waiting for its speed-dial key (Contacts -> Speed dial)
    pending: Option<String>,
    /// tones typed after a `p`/`w`, waiting for the dialled call to connect (until the deadline), then playing
    tones_after: Option<(String, u64)>,
    tones: Option<Tones>,
    /// the dialled call was seen up: when no call is left, the tones are dropped
    tones_live: bool,
}

pub fn bind(ctx: &Rc<Ctx>, ui: &PhoneApp) {
    let ctx = Rc::downgrade(ctx);
    ui.global::<DialState>().on_act(move |action, value| {
        let Some(ctx) = ctx.upgrade() else { return };
        act(&ctx, &action, &value);
    });
}

fn act(ctx: &Ctx, action: &str, value: &str) {
    let now = e32::time::millis();
    let n = value.parse::<i64>().unwrap_or(0);
    let id = u32::try_from(n).unwrap_or(0);
    match action {
        "key" => {
            ctx.m.borrow_mut().dial.dialer.key(value, now);
            if number(ctx) == IMEI {
                ctx.m.borrow_mut().dial.dialer.clear();
                imei(ctx);
            }
        }
        "key-long" => {
            let long = ctx.m.borrow_mut().dial.dialer.long(value, now);
            match long {
                Long::Voicemail => call_voicemail(ctx),
                Long::Speed(k) => speed_dial(ctx, k),
                Long::Plus | Long::Typed => {}
            }
        }
        "backspace" => ctx.m.borrow_mut().dial.dialer.backspace(),
        "clear" => ctx.m.borrow_mut().dial.dialer.clear(),
        "match" => {
            let n = ctx.m.borrow().dial.matched.get(id as usize).cloned();
            if let Some(n) = n {
                place_call(ctx, &n);
            }
        }
        "add-contact" => {
            let n = number(ctx);
            people::new_with_number(ctx, &n);
            return;
        }
        "recent" | "row-call" => call_entry(ctx, id),
        "filter" => ctx.m.borrow_mut().dial.filter = Filter::from_index(n as i32),
        "row" => {
            ctx.m.borrow_mut().dial.detail = id;
            ctx.go(Page::Phone(PhoneView::Detail));
            return;
        }
        "row-hold" => ctx.m.borrow_mut().dial.sheet = Some(Sheet::Row(id)),
        "detail-call" => {
            let id = ctx.m.borrow().dial.detail;
            call_entry(ctx, id);
        }
        "detail-message" => {
            let n = entry(ctx, ctx.m.borrow().dial.detail).map(|e| String::from(e.number.as_str()));
            if let Some(n) = n.filter(|n| !n.is_empty()) {
                crate::msgs::open_thread(ctx, &n, true);
            }
            return;
        }
        "detail-contact" => {
            let n = entry(ctx, ctx.m.borrow().dial.detail).map(|e| String::from(e.number.as_str()));
            if let Some(n) = n.filter(|n| !n.is_empty()) {
                if people::name_of(ctx, &n).is_some() {
                    if !people::open_for_number(ctx, &n) {
                        ctx.note("Contact not found");
                    }
                } else {
                    people::new_with_number(ctx, &n);
                }
            }
            return;
        }
        "detail-block" => {
            let n = entry(ctx, ctx.m.borrow().dial.detail).map(|e| String::from(e.number.as_str()));
            if let Some(n) = n.filter(|n| !n.is_empty()) {
                toggle_block(ctx, &n);
            }
        }
        "detail-delete" => {
            let id = ctx.m.borrow().dial.detail;
            delete_calls(ctx, id);
            ctx.go(Page::Phone(PhoneView::Recents));
            return;
        }
        "speed" => speed_key(ctx, n as usize),
        "sheet" => sheet_item(ctx, value),
        "sheet-dismiss" => ctx.m.borrow_mut().dial.sheet = None,
        "pick" => {
            let (k, n) = {
                let mut m = ctx.m.borrow_mut();
                let n = m.dial.picked.get(id as usize).cloned();
                (m.dial.pick.take(), n)
            };
            if let (Some(k), Some(n)) = (k, n) {
                assign(ctx, k, &n);
            }
        }
        "pick-dismiss" => ctx.m.borrow_mut().dial.pick = None,
        _ => {}
    }
    ctx.show();
}

fn number(ctx: &Ctx) -> String {
    ctx.m.borrow().dial.dialer.number.clone()
}

fn entry(ctx: &Ctx, id: u32) -> Option<LogEntry> {
    ctx.m.borrow().log.iter().find(|e| e.id == id).copied()
}

/// Call back the number of log entry `id`.
fn call_entry(ctx: &Ctx, id: u32) {
    match entry(ctx, id) {
        Some(e) if !e.number.as_str().is_empty() => place_call(ctx, e.number.as_str()),
        Some(_) => ctx.note("No number to call"),
        None => {}
    }
}

fn imei(ctx: &Ctx) {
    // shortcut: MODEMD's status carries no IMEI yet (modem-ipc 2.2); show it here once it does.
    ctx.note("IMEI not available");
}

fn call_voicemail(ctx: &Ctx) {
    let vm = ctx.m.borrow().prefs.voicemail.clone();
    if vm.is_empty() {
        ctx.go(Page::Settings(SettingsView::Main));
        ctx.note("No voicemail number");
    } else {
        ctx.m.borrow_mut().dial.dialer.clear();
        place_call(ctx, &vm);
    }
}

/// Call speed-dial key `k` (1 = the voice mailbox); an empty key opens Speed dial to assign it.
fn speed_dial(ctx: &Ctx, k: usize) {
    if k == 1 {
        return call_voicemail(ctx);
    }
    let n = speed::number(&ctx.m.borrow().prefs.speed, k).map(String::from);
    match n {
        Some(n) => {
            ctx.m.borrow_mut().dial.dialer.clear();
            place_call(ctx, &n);
        }
        None => {
            ctx.go(Page::Phone(PhoneView::Speed));
            ctx.note(&format!("No number on key {k}. Tap it to assign one"));
        }
    }
}

/// A tile of the speed-dial grid: voicemail, the waiting number's key, an empty key's picker, an assigned key's
/// Change / Remove.
fn speed_key(ctx: &Ctx, k: usize) {
    if k == 1 {
        return call_voicemail(ctx);
    }
    if !(2..=9).contains(&k) {
        return;
    }
    let pending = ctx.m.borrow_mut().dial.pending.take();
    if let Some(n) = pending {
        return assign(ctx, k, &n);
    }
    if speed::number(&ctx.m.borrow().prefs.speed, k).is_some() {
        ctx.m.borrow_mut().dial.sheet = Some(Sheet::Speed(k));
    } else {
        open_picker(ctx, k);
    }
}

/// The contacts' numbers to choose from for key `k`.
fn open_picker(ctx: &Ctx, k: usize) {
    let (rows, numbers): (Vec<PickRow>, Vec<String>) = {
        let m = ctx.m.borrow();
        m.book
            .sorted(m.order)
            .into_iter()
            .filter_map(|id| m.book.get(id))
            .flat_map(|c| {
                let title = c.title(m.order);
                c.numbers.iter().map(move |(l, n)| {
                    let row = PickRow {
                        title: title.as_str().into(),
                        number: format!("{} · {}", l.text(), n).into(),
                    };
                    (row, n.clone())
                })
            })
            .unzip()
    };
    let ui = ctx.ui();
    let d = ui.global::<DialState>();
    d.set_pick(model(rows));
    d.set_pick_heading(format!("Key {k}").into());
    let mut m = ctx.m.borrow_mut();
    m.dial.picked = numbers;
    m.dial.pick = Some(k);
}

fn assign(ctx: &Ctx, k: usize, n: &str) {
    let cells = speed::with(&ctx.m.borrow().prefs.speed, k, n);
    let Some(cells) = cells else { return };
    match prefs::save_speed(ctx, &cells) {
        Ok(()) if n.is_empty() => ctx.note(&format!("Key {k} cleared")),
        Ok(()) => {
            let name = people::name_of(ctx, n).unwrap_or_else(|| n.into());
            ctx.note(&format!("Key {k}: {name}"));
        }
        Err(e) => ctx.note(&format!("Not saved ({e})")),
    }
}

fn sheet_item(ctx: &Ctx, item: &str) {
    let sheet = ctx.m.borrow_mut().dial.sheet.take();
    match (sheet, item) {
        (Some(Sheet::Row(id)), "Block" | "Unblock") => {
            if let Some(e) = entry(ctx, id) {
                toggle_block(ctx, e.number.as_str());
            }
        }
        (Some(Sheet::Row(id)), "Delete") => delete_calls(ctx, id),
        (Some(Sheet::Speed(k)), "Change") => open_picker(ctx, k),
        (Some(Sheet::Speed(k)), "Remove") => assign(ctx, k, ""),
        _ => {}
    }
}

fn toggle_block(ctx: &Ctx, n: &str) {
    if prefs::blocked(ctx, n).is_some() {
        prefs::unblock_number(ctx, n);
    } else {
        prefs::block_number(ctx, n, true, true);
    }
}

/// Delete every call of log entry `id`'s row (the same number).
fn delete_calls(ctx: &Ctx, id: u32) {
    let ids: Vec<u32> = {
        let m = ctx.m.borrow();
        recents::calls_of(&m.log, id)
            .into_iter()
            .map(|i| m.log[i].id)
            .collect()
    };
    let failed = ids
        .into_iter()
        .map(|id| ctx.m.borrow_mut().modem.clear_log(id, 0))
        .find(|r| *r < 0);
    if let Some(r) = failed {
        ctx.note(&format!("Not deleted ({r})"));
    }
}

/// Clear the calls the current filter shows.
fn clear_filter(ctx: &Ctx) {
    let kinds = ctx.m.borrow().dial.filter.kinds();
    let failed = kinds
        .iter()
        .map(|k| ctx.m.borrow_mut().modem.clear_log(0, *k))
        .find(|r| *r < 0);
    if let Some(r) = failed {
        ctx.note(&format!("Not cleared ({r})"));
    }
}

/// A call is up (ringing, dialling or connected).
fn live(m: &Model) -> bool {
    m.status.is_some_and(|s| {
        s.calls
            .as_slice()
            .iter()
            .any(|c| !matches!(c.state, call::NONE | call::END))
    })
}

pub fn show(ctx: &Ctx) {
    let ui = ctx.ui();
    let d = ui.global::<DialState>();
    let view = match ctx.page() {
        Page::Phone(v) => v,
        _ => return,
    };
    let (sheet, pick) = {
        let m = ctx.m.borrow();
        (m.dial.sheet, m.dial.pick.is_some())
    };
    d.set_pick_open(pick);
    show_sheet(ctx, sheet);
    let (utc, offset) = e32::time::utc_now();
    let now = utc * 1000;
    match view {
        PhoneView::Dial => show_dial(ctx, now, offset),
        PhoneView::Recents => show_recents(ctx, now, offset),
        PhoneView::Detail => show_detail(ctx, now, offset),
        PhoneView::Speed => show_speed(ctx),
    }
}

fn show_sheet(ctx: &Ctx, sheet: Option<Sheet>) {
    let ui = ctx.ui();
    let d = ui.global::<DialState>();
    d.set_sheet_open(sheet.is_some());
    let (heading, items): (String, &[&str]) = match sheet {
        None => return,
        Some(Sheet::Speed(k)) => (format!("Key {k}"), &["Change", "Remove"]),
        Some(Sheet::Row(id)) => match entry(ctx, id) {
            Some(e) if !e.number.as_str().is_empty() => {
                let n = e.number.as_str();
                let blocked = prefs::blocked(ctx, n).is_some();
                let title = people::name_of(ctx, n).unwrap_or_else(|| n.into());
                (
                    title,
                    if blocked {
                        &["Unblock", "Delete"]
                    } else {
                        &["Block", "Delete"]
                    },
                )
            }
            _ => (String::new(), &["Delete"]),
        },
    };
    d.set_sheet_heading(heading.into());
    d.set_sheet_items(strings(items));
}

/// The row of log entry `e` folding `count` calls: who, the direction, `line` under it and `when` beside it.
fn log_row(m: &Model, e: &LogEntry, count: u32, section: &str, line: &str, when: &str) -> LogRow {
    let n = e.number.as_str();
    let contact = m.book.lookup(n);
    let name = contact.map(|c| c.title(m.order));
    let (title, _) = caller_line(name.as_deref(), n, e.presentation, false);
    LogRow {
        id: e.id as i32,
        section: section.into(),
        title: title.into(),
        count: count as i32,
        kind: i32::from(e.kind),
        line: line.into(),
        when: when.into(),
        initials: name
            .as_deref()
            .map(recents::initials)
            .unwrap_or_default()
            .into(),
        tint: contact.map_or(-1, |c| recents::tint(Some(c.id), n, TINTS)),
        private: n.is_empty() || e.presentation == presentation::RESTRICTED,
    }
}

fn show_dial(ctx: &Ctx, now: i64, offset: i32) {
    let (number, recent, rows, numbers) = {
        let m = ctx.m.borrow();
        let number = m.dial.dialer.number.clone();
        let recent: Vec<LogRow> = if number.is_empty() {
            recents::recent(&m.log)
                .into_iter()
                .take(30)
                .map(|(i, count)| {
                    let e = &m.log[i];
                    log_row(&m, e, count, "", "", &when(e.utc, now, offset))
                })
                .collect()
        } else {
            Vec::new()
        };
        let found = if m.prefs.smart_dial {
            matches(&m.book, &number, m.order)
        } else {
            Vec::new()
        };
        let numbers = found
            .iter()
            .map(|f| format!("{}{}{}", f.number.pre, f.number.hit, f.number.post))
            .collect();
        let rows: Vec<MatchRow> = found
            .iter()
            .map(|f| {
                let title = format!("{}{}{}", f.name.pre, f.name.hit, f.name.post);
                MatchRow {
                    id: f.id as i32,
                    name_pre: f.name.pre.as_str().into(),
                    name_hit: f.name.hit.as_str().into(),
                    name_post: f.name.post.as_str().into(),
                    label: f.label.text().into(),
                    num_pre: f.number.pre.as_str().into(),
                    num_hit: f.number.hit.as_str().into(),
                    num_post: f.number.post.as_str().into(),
                    initials: recents::initials(&title).into(),
                    tint: recents::tint(Some(f.id), "", TINTS),
                }
            })
            .collect();
        (number, recent, rows, numbers)
    };
    ctx.m.borrow_mut().dial.matched = numbers;
    let ui = ctx.ui();
    let d = ui.global::<DialState>();
    d.set_number(number.into());
    d.set_recent(model(recent));
    d.set_matches(model(rows));
}

fn show_recents(ctx: &Ctx, now: i64, offset: i32) {
    {
        // the Blocked chip goes with the last blocked call
        let mut m = ctx.m.borrow_mut();
        if m.dial.filter == Filter::Blocked && !m.log.iter().any(|e| e.kind == log::BLOCKED) {
            m.dial.filter = Filter::All;
        }
    }
    let (rows, filter, missed, blocked, newest) = {
        let m = ctx.m.borrow();
        let filter = m.dial.filter;
        let rows: Vec<LogRow> = recents::rows(&m.log, filter, now, offset)
            .into_iter()
            .map(|r| {
                let e = &m.log[r.first];
                let known = m.book.lookup(e.number.as_str()).is_some();
                let at = recents::row_when(e.utc, now, offset);
                let word = if e.kind == log::BLOCKED {
                    recents::why_text(e, &m.block, known)
                } else {
                    let label = m
                        .book
                        .lookup(e.number.as_str())
                        .and_then(|c| recents::label_of(c, e.number.as_str()));
                    recents::second_line(e, label)
                };
                let line = if at.is_empty() {
                    word
                } else {
                    format!("{word} · {at}")
                };
                log_row(&m, e, r.count, r.section.text(), &line, "")
            })
            .collect();
        let blocked = m.log.iter().filter(|e| e.kind == log::BLOCKED).count();
        let missed = recents::new_missed(&m.log, m.prefs.missed_seen);
        let newest = recents::newest(&m.log);
        let seen = (m.visible && matches!(filter, Filter::All | Filter::Missed))
            .then_some(newest)
            .filter(|n| *n > m.prefs.missed_seen);
        (rows, filter, missed, blocked, seen)
    };
    // seeing the list marks its missed calls seen (the count shows this once)
    if let Some(id) = newest {
        ctx.m.borrow_mut().prefs.missed_seen = id;
        if let Err(e) = ctx.app.set_setting(keys::PHONE_MISSED_SEEN, id as i32) {
            e32::dprintln!("neophone: missed calls not marked seen ({})", e);
        }
    }
    let ui = ctx.ui();
    let d = ui.global::<DialState>();
    d.set_filter(filter.index());
    d.set_missed_new(missed as i32);
    d.set_has_blocked(blocked > 0);
    d.set_blocked_count(blocked as i32);
    d.set_rows(model(rows));
}

fn show_detail(ctx: &Ctx, now: i64, offset: i32) {
    let Some(e) = entry(ctx, ctx.m.borrow().dial.detail) else {
        return;
    };
    let n = e.number.as_str();
    let blocked = !n.is_empty() && prefs::blocked(ctx, n).is_some();
    let m = ctx.m.borrow();
    let contact = m.book.lookup(n);
    let name = contact.map(|c| c.title(m.order));
    let (title, _) = caller_line(name.as_deref(), n, e.presentation, false);
    let number = match contact.and_then(|c| recents::label_of(c, n)) {
        Some(l) => format!("{n} · {}", l.text()),
        None if title == n => String::new(),
        None => n.into(),
    };
    let calls: Vec<DetailCall> = recents::calls_of(&m.log, e.id)
        .into_iter()
        .map(|i| {
            let c = &m.log[i];
            DetailCall {
                kind: i32::from(c.kind),
                when: when_long(c.utc, now, offset).into(),
                duration: if matches!(c.kind, log::DIALLED | log::RECEIVED) {
                    duration(c.duration_s).into()
                } else {
                    SharedString::new()
                },
            }
        })
        .collect();
    let ui = ctx.ui();
    let d = ui.global::<DialState>();
    d.set_detail_title(title.as_str().into());
    d.set_detail_number(number.into());
    d.set_detail_initials(
        name.as_deref()
            .map(recents::initials)
            .unwrap_or_default()
            .into(),
    );
    d.set_detail_tint(contact.map_or(-1, |c| recents::tint(Some(c.id), n, TINTS)));
    d.set_detail_private(n.is_empty() || e.presentation == presentation::RESTRICTED);
    d.set_detail_known(contact.is_some());
    d.set_detail_blocked(blocked);
    d.set_detail_can_call(!n.is_empty());
    d.set_detail_calls(model(calls));
}

fn show_speed(ctx: &Ctx) {
    let tiles: Vec<SpeedTile> = {
        let m = ctx.m.borrow();
        (1..=9)
            .map(|k| {
                let n = speed::number(&m.prefs.speed, k).unwrap_or("");
                let contact = (!n.is_empty()).then(|| m.book.lookup(n)).flatten();
                SpeedTile {
                    key: k as i32,
                    name: if n.is_empty() {
                        SharedString::new()
                    } else {
                        speed::short_name(&m.book, m.order, n).into()
                    },
                    initials: contact
                        .map(|c| recents::initials(&c.title(m.order)))
                        .unwrap_or_default()
                        .into(),
                    tint: contact.map_or(-1, |c| recents::tint(Some(c.id), n, TINTS)),
                }
            })
            .collect()
    };
    ctx.ui().global::<DialState>().set_speed(model(tiles));
}

pub fn chrome(ctx: &Ctx) -> Chrome {
    match ctx.page() {
        Page::Phone(PhoneView::Recents) => {
            Chrome::titled("Recent calls").with(proto::TOOL_DELETE, 0, "Clear")
        }
        Page::Phone(PhoneView::Detail) => Chrome::titled("Call details"),
        Page::Phone(PhoneView::Speed) => Chrome::titled("Speed dial"),
        _ => Chrome::titled("Phone").with(proto::TOOL_CALL, proto::TOOLBAR_PILL, "Call"),
    }
}

pub fn options(ctx: &Ctx) -> Vec<&'static str> {
    match ctx.page() {
        Page::Phone(PhoneView::Recents) => alloc::vec!["Clear list", "Settings"],
        Page::Phone(PhoneView::Detail) => alloc::vec!["Delete from log"],
        Page::Phone(PhoneView::Speed) => alloc::vec!["Settings"],
        _ => alloc::vec!["Recent calls", "Speed dial", "Call voicemail", "Settings"],
    }
}

pub fn option(ctx: &Ctx, item: &str) {
    match item {
        "Recent calls" => ctx.go(Page::Phone(PhoneView::Recents)),
        "Speed dial" => ctx.go(Page::Phone(PhoneView::Speed)),
        "Call voicemail" => call_voicemail(ctx),
        "Settings" => ctx.go(Page::Settings(SettingsView::Main)),
        "Clear list" => {
            clear_filter(ctx);
            ctx.show();
        }
        "Delete from log" => act(ctx, "detail-delete", ""),
        _ => {}
    }
}

/// The toolbar's middle button: Call in the dialer (an empty number opens Recents, as on the N8; one key 2..9
/// speed-dials), Clear in Recents.
pub fn action(ctx: &Ctx) {
    match ctx.page() {
        Page::Phone(PhoneView::Dial) => {
            let (n, key) = {
                let m = ctx.m.borrow();
                (m.dial.dialer.number.clone(), m.dial.dialer.single_key())
            };
            if n.is_empty() {
                ctx.go(Page::Phone(PhoneView::Recents));
            } else if let Some(k) = key {
                speed_dial(ctx, k);
            } else {
                place_call(ctx, &n);
            }
        }
        Page::Phone(PhoneView::Recents) => {
            clear_filter(ctx);
            ctx.show();
        }
        _ => {}
    }
}

/// Back closes a sheet or the picker first; leaving Speed dial drops a number waiting for a key.
pub fn back(ctx: &Ctx) -> bool {
    let closed = {
        let mut m = ctx.m.borrow_mut();
        let d = &mut m.dial;
        if d.sheet.is_some() || d.pick.is_some() {
            d.sheet = None;
            d.pick = None;
            true
        } else {
            d.pending = None;
            false
        }
    };
    if closed {
        ctx.show();
    }
    closed
}

pub fn wants_back(ctx: &Ctx) -> bool {
    let m = ctx.m.borrow();
    m.dial.sheet.is_some() || m.dial.pick.is_some()
}

/// The tones after a dialled number: start them when the call connects, send each when due, drop them when the call
/// is gone.
pub fn tick(ctx: &Ctx) {
    let now = e32::time::millis();
    let key = {
        let mut m = ctx.m.borrow_mut();
        let up = live(&m);
        let connected = m.status.is_some_and(|s| {
            s.calls
                .as_slice()
                .iter()
                .any(|c| c.state == call::CONVERSATION && !c.incoming)
        });
        let d = &mut m.dial;
        if d.tones_after.is_none() && d.tones.is_none() {
            return;
        }
        d.tones_live |= up;
        let expired = d
            .tones_after
            .as_ref()
            .is_some_and(|(_, until)| now >= *until);
        if (d.tones_live && !up) || expired {
            d.tones_after = None;
            d.tones = None;
            d.tones_live = false;
            return;
        }
        if connected && let Some((rest, _)) = d.tones_after.take() {
            d.tones = Some(Tones::new(&rest, now));
        }
        let key = d.tones.as_mut().and_then(|t| t.step(now));
        if d.tones.as_ref().is_some_and(Tones::done) {
            d.tones = None;
            d.tones_live = false;
        }
        key
    };
    if let Some(k) = key {
        let r = ctx.m.borrow_mut().modem.dtmf(k as u8);
        if r < 0 {
            e32::dprintln!("neophone: tone not sent ({})", r);
        }
    }
}

/// When the dialer needs the next tick (ms, `e32::time::millis`): the next tone, or a look whether the dialled call
/// connected.
pub fn next_wake(ctx: &Ctx) -> Option<u64> {
    let m = ctx.m.borrow();
    if let Some(t) = &m.dial.tones {
        return Some(t.at());
    }
    m.dial
        .tones_after
        .as_ref()
        .map(|_| e32::time::millis() + TONES_POLL_MS)
}

/// Call `number` from anywhere in the app (a contact's number, a Recents row, speed dial): dial strings and service
/// codes go through `phone_rules::dial::classify`, a number is dialled (MODEMD adds the caller-ID setting's prefix)
/// and the call screens open; tones after a `p`/`w` play once the call connects.
pub fn place_call(ctx: &Ctx, typed: &str) {
    if typed == IMEI {
        return imei(ctx);
    }
    let in_call = live(&ctx.m.borrow());
    let kind = classify(
        typed,
        Flags {
            send: true,
            in_call,
        },
    );
    let note = match kind {
        Kind::Number { clir, number, dtmf } => {
            // a typed `*31#`/`#31#` overrides the setting for this call
            let wire = format!("{}{}", clir.map_or("", |c| c.prefix()), number);
            let r = ctx.m.borrow_mut().modem.dial(&wire);
            if r == 0 {
                {
                    let mut m = ctx.m.borrow_mut();
                    m.dial.dialer.clear();
                    m.dial.tones = None;
                    m.dial.tones_live = false;
                    m.dial.tones_after =
                        (!dtmf.is_empty()).then(|| (dtmf, e32::time::millis() + TONES_WAIT_MS));
                }
                ctx.go(Page::Call);
                return;
            }
            format!("Not dialled ({r})")
        }
        Kind::Emergency(_) => "Emergency calls are not supported on this phone yet".into(),
        Kind::Imei => return imei(ctx),
        Kind::Ss { action, code, .. } if code == "43" => {
            let set = match action {
                Action::Activate | Action::Register => Some(true),
                Action::Deactivate | Action::Erase => Some(false),
                Action::Interrogate => None,
            };
            match ctx.m.borrow_mut().modem.waiting(set) {
                0 => "Call waiting: asking the network…".into(),
                r => format!("Call waiting: not available ({r})"),
            }
        }
        Kind::InCall { cmd: 2, call: None } => {
            ctx.m.borrow_mut().modem.swap();
            return;
        }
        Kind::InCall { cmd: 3, call: None } => {
            ctx.m.borrow_mut().modem.merge();
            return;
        }
        Kind::None => return,
        _ => "Not supported yet".into(),
    };
    ctx.m.borrow_mut().dial.dialer.clear();
    ctx.note(&note);
    ctx.show();
}

/// Contacts -> "Speed dial" on a card: Speed dial opens with `number` waiting for a key.
pub fn start_assign(ctx: &Ctx, number: &str) {
    let name = people::name_of(ctx, number).unwrap_or_else(|| number.into());
    ctx.m.borrow_mut().dial.pending = Some(number.into());
    ctx.go(Page::Phone(PhoneView::Speed));
    ctx.note(&format!("Tap a key for {name}"));
}
