// SPDX-License-Identifier: GPL-3.0-only
//! Messages (NEOPHONE v2 p2-msg-*): the conversations and the archived ones, a thread, a message's details, a new
//! message, the blocked messages; Reply with message from the call screens. MODEMD keeps the messages (modem-ipc
//! 2.4): this module mirrors them ([`phone_core::messages::Mirror`], re-read when `Status::sms_revision` moves),
//! sends, marks read, archives, deletes, and posts one shade entry per conversation with unread messages.
//!
//! The screens keep the composer and the To field in `MessagesState`; the open thread, the search, the selection,
//! the chosen recipients, the held bubble, a swipe's Undo and the queries live here.

use crate::ctx::Ctx;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use e32::dprintln;
use messages_ui::{
    BlockedMessage, Bubble, KeyboardFocus, MessagesApp, MessagesState, Nav, Suggestion, ThreadRow,
};
use modem_ipc::{SmsSettings, sms as st};
use phone_common::ui::{Chrome, model, strings, tool};
use phone_core::messages::{self as messages, Mirror, Swiped, Thread, Undo};
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
    /// selection mode: the checked conversations' numbers (None: off)
    selected: Option<Vec<String>>,
    /// the open thread's number
    pub open: String,
    /// the new message's recipients: (name, number)
    recipients: Vec<(String, String)>,
    suggestions: Vec<(String, &'static str, String)>,
    /// the bubble whose menu is open, the blocked message whose menu is open, the message the details show
    held: Option<u32>,
    held_blocked: Option<u32>,
    details: Option<u32>,
    query: Query,
    /// the last swipe while its toast can take it back
    undo: Undo,
    /// the shade's entries as posted: (id, newest message id)
    posted: Vec<(u32, u32)>,
    /// MODEMD's message settings
    pub settings: SmsSettings,
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

/// Load MODEMD's message settings (start, and Settings).
pub fn load(ctx: &Ctx) {
    let settings = ctx.m.borrow_mut().modem.sms_settings();
    if let Ok(s) = settings {
        ctx.m.borrow_mut().msgs.settings = s;
    }
}

/// The user tapped our shade entry `id` (`action` 0), its Reply (1) or its Call (2).
pub fn notified(ctx: &Ctx, id: u32, action: u16) {
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
        return ctx.go(Page::Messages(MessagesView::List));
    };
    ctx.m.borrow_mut().opened = false;
    match action {
        2 => ctx.call(&number),
        _ => open_thread(ctx, &number, action == 1),
    }
}

/// The toast's Undo: an archive is taken back, a held delete is never sent.
pub fn undo(ctx: &Ctx) {
    let last = ctx.m.borrow_mut().msgs.undo.undo();
    if let Some(Swiped::Archived(id)) = last {
        edit(ctx, st::EDIT_UNARCHIVE_THREAD, id);
    }
    ctx.show();
}

/// The event loop's timer: how long until a held delete is due (60 s when none).
pub fn wait_us(ctx: &Ctx) -> i32 {
    let ms = ctx.m.borrow().msgs.undo.wait_ms(neoapp::wall_ms());
    ms.map_or(60_000_000, |ms| (ms.min(60_000) as i32 + 50) * 1000)
}

/// The timer fired: send a delete whose toast is gone.
pub fn tick(ctx: &Ctx) {
    let due = ctx.m.borrow_mut().msgs.undo.due(neoapp::wall_ms());
    if let Some(id) = due {
        edit(ctx, st::EDIT_DELETE_THREAD, id);
        ctx.show();
    }
}

/// Another action: a held delete is sent now.
fn settle(ctx: &Ctx) {
    let held = ctx.m.borrow_mut().msgs.undo.flush();
    if let Some(id) = held {
        edit(ctx, st::EDIT_DELETE_THREAD, id);
    }
}

/// Open the thread with `number` (from the list, Contacts, Recents, a call, the shade); `write`: the composer takes
/// the focus.
pub fn open_thread(ctx: &Ctx, number: &str, write: bool) {
    {
        let mut m = ctx.m.borrow_mut();
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
    if !matches!(action, "draft" | "to" | "archive" | "delete" | "dismiss") {
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
        ("toggle", Some((number, ..))) => {
            let mut m = ctx.m.borrow_mut();
            if let Some(sel) = m.msgs.selected.as_mut() {
                match sel.iter().position(|n| *n == number) {
                    Some(k) => {
                        sel.remove(k);
                    }
                    None => sel.push(number),
                }
                if sel.is_empty() {
                    m.msgs.selected = None;
                }
            }
        }
        ("archive", Some((_, last, _))) => {
            swiped(ctx, Swiped::Archived(last));
            edit(ctx, st::EDIT_ARCHIVE_THREAD, last);
            ctx.note_undo("Conversation archived");
        }
        ("unarchive", Some((_, last, _))) => {
            edit(ctx, st::EDIT_UNARCHIVE_THREAD, last);
            ctx.note("Conversation unarchived");
        }
        ("delete", Some((number, last, _))) => {
            swiped(ctx, Swiped::Deleted { number, id: last });
            ctx.note_undo("Conversation deleted");
        }
        ("new", _) => return new_message(ctx),
        ("send", _) => return send(ctx),
        ("retry", _) => retry(ctx, id),
        ("hold", _) => ctx.m.borrow_mut().msgs.held = Some(id),
        ("blocked", _) => ctx.m.borrow_mut().msgs.held_blocked = Some(id),
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

/// A swipe: the delete held before it is sent now.
fn swiped(ctx: &Ctx, s: Swiped) {
    let held = ctx.m.borrow_mut().msgs.undo.push(s, neoapp::wall_ms());
    if let Some(id) = held {
        edit(ctx, st::EDIT_DELETE_THREAD, id);
    }
}

/// The held bubble's menu: Retry, Copy text (the screen copied it), Forward, Delete, Details.
fn bubble(ctx: &Ctx, id: u32, item: &str) {
    let text = ctx.m.borrow().msgs.mirror.get(id).map(|m| m.text.clone());
    let Some(text) = text else { return };
    match item {
        "Copy text" => ctx.note("Copied"),
        "Forward" => {
            new_message(ctx);
            ctx.ui()
                .global::<MessagesState>()
                .set_draft(text.as_str().into());
        }
        "Delete" => edit(ctx, st::EDIT_DELETE, id),
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
        ("Unblock sender", Some(n)) => ctx.unblock_number(&n),
        ("Delete", _) => edit(ctx, st::EDIT_DELETE, id),
        _ => {}
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

/// A new message, empty.
fn new_message(ctx: &Ctx) {
    {
        let mut m = ctx.m.borrow_mut();
        m.msgs.recipients.clear();
        m.msgs.suggestions.clear();
        m.msgs.selected = None;
    }
    let ui = ctx.ui();
    let g = ui.global::<MessagesState>();
    g.set_to_text(SharedString::new());
    g.set_draft(SharedString::new());
    ctx.go(Page::Messages(MessagesView::New));
}

/// The contacts matching what is typed in To.
fn suggest(ctx: &Ctx, typed: &str) {
    let mut m = ctx.m.borrow_mut();
    let found = messages::suggestions(&m.book, m.order, typed, 6)
        .into_iter()
        .map(|(_, name, label, number)| (name, label, number))
        .collect();
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
    for n in &to {
        let r = ctx.m.borrow_mut().modem.sms_send(n, &text);
        if r < 0 {
            dprintln!("neomsg: message not queued ({})", r);
            failed += 1;
        }
    }
    if failed == to.len() {
        ctx.note("Can't send to this number");
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
    Bubble {
        id: e.id as i32,
        text: s.mirror.msgs[i].text.as_str().into(),
        meta: meta(e, offset).into(),
        mine: !e.incoming,
        failed: !e.incoming && e.state == st::FAILED,
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
    post_alerts(ctx);
    let m = ctx.m.borrow();
    let s = &m.msgs;
    match view {
        MessagesView::List | MessagesView::Archived => {
            g.set_empty(
                match (s.shown.is_empty(), s.search.is_empty(), view) {
                    (false, ..) => "",
                    (true, false, _) => "No matches",
                    (true, true, MessagesView::Archived) => "No archived conversations",
                    _ => "No messages yet",
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
            let idx = messages::thread(&s.mirror, &s.open);
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
                let (name, _) = messages::who(&m.book, m.order, msg.number());
                let lines = messages::details(msg, &name, now, offset);
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
                    let (pre, hit, post) = messages::split_match(name, &typed);
                    let id = m.book.lookup(number).map_or(0, |c| c.id);
                    Suggestion {
                        pre: pre.into(),
                        hit: hit.into(),
                        post: post.into(),
                        detail: format!("{label} · {number}").into(),
                        initials: initials(name).into(),
                        tint: color(tint(id)),
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
        g.set_menu_text(held.map_or_else(SharedString::new, |h| h.text.as_str().into()));
    }
    g.set_menu_open(!items.is_empty());
}

/// The shade's entries: one per conversation with unread messages, except the thread on screen; read ones go.
fn post_alerts(ctx: &Ctx) {
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
        let n = neoapp::Notification {
            id: a.id,
            title: cut(&a.title, neoui_proto::ipc::TITLE_MAX),
            text: cut(&a.text, neoui_proto::page::TEXT_MAX),
            page: number::messages::THREAD,
            actions: alloc::vec!["Reply".into(), "Call".into()],
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

/// `s` cut to at most `max` bytes, on a character boundary.
fn cut(s: &str, max: usize) -> String {
    let mut end = s.len().min(max);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    String::from(&s[..end])
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
    match m.page {
        Page::Messages(MessagesView::Thread) => {
            let (name, _) = messages::who(&m.book, m.order, &m.msgs.open);
            Chrome::titled(&name).with(tool::CALL, 0, "Call")
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
        _ => match &m.msgs.selected {
            Some(sel) => Chrome {
                selected: sel.len() as i32,
                selection: alloc::vec![
                    (tool::SELECT_ALL, "Select all"),
                    (tool::DELETE, "Delete selected")
                ],
                ..Chrome::titled("Messages")
            },
            // [Messages · 🔍 ✎ ⋮]: the search first, as the board has it (the kit's own 🔍 would come after ✎)
            None => Chrome {
                more: alloc::vec![(tool::EDIT, "New")],
                ..Chrome::titled("Messages").with(tool::SEARCH, 0, "Search")
            },
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
            "Blocked messages",
            "Settings",
        ],
        Page::Messages(MessagesView::Thread) => alloc::vec![
            "Paste",
            if m.book.lookup(&m.msgs.open).is_some() {
                "Contact card"
            } else {
                "Add to contacts"
            },
            "Archive",
            "Delete conversation",
            "Block number",
        ],
        Page::Messages(MessagesView::New) => alloc::vec!["Paste"],
        Page::Messages(MessagesView::Blocked) if !messages::blocked(&m.msgs.mirror).is_empty() => {
            alloc::vec!["Delete all"]
        }
        _ => Vec::new(),
    }
}

pub fn option(ctx: &Ctx, item: &str) {
    settle(ctx);
    let open = ctx.m.borrow().msgs.open.clone();
    match item {
        "New" => return new_message(ctx),
        "Mark all as read" => edit(ctx, st::EDIT_ALL_READ, 0),
        "Select" => ctx.m.borrow_mut().msgs.selected = Some(Vec::new()),
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
        "Delete selected" => ctx.m.borrow_mut().msgs.query = Query::DeleteSelected,
        "Archived" => return ctx.go(Page::Messages(MessagesView::Archived)),
        "Blocked messages" => return ctx.go(Page::Messages(MessagesView::Blocked)),
        // Phone's settings page, at its Messages part
        "Settings" => return ctx.open(Page::Settings(SettingsView::Main), "messages"),
        // the system clipboard's text goes into the composer (the screen reads it)
        "Paste" => {
            let ui = ctx.ui();
            let g = ui.global::<MessagesState>();
            g.set_paste(g.get_paste().wrapping_add(1));
        }
        "Contact card" => return ctx.contact(&open),
        "Add to contacts" => return ctx.open(Page::Contacts(ContactsView::Edit), &open),
        "Archive" => {
            if let Some(id) = newest_of(ctx, &open) {
                swiped(ctx, Swiped::Archived(id));
                edit(ctx, st::EDIT_ARCHIVE_THREAD, id);
                ctx.go(Page::Messages(MessagesView::List));
                ctx.note_undo("Conversation archived");
            }
        }
        "Delete conversation" => ctx.m.borrow_mut().msgs.query = Query::DeleteThread,
        "Block number" => ctx.block_number(&open, true, true),
        "Delete all" => ctx.m.borrow_mut().msgs.query = Query::ClearBlocked,
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

/// Back closes the menu, a query or selection mode first; a new message with something in it asks before it goes.
pub fn back(ctx: &Ctx) -> bool {
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
            || m.msgs.selected.is_some();
        m.msgs.held = None;
        m.msgs.held_blocked = None;
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
        || m.msgs.selected.is_some()
        || draft
}
