// SPDX-License-Identifier: GPL-3.0-only
//! Messages (NEOPHONE v2 p2-msg-*): the conversations and the archived ones, a thread, a message's details, a new
//! message, the blocked messages; Reply with message from the call screens. MODEMD keeps the messages (modem-ipc
//! 2.5): this module mirrors them ([`phone_core::messages::Mirror`], re-read when `Status::sms_revision` moves),
//! sends, marks read, archives, deletes, and posts one shade entry per conversation with unread messages.
//!
//! The screens keep the composer and the To field in `MessagesState`; the open thread, the search, the selection,
//! the chosen recipients, the held bubble, a delete's Undo and the queries live here.

use crate::ctx::Ctx;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use e32::dprintln;
use messages_ui::{
    BlockedMessage, Bubble, KeyboardFocus, MessagesApp, MessagesState, Nav, Suggestion, ThreadRow,
};
use modem_ipc::sms as st;
use neoapp::card;
use phone_common::ui::{Chrome, model, strings, tool};
use phone_core::messages::{self as messages, Mirror, Swiped, Thread, Undo, cut};
use phone_core::page::{ContactsView, MessagesView, Page, SettingsView, number};
use phone_core::people::{initials, tint};
use slint::{Color, ComponentHandle, SharedString};

/// What a query over the page will do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Query {
    #[default]
    None,
    DeleteSelected,
    DeleteThread,
    DeleteAll,
    ClearBlocked,
    Discard,
}

#[derive(Default)]
pub struct State {
    pub mirror: Mirror,
    /// the conversations (archived ones too) as last shown, and which of them the list shows
    threads: Vec<Thread>,
    shown: Vec<usize>,
    /// the header's search text
    search: String,
    /// selection mode: the checked conversations' numbers (None: off; empty: ⋮ ▸ Select, nothing ticked yet)
    selected: Option<Vec<String>>,
    /// the open thread's number
    pub open: String,
    /// the new message's recipients: (name, number)
    recipients: Vec<(String, String)>,
    /// To's rows: (name, label, number); "Send to" a typed number has no label
    suggestions: Vec<(String, &'static str, String)>,
    /// the bubble whose menu is open, the blocked message whose menu is open, the message the details show
    held: Option<u32>,
    held_blocked: Option<u32>,
    details: Option<u32>,
    query: Query,
    /// Block number's query open: its Calls and Messages boxes
    block_ask: Option<[bool; 2]>,
    /// the last delete or archive while its toast can take it back
    undo: Undo,
    /// the toast's button is Settings (a prefix still blocks an unblocked sender), not Undo
    note_settings: bool,
    /// the shade's entries as posted: (id, newest message id)
    posted: Vec<(u32, u32)>,
}

pub fn bind(ctx: &Rc<Ctx>, ui: &MessagesApp) {
    let ctx = Rc::downgrade(ctx);
    ui.global::<MessagesState>().on_act(move |action, value| {
        let Some(ctx) = ctx.upgrade() else { return };
        act(&ctx, &action, &value);
    });
}

/// Read what changed in MODEMD's messages since the mirror's revision.
pub fn sync(ctx: &Ctx) {
    for _ in 0..64 {
        let since = ctx.m.borrow().msgs.mirror.rev;
        let r = ctx.m.borrow_mut().modem.sms_read(since);
        match r {
            Ok(b) => {
                if ctx.m.borrow_mut().msgs.mirror.apply(&b) == 0 {
                    return;
                }
            }
            // deletions since then are forgotten: read everything again
            Err(-1) if since != 0 => ctx.m.borrow_mut().msgs.mirror.reset(),
            Err(e) => {
                dprintln!("neomsg: messages not read ({})", e);
                return;
            }
        }
    }
}

/// MODEMD's status: re-read the messages when they changed.
pub fn on_status(ctx: &Ctx) {
    let changed = {
        let m = ctx.m.borrow();
        m.status
            .is_some_and(|s| s.sms_revision != m.msgs.mirror.rev)
    };
    if changed {
        sync(ctx);
    }
}

/// The user tapped our card `id` (`action` 0) or one of its actions (`messages::alert_action`: Reply, Mark read,
/// Call). Mark read comes in place: the app stays where it is (maybe hidden).
pub fn notified(ctx: &Ctx, id: u32, action: u16) {
    use messages::alert_action as act;
    settle(ctx);
    let number = ctx
        .m
        .borrow()
        .msgs
        .threads
        .iter()
        .find(|t| messages::alert_id(&t.number) == id)
        .map(|t| t.number.clone());
    let Some(number) = number else {
        if action != act::MARK_READ {
            ctx.go(Page::Messages(MessagesView::List));
        }
        return;
    };
    if action == act::MARK_READ {
        if let Some(newest) = newest_of(ctx, &number) {
            edit(ctx, st::EDIT_THREAD_READ, newest);
        }
        // the card and the badge go
        return ctx.show();
    }
    ctx.m.borrow_mut().opened = false;
    match action {
        act::CALL => ctx.call(&number),
        _ => open_thread(ctx, &number, action == act::REPLY),
    }
}

/// The toast's button: Undo (an archive is taken back, a held delete is never sent), or Settings.
pub fn undo(ctx: &Ctx) {
    if core::mem::take(&mut ctx.m.borrow_mut().msgs.note_settings) {
        return ctx.open(Page::Settings(SettingsView::Main), "blocked");
    }
    let last = ctx.m.borrow_mut().msgs.undo.undo();
    if let Some(Swiped::Archived(id)) = last {
        edit(ctx, st::EDIT_UNARCHIVE_THREAD, id);
    }
    ctx.show();
}

/// A toast with Undo.
fn note_undo(ctx: &Ctx, text: &str) {
    ctx.m.borrow_mut().msgs.note_settings = false;
    ctx.note_undo(text);
}

/// The event loop's timer: how long until a held delete is due (60 s when none).
pub fn wait_us(ctx: &Ctx) -> i32 {
    let ms = ctx.m.borrow().msgs.undo.wait_ms(neoapp::wall_ms());
    ms.map_or(60_000_000, |ms| (ms.min(60_000) as i32 + 50) * 1000)
}

/// The timer fired: send a delete whose toast is gone.
pub fn tick(ctx: &Ctx) {
    let due = ctx.m.borrow_mut().msgs.undo.due(neoapp::wall_ms());
    if let Some((action, id)) = due {
        edit(ctx, action, id);
        ctx.show();
    }
}

/// Another action: a held delete is sent now.
fn settle(ctx: &Ctx) {
    let held = ctx.m.borrow_mut().msgs.undo.flush();
    if let Some((action, id)) = held {
        edit(ctx, action, id);
    }
}

/// Open the thread with `number` (from the list, Contacts, Recents, a call, the shade); `write`: the composer takes
/// the focus.
pub fn open_thread(ctx: &Ctx, number: &str, write: bool) {
    {
        let mut m = ctx.m.borrow_mut();
        // a thread opened here: Back goes to the list (lib.rs sets it again when another app opened it)
        m.opened = false;
        m.msgs.open = String::from(number.trim());
        m.msgs.held = None;
        m.msgs.details = None;
        m.msgs.selected = None;
    }
    let ui = ctx.ui();
    let g = ui.global::<MessagesState>();
    g.set_draft(SharedString::new());
    g.set_autofocus(write);
    ctx.go(Page::Messages(MessagesView::Thread));
}

/// The header's search changed ("" when it closed).
pub fn searched(ctx: &Ctx, query: &str) {
    ctx.m.borrow_mut().msgs.search = String::from(query);
    ctx.show();
}

fn color(rgb: u32) -> Color {
    Color::from_rgb_u8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

fn act(ctx: &Ctx, action: &str, value: &str) {
    let ui = ctx.ui();
    let g = ui.global::<MessagesState>();
    let id: u32 = value.parse().unwrap_or(0);
    let i: usize = value.parse().unwrap_or(usize::MAX);
    // the list's row i: its number, its newest message and whether that failed (before a delete moves the rows)
    let row = {
        let m = ctx.m.borrow();
        m.msgs
            .shown
            .get(i)
            .and_then(|&t| m.msgs.threads.get(t))
            .and_then(|t| {
                let e = &m.msgs.mirror.msgs.get(t.last)?.entry;
                Some((t.number.clone(), e.id, e.state == st::FAILED))
            })
    };
    if !matches!(
        action,
        "draft" | "to" | "archive" | "delete" | "dismiss" | "block-check"
    ) {
        settle(ctx);
    }
    match (action, row) {
        ("open", Some((number, last, failed))) => {
            // a failed newest message: tap to retry
            if failed {
                retry(ctx, last);
            }
            return open_thread(ctx, &number, false);
        }
        ("select", Some((number, ..))) => {
            ctx.m.borrow_mut().msgs.selected = Some(alloc::vec![number]);
        }
        // selection mode stays on with nothing ticked (✕ ends it)
        ("toggle", Some((number, ..))) => {
            let mut m = ctx.m.borrow_mut();
            if let Some(sel) = m.msgs.selected.as_mut() {
                match sel.iter().position(|n| *n == number) {
                    Some(k) => {
                        sel.remove(k);
                    }
                    None => sel.push(number),
                }
            }
        }
        ("archive", Some((_, last, _))) => {
            swiped(ctx, Swiped::Archived(last));
            edit(ctx, st::EDIT_ARCHIVE_THREAD, last);
            note_undo(ctx, "Conversation archived");
        }
        ("unarchive", Some((_, last, _))) => {
            edit(ctx, st::EDIT_UNARCHIVE_THREAD, last);
            ctx.note("Conversation unarchived");
        }
        ("delete", Some((number, last, _))) => {
            swiped(ctx, Swiped::Deleted { number, id: last });
            note_undo(ctx, "Conversation deleted");
        }
        ("new", _) => return new_message(ctx, ""),
        ("send", _) => return send(ctx),
        ("retry", _) => retry(ctx, id),
        ("hold", _) => ctx.m.borrow_mut().msgs.held = Some(id),
        // a tap on a blocked message: the whole of it, in its details
        ("blocked", _) => {
            ctx.m.borrow_mut().msgs.details = Some(id);
            return ctx.go(Page::Messages(MessagesView::Details));
        }
        ("blocked-hold", _) => ctx.m.borrow_mut().msgs.held_blocked = Some(id),
        ("block-check", _) => {
            if let Some(on) = ctx
                .m
                .borrow_mut()
                .msgs
                .block_ask
                .as_mut()
                .and_then(|b| b.get_mut(i))
            {
                *on = !*on;
            }
        }
        ("block-cancel", _) => ctx.m.borrow_mut().msgs.block_ask = None,
        ("block-confirm", _) => {
            let (ask, open) = {
                let mut m = ctx.m.borrow_mut();
                (m.msgs.block_ask.take(), m.msgs.open.clone())
            };
            if let Some([calls, messages]) = ask
                && (calls || messages)
            {
                ctx.block_number(&open, calls, messages);
            }
        }
        ("menu", _) => {
            let (held, blocked) = {
                let mut m = ctx.m.borrow_mut();
                (m.msgs.held.take(), m.msgs.held_blocked.take())
            };
            g.set_menu_open(false);
            if let Some(id) = held {
                return bubble(ctx, id, value);
            }
            if let Some(id) = blocked {
                blocked_item(ctx, id, value);
            }
        }
        ("dismiss", _) => {
            let mut m = ctx.m.borrow_mut();
            m.msgs.held = None;
            m.msgs.held_blocked = None;
        }
        ("to", _) => suggest(ctx, value),
        ("pick", _) => {
            let s = ctx.m.borrow().msgs.suggestions.get(i).cloned();
            if let Some((name, _, number)) = s {
                ctx.m.borrow_mut().msgs.recipients.push((name, number));
                g.set_to_text(SharedString::new());
                suggest(ctx, "");
            }
        }
        ("unpick", _) => {
            let mut m = ctx.m.borrow_mut();
            if i < m.msgs.recipients.len() {
                m.msgs.recipients.remove(i);
            }
        }
        _ => {}
    }
    ctx.show();
}

/// A swipe or a bubble's Delete: the delete held before it is sent now.
fn swiped(ctx: &Ctx, s: Swiped) {
    let held = ctx.m.borrow_mut().msgs.undo.push(s, neoapp::wall_ms());
    if let Some((action, id)) = held {
        edit(ctx, action, id);
    }
}

/// The held bubble's menu: Retry, Copy text, Forward, Delete (with Undo), Details.
fn bubble(ctx: &Ctx, id: u32, item: &str) {
    let text = ctx.m.borrow().msgs.mirror.get(id).map(|m| m.text.clone());
    let Some(text) = text else { return };
    match item {
        "Copy text" => ctx.note(if ctx.app.set_clipboard(&text) {
            "Copied"
        } else {
            "Not copied"
        }),
        "Forward" => {
            new_message(ctx, "");
            ctx.ui()
                .global::<MessagesState>()
                .set_draft(text.as_str().into());
        }
        "Delete" => {
            swiped(ctx, Swiped::Message(id));
            note_undo(ctx, "Message deleted");
        }
        "Details" => {
            ctx.m.borrow_mut().msgs.details = Some(id);
            return ctx.go(Page::Messages(MessagesView::Details));
        }
        "Retry" => retry(ctx, id),
        _ => {}
    }
    ctx.show();
}

/// The held blocked message's menu: Unblock sender, Delete.
fn blocked_item(ctx: &Ctx, id: u32, item: &str) {
    let number = ctx
        .m
        .borrow()
        .msgs
        .mirror
        .get(id)
        .map(|m| String::from(m.number()));
    match (item, number) {
        ("Unblock sender", Some(n)) => unblock_sender(ctx, id, &n),
        ("Delete", _) => edit(ctx, st::EDIT_DELETE, id),
        _ => {}
    }
}

/// Unblock sender: its number rules go, and its blocked messages come back to its conversation. A prefix rule that
/// still blocks it is only said, with the way to Settings ▸ Blocked numbers.
fn unblock_sender(ctx: &Ctx, id: u32, number: &str) {
    let r = {
        let mut m = ctx.m.borrow_mut();
        let m = &mut *m;
        phone_common::block::unblock_number(&mut m.modem, &mut m.block, number)
    };
    if let Err(e) = r {
        return ctx.note(&e);
    }
    edit(ctx, st::EDIT_UNBLOCK_SENDER, id);
    let still = ctx.m.borrow().block.check_message(number);
    let text = match still {
        None => return ctx.note("Unblocked"),
        Some(phone_rules::block::why::PREFIX) => "A blocked prefix still blocks this number",
        Some(_) => "Another rule still blocks this number",
    };
    ctx.m.borrow_mut().msgs.note_settings = true;
    let text = phone_common::ui::note_text(text);
    if let Err(e) = ctx.app.note(&text, "Settings") {
        dprintln!("neomsg: note not shown ({})", e);
    }
}

fn retry(ctx: &Ctx, id: u32) {
    edit(ctx, st::EDIT_RETRY, id);
}

fn edit(ctx: &Ctx, action: i32, id: u32) {
    let r = ctx.m.borrow_mut().modem.sms_edit(action, id);
    if r < 0 {
        ctx.note(&format!("Not done ({r})"));
    }
    sync(ctx);
}

/// The newest message of the thread with `number`.
fn newest_of(ctx: &Ctx, number: &str) -> Option<u32> {
    let m = ctx.m.borrow();
    messages::thread(&m.msgs.mirror, number)
        .last()
        .map(|&i| m.msgs.mirror.msgs[i].entry.id)
}

/// A new message, empty or with `draft` in the composer (Contacts' Send as vCard ▸ Message).
pub fn new_message(ctx: &Ctx, draft: &str) {
    {
        let mut m = ctx.m.borrow_mut();
        m.msgs.recipients.clear();
        m.msgs.suggestions.clear();
        m.msgs.selected = None;
    }
    let ui = ctx.ui();
    let g = ui.global::<MessagesState>();
    g.set_to_text(SharedString::new());
    g.set_draft(draft.into());
    ctx.go(Page::Messages(MessagesView::New));
}

/// The contacts matching what is typed in To; a number none of them has: "Send to" it.
fn suggest(ctx: &Ctx, typed: &str) {
    let mut m = ctx.m.borrow_mut();
    let mut found: Vec<_> = messages::suggestions(&m.book, m.order, typed, 6)
        .into_iter()
        .map(|(_, name, label, number)| (name, label, number))
        .collect();
    if found.is_empty()
        && let Some(n) = messages::as_number(typed)
    {
        found.push((n.clone(), "", n));
    }
    m.msgs.suggestions = found;
}

/// Send the composer's text: to the open thread, or to each recipient of a new message (plus a number typed in To).
fn send(ctx: &Ctx) {
    let ui = ctx.ui();
    let g = ui.global::<MessagesState>();
    let text = String::from(g.get_draft().as_str());
    if !messages::sendable(&text) {
        return;
    }
    let new = ctx.page() == Page::Messages(MessagesView::New);
    let to: Vec<String> = if new {
        let mut v: Vec<String> = ctx
            .m
            .borrow()
            .msgs
            .recipients
            .iter()
            .map(|(_, n)| n.clone())
            .collect();
        if let Some(n) = messages::as_number(&g.get_to_text()) {
            v.push(n);
        }
        v
    } else {
        alloc::vec![ctx.m.borrow().msgs.open.clone()]
    };
    if to.is_empty() {
        ctx.note("Add a recipient");
        return;
    }
    let mut failed = 0;
    let mut error = 0;
    for n in &to {
        let r = ctx.m.borrow_mut().modem.sms_send(n, &text);
        if r < 0 {
            dprintln!("neomsg: message not queued ({})", r);
            failed += 1;
            error = r;
        }
    }
    if failed == to.len() {
        ctx.note(messages::send_error(error));
        return;
    }
    g.set_draft(SharedString::new());
    sync(ctx);
    if !new {
        return ctx.show();
    }
    {
        let mut m = ctx.m.borrow_mut();
        m.msgs.recipients.clear();
        m.msgs.suggestions.clear();
    }
    g.set_to_text(SharedString::new());
    if to.len() == 1 {
        open_thread(ctx, &to[0], false);
    } else {
        ctx.go(Page::Messages(MessagesView::List));
        ctx.note(&format!("Sent to {} people", to.len()));
    }
}

/// A bubble's meta: its time, then ✓ sent, ✓✓ delivered, Sending.
fn meta(e: &modem_ipc::SmsEntry, offset: i32) -> String {
    let time = messages::time(e.utc, offset);
    match e.state {
        _ if e.incoming => time,
        st::SENDING => format!("{time} · Sending"),
        st::SENT => format!("{time} ✓"),
        st::DELIVERED => format!("{time} ✓✓"),
        _ => time,
    }
}

fn bubble_of(s: &State, i: usize, band: String, offset: i32) -> Bubble {
    let e = &s.mirror.msgs[i].entry;
    let failed = !e.incoming && e.state == st::FAILED;
    Bubble {
        id: e.id as i32,
        text: s.mirror.msgs[i].text.as_str().into(),
        meta: if failed {
            messages::failed_meta(e.why)
        } else {
            meta(e, offset)
        }
        .into(),
        mine: !e.incoming,
        failed,
        band: band.into(),
    }
}

pub fn show(ctx: &Ctx) {
    let Page::Messages(view) = ctx.page() else {
        return;
    };
    let ui = ctx.ui();
    let g = ui.global::<MessagesState>();
    let (utc, offset) = e32::time::utc_now();
    let now = utc * 1000;
    // the open thread is read once it shows
    if view == MessagesView::Thread && ctx.m.borrow().visible {
        let unread = {
            let m = ctx.m.borrow();
            messages::thread(&m.msgs.mirror, &m.msgs.open)
                .into_iter()
                .rev()
                .find(|&i| m.msgs.mirror.msgs[i].entry.state == st::UNREAD)
                .map(|i| m.msgs.mirror.msgs[i].entry.id)
        };
        if let Some(id) = unread {
            edit(ctx, st::EDIT_THREAD_READ, id);
        }
    }
    {
        let mut m = ctx.m.borrow_mut();
        let threads = messages::threads(&m.msgs.mirror, &m.book, m.order);
        let s = &m.msgs;
        let shown = messages::listed(
            &s.mirror,
            &threads,
            &s.search,
            view == MessagesView::Archived,
            s.undo.hidden(),
        );
        m.msgs.shown = shown;
        m.msgs.threads = threads;
    }
    post_alerts(ctx, offset);
    let m = ctx.m.borrow();
    let s = &m.msgs;
    match view {
        MessagesView::List | MessagesView::Archived => {
            let searching = !s.search.trim().is_empty();
            g.set_empty(
                match (s.shown.is_empty(), searching, view) {
                    (false, ..) => "",
                    (true, true, _) => "No matches",
                    (true, false, MessagesView::Archived) => "No archived conversations",
                    _ => "No messages yet",
                }
                .into(),
            );
            g.set_result(
                match s.shown.len() {
                    _ if !searching => String::new(),
                    0 => String::new(),
                    1 => String::from("1 conversation"),
                    n => format!("{n} conversations"),
                }
                .into(),
            );
            g.set_selecting(s.selected.is_some());
            let rows: Vec<ThreadRow> = s
                .shown
                .iter()
                .map(|&i| {
                    let t = &s.threads[i];
                    ThreadRow {
                        name: t.name.as_str().into(),
                        initials: t
                            .contact
                            .map_or_else(String::new, |_| initials(&t.name))
                            .into(),
                        tint: color(t.contact.map_or(0, tint)),
                        known: t.contact.is_some(),
                        snippet: t.snippet.as_str().into(),
                        when: contacts::recents::when(t.utc, now, offset).into(),
                        unread: t.unread as i32,
                        failed: t.failed,
                        checked: s.selected.as_ref().is_some_and(|v| v.contains(&t.number)),
                    }
                })
                .collect();
            g.set_threads(model(rows));
        }
        MessagesView::Thread => {
            // a deleted bubble is hidden while its toast can bring it back
            let hidden = s.undo.hidden_message();
            let idx: Vec<usize> = messages::thread(&s.mirror, &s.open)
                .into_iter()
                .filter(|&i| Some(s.mirror.msgs[i].entry.id) != hidden)
                .collect();
            let bands = messages::bands(&s.mirror, &idx, now, offset);
            let bubbles: Vec<Bubble> = idx
                .iter()
                .zip(bands)
                .map(|(&i, band)| bubble_of(s, i, band, offset))
                .collect();
            g.set_bubbles(model(bubbles));
        }
        MessagesView::Details => {
            let msg = s.details.and_then(|id| {
                let at = s.mirror.msgs.iter().position(|m| m.entry.id == id)?;
                Some((at, &s.mirror.msgs[at]))
            });
            if let Some((at, msg)) = msg {
                let (name, contact) = messages::who(&m.book, m.order, msg.number());
                let mut lines = messages::details(msg, &name, now, offset);
                if msg.entry.state == st::BLOCKED {
                    let why = messages::why_text(msg.entry.why, contact.is_some());
                    lines.push(("Blocked", String::from(why)));
                }
                let keys: Vec<&str> = lines.iter().map(|(k, _)| *k).collect();
                let values: Vec<&str> = lines.iter().map(|(_, v)| v.as_str()).collect();
                g.set_details(bubble_of(s, at, String::new(), offset));
                g.set_details_keys(strings(&keys));
                g.set_details_values(strings(&values));
            }
        }
        MessagesView::New => {
            let names: Vec<&str> = s.recipients.iter().map(|(n, _)| n.as_str()).collect();
            g.set_recipients(strings(&names));
            let typed = g.get_to_text();
            let sugg: Vec<Suggestion> = s
                .suggestions
                .iter()
                .map(|(name, label, number)| {
                    if label.is_empty() {
                        return Suggestion {
                            pre: format!("Send to {number}").into(),
                            ..Suggestion::default()
                        };
                    }
                    let (pre, hit, post) = messages::split_match(name, &typed);
                    let id = m.book.lookup(number).map_or(0, |c| c.id);
                    Suggestion {
                        pre: pre.into(),
                        hit: hit.into(),
                        post: post.into(),
                        detail: format!("{label} · {number}").into(),
                        initials: initials(name).into(),
                        tint: color(tint(id)),
                        known: true,
                    }
                })
                .collect();
            g.set_suggestions(model(sugg));
        }
        MessagesView::Blocked => {
            let idx = messages::blocked(&s.mirror);
            let mut last_band = "";
            let rows: Vec<BlockedMessage> = idx
                .iter()
                .map(|&i| {
                    let msg = &s.mirror.msgs[i];
                    let e = &msg.entry;
                    let (name, contact) = messages::who(&m.book, m.order, msg.number());
                    let band = match phone_core::recents::Section::of(e.utc, now, offset) {
                        phone_core::recents::Section::Today => "Today",
                        phone_core::recents::Section::Yesterday => "Yesterday",
                        _ => "Earlier",
                    };
                    let show_band = band != last_band;
                    last_band = band;
                    BlockedMessage {
                        id: e.id as i32,
                        band: if show_band { band } else { "" }.into(),
                        title: name.as_str().into(),
                        snippet: msg.text.as_str().into(),
                        why: messages::why_text(e.why, contact.is_some()).into(),
                        when: contacts::recents::when(e.utc, now, offset).into(),
                        initials: initials(&name).into(),
                        tint: color(contact.map_or(0, tint)),
                        known: contact.is_some(),
                    }
                })
                .collect();
            g.set_blocked(model(rows));
        }
    }
    // the composer's counter and Send
    let draft = g.get_draft();
    g.set_counter(messages::counter(&draft).into());
    let has_to = view != MessagesView::New
        || !s.recipients.is_empty()
        || messages::as_number(&g.get_to_text()).is_some();
    g.set_can_send(messages::sendable(&draft) && has_to);
    // the menu at the finger: a held bubble's (Retry first when it failed), a held blocked message's
    let held = s.held.and_then(|id| s.mirror.get(id));
    let items: &[&str] = match held {
        Some(h) if h.entry.state == st::FAILED => {
            &["Retry", "Copy text", "Forward", "Delete", "Details"]
        }
        Some(_) => &["Copy text", "Forward", "Delete", "Details"],
        None if s.held_blocked.is_some() => &["Unblock sender", "Delete"],
        None => &[],
    };
    if !items.is_empty() {
        g.set_menu_items(strings(items));
        g.set_menu_destructive(
            items
                .iter()
                .position(|i| *i == "Delete")
                .map_or(-1, |p| p as i32),
        );
    }
    g.set_menu_open(!items.is_empty());
    g.set_held(held.map_or(-1, |h| h.entry.id as i32));
    // Block number's query
    g.set_block_open(s.block_ask.is_some());
    if let Some([calls, msgs]) = s.block_ask {
        let (name, _) = messages::who(&m.book, m.order, &s.open);
        g.set_block_question(format!("Block {name}?").into());
        g.set_block_calls(calls);
        g.set_block_messages(msgs);
    }
}

/// The shade's entries: one per conversation with unread messages, except the thread on screen; read ones go. Each has
/// the contact's avatar, the newest message's time, the unread message before it and the newest, Reply and (for a
/// number) Call.
fn post_alerts(ctx: &Ctx, offset: i32) {
    let (want, posted) = {
        let m = ctx.m.borrow();
        let on_screen = m.visible && m.page == Page::Messages(MessagesView::Thread);
        let open = on_screen.then_some(m.msgs.open.as_str());
        (
            messages::alerts(&m.msgs.mirror, &m.msgs.threads, open),
            m.msgs.posted.clone(),
        )
    };
    let (post, cancel) = messages::alert_changes(&posted, &want);
    if post.is_empty() && cancel.is_empty() {
        return;
    }
    for a in post {
        let actions = messages::alert_actions(&a.number);
        let n = neoapp::Notification {
            id: a.id,
            title: cut(&a.title, neoui_proto::ipc::TITLE_MAX).into(),
            text: a.lines(neoui_proto::page::TEXT_MAX),
            page: number::messages::THREAD,
            actions: actions.iter().map(|a| String::from(*a)).collect(),
            initials: a.contact.map_or_else(String::new, |_| {
                cut(&initials(&a.title), neoui_proto::page::INITIALS_MAX).into()
            }),
            tint: a.contact.map_or(0, tint),
            time: messages::time(a.utc, offset),
            progress: None,
            flags: 0,
            style: card::style::CONVERSATION,
            channel: 0,
            group: messages::ALERT_GROUP.into(),
            when: a.utc.max(0) as u64,
            // Reply (drawn as the main one; TP4 makes it inline), Mark read where the user is, Call
            action_kinds: alloc::vec![
                card::action::REPLY,
                card::action::IN_PLACE,
                card::action::BUTTON
            ][..actions.len()]
                .into(),
        };
        if let Err(e) = ctx.app.notify(&n) {
            dprintln!("neomsg: entry not posted ({})", e);
        }
    }
    for id in cancel {
        if let Err(e) = ctx.app.cancel_notify(id) {
            dprintln!("neomsg: entry not removed ({})", e);
        }
    }
    ctx.m.borrow_mut().msgs.posted = want.iter().map(|a| (a.id, a.newest)).collect();
}

/// The question open, asked with the UI server's dialog.
pub fn query(ctx: &Ctx) -> Option<neoapp::Query> {
    let m = ctx.m.borrow();
    let n = m.msgs.selected.as_ref().map_or(0, Vec::len);
    let (q, detail, yes) = match m.msgs.query {
        Query::None => return None,
        Query::DeleteSelected if n == 1 => ("Delete this conversation?", String::new(), "Delete"),
        Query::DeleteSelected => (
            "Delete these conversations?",
            format!("{n} conversations are deleted from the phone."),
            "Delete",
        ),
        Query::DeleteThread => ("Delete this conversation?", String::new(), "Delete"),
        Query::DeleteAll => (
            "Delete all conversations?",
            String::from("Every message is deleted from the phone."),
            "Delete",
        ),
        Query::ClearBlocked => ("Delete all blocked messages?", String::new(), "Delete"),
        Query::Discard => ("Discard this message?", String::new(), "Discard"),
    };
    Some(phone_common::ui::ask(q, &detail, yes))
}

pub fn answer(ctx: &Ctx, yes: bool) {
    let (q, open) = {
        let mut m = ctx.m.borrow_mut();
        (core::mem::take(&mut m.msgs.query), m.msgs.open.clone())
    };
    if !yes {
        return ctx.show();
    }
    settle(ctx);
    match q {
        Query::DeleteSelected => {
            let numbers = ctx.m.borrow_mut().msgs.selected.take().unwrap_or_default();
            for n in &numbers {
                if let Some(id) = newest_of(ctx, n) {
                    edit(ctx, st::EDIT_DELETE_THREAD, id);
                }
            }
            ctx.note(&if numbers.len() == 1 {
                String::from("Conversation deleted")
            } else {
                format!("{} conversations deleted", numbers.len())
            });
        }
        Query::DeleteThread => {
            if let Some(id) = newest_of(ctx, &open) {
                edit(ctx, st::EDIT_DELETE_THREAD, id);
            }
            ctx.go(Page::Messages(MessagesView::List));
            ctx.note("Conversation deleted");
        }
        Query::DeleteAll => {
            edit(ctx, st::EDIT_DELETE_ALL, 0);
            ctx.note("Conversations deleted");
        }
        Query::ClearBlocked => edit(ctx, st::EDIT_DELETE_BLOCKED, 0),
        Query::Discard => {
            ctx.ui()
                .global::<MessagesState>()
                .set_draft(SharedString::new());
            ctx.m.borrow_mut().msgs.recipients.clear();
            ctx.go(Page::Messages(MessagesView::List));
        }
        Query::None => {}
    }
    ctx.show();
}

pub fn chrome(ctx: &Ctx) -> Chrome {
    let m = ctx.m.borrow();
    let list = matches!(
        m.page,
        Page::Messages(MessagesView::List | MessagesView::Archived)
    );
    if let (true, Some(sel)) = (list, &m.msgs.selected) {
        // [✕ N selected · select all, delete]; 0 after ⋮ ▸ Select
        return Chrome {
            selected: sel.len() as i32,
            selecting: true,
            selection: alloc::vec![
                (tool::SELECT_ALL, "Select all"),
                (tool::DELETE, "Delete selected")
            ],
            ..Chrome::titled("Messages")
        };
    }
    match m.page {
        Page::Messages(MessagesView::Thread) => {
            let (name, _) = messages::who(&m.book, m.order, &m.msgs.open);
            let c = Chrome::titled(&name);
            // an alphanumeric sender (a bank, the operator) can't be called
            if messages::as_number(&m.msgs.open).is_some() {
                c.with(tool::CALL, 0, "Call")
            } else {
                c
            }
        }
        Page::Messages(MessagesView::Details) => Chrome::titled("Message details"),
        Page::Messages(MessagesView::New) => {
            Chrome::titled("New message").with(tool::NONE, tool::CANCEL, "")
        }
        Page::Messages(MessagesView::Archived) => Chrome::titled("Archived"),
        Page::Messages(MessagesView::Blocked) => {
            let c = Chrome::titled("Blocked messages");
            if messages::blocked(&m.msgs.mirror).is_empty() {
                c
            } else {
                c.with(tool::DELETE, 0, "Clear")
            }
        }
        // [Messages · 🔍 ✎ ⋮]: the search first, as the board has it (the kit's own 🔍 would come after ✎)
        _ => Chrome {
            more: alloc::vec![(tool::EDIT, "New")],
            ..Chrome::titled("Messages").with(tool::SEARCH, 0, "Search")
        },
    }
}

pub fn options(ctx: &Ctx) -> Vec<&'static str> {
    let m = ctx.m.borrow();
    match m.page {
        Page::Messages(MessagesView::List) if m.msgs.selected.is_none() => alloc::vec![
            "Mark all as read",
            "Select",
            "Archived",
            "Delete conversations",
            "Blocked messages",
            "Settings",
        ],
        Page::Messages(MessagesView::Thread) => {
            let mut v = Vec::new();
            // the clipboard's text into the composer, when there is some
            if ctx.app.clipboard().is_some() {
                v.push("Paste");
            }
            v.push(if m.book.lookup(&m.msgs.open).is_some() {
                "Contact card"
            } else {
                "Add to contacts"
            });
            v.push("Archive");
            v.push("Delete conversation");
            v.push(
                if phone_common::block::blocked(&m.block, &m.msgs.open).is_some() {
                    "Unblock number"
                } else {
                    "Block number"
                },
            );
            v
        }
        _ => Vec::new(),
    }
}

pub fn option(ctx: &Ctx, item: &str) {
    settle(ctx);
    let open = ctx.m.borrow().msgs.open.clone();
    match item {
        "New" => return new_message(ctx, ""),
        "Mark all as read" => edit(ctx, st::EDIT_ALL_READ, 0),
        "Select" => ctx.m.borrow_mut().msgs.selected = Some(Vec::new()),
        "Delete conversations" => ctx.m.borrow_mut().msgs.query = Query::DeleteAll,
        "Select all" => {
            let mut m = ctx.m.borrow_mut();
            let all = m
                .msgs
                .shown
                .iter()
                .map(|&i| m.msgs.threads[i].number.clone())
                .collect();
            m.msgs.selected = Some(all);
        }
        "Delete selected" => {
            let mut m = ctx.m.borrow_mut();
            if m.msgs.selected.as_ref().is_some_and(|s| !s.is_empty()) {
                m.msgs.query = Query::DeleteSelected;
            }
        }
        "Archived" => return ctx.go(Page::Messages(MessagesView::Archived)),
        "Blocked messages" => return ctx.go(Page::Messages(MessagesView::Blocked)),
        // Phone's settings page, at its Messages part
        "Settings" => return ctx.open(Page::Settings(SettingsView::Main), "messages"),
        "Paste" => {
            if let Some(clip) = ctx.app.clipboard() {
                let ui = ctx.ui();
                let g = ui.global::<MessagesState>();
                g.set_draft(format!("{}{clip}", g.get_draft()).into());
            }
        }
        "Contact card" => return ctx.contact(&open),
        "Add to contacts" => return ctx.open(Page::Contacts(ContactsView::Edit), &open),
        "Archive" => {
            if let Some(id) = newest_of(ctx, &open) {
                swiped(ctx, Swiped::Archived(id));
                edit(ctx, st::EDIT_ARCHIVE_THREAD, id);
                ctx.go(Page::Messages(MessagesView::List));
                note_undo(ctx, "Conversation archived");
            }
        }
        "Delete conversation" => ctx.m.borrow_mut().msgs.query = Query::DeleteThread,
        "Block number" => ctx.m.borrow_mut().msgs.block_ask = Some([true, true]),
        "Unblock number" => ctx.unblock_number(&open),
        _ => {}
    }
    ctx.show();
}

/// The header's action: the list's search, the thread's Call, the blocked messages' Clear.
pub fn action(ctx: &Ctx) {
    settle(ctx);
    match ctx.page() {
        Page::Messages(MessagesView::List) => ctx.ui().global::<Nav>().set_searching(true),
        Page::Messages(MessagesView::Thread) => {
            let open = ctx.m.borrow().msgs.open.clone();
            ctx.call(&open);
        }
        Page::Messages(MessagesView::Blocked) => {
            ctx.m.borrow_mut().msgs.query = Query::ClearBlocked;
            ctx.show();
        }
        _ => {}
    }
}

/// Back closes the menu, a query or selection mode first; a new message with something in it asks before it goes; a
/// blocked message's details go back to the blocked messages.
pub fn back(ctx: &Ctx) -> bool {
    let blocked_details = {
        let m = ctx.m.borrow();
        m.page == Page::Messages(MessagesView::Details)
            && m.msgs
                .details
                .and_then(|id| m.msgs.mirror.get(id))
                .is_some_and(|msg| msg.entry.state == st::BLOCKED)
    };
    if blocked_details && !wants_back(ctx) {
        ctx.go(Page::Messages(MessagesView::Blocked));
        return true;
    }
    if !wants_back(ctx) {
        // a conversation another app opened (a contact's Message, a call's Reply): back to that app
        let opened = ctx.m.borrow().opened;
        if opened && ctx.page() == Page::Messages(MessagesView::Thread) {
            ctx.back_to(Page::Contacts(ContactsView::List));
            return true;
        }
        return false;
    }
    {
        let mut m = ctx.m.borrow_mut();
        let open = m.msgs.held.is_some()
            || m.msgs.held_blocked.is_some()
            || m.msgs.query != Query::None
            || m.msgs.block_ask.is_some()
            || m.msgs.selected.is_some();
        m.msgs.held = None;
        m.msgs.held_blocked = None;
        m.msgs.block_ask = None;
        m.msgs.selected = None;
        m.msgs.query = if open { Query::None } else { Query::Discard };
    }
    ctx.ui().global::<KeyboardFocus>().set_open(false);
    ctx.show();
    true
}

pub fn wants_back(ctx: &Ctx) -> bool {
    let m = ctx.m.borrow();
    let draft = m.page == Page::Messages(MessagesView::New)
        && (!m.msgs.recipients.is_empty()
            || !ctx.ui().global::<MessagesState>().get_draft().is_empty());
    m.msgs.held.is_some()
        || m.msgs.held_blocked.is_some()
        || m.msgs.query != Query::None
        || m.msgs.block_ask.is_some()
        || m.msgs.selected.is_some()
        || draft
}
