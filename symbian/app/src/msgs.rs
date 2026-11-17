// SPDX-License-Identifier: GPL-3.0-only
//! Messages (mockups 28-36): the conversations, a thread, a new message, the blocked messages; Reply with message
//! from the call screens. MODEMD keeps the messages (modem-ipc 2.3): this module mirrors them
//! ([`phone_core::messages::Mirror`], re-read when `Status::sms_revision` moves), sends, marks read, deletes.
//!
//! The screens keep the search text, the composer and the To field in `MessagesState`; the open thread, the chosen
//! recipients, the held bubble and the queries live here.

use crate::ctx::{Chrome, Ctx, model, strings, tool};
use crate::{people, prefs, store};
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use e32::dprintln;
use modem_ipc::{SmsSettings, sms as st};
use phone_core::messages::{self as messages, Mirror, Thread};
use phone_core::page::{MessagesView, Page, SettingsView};
use phone_core::people::{initials, tint};
use phone_ui::{
    BlockedMessage, Bubble, KeyboardFocus, MessagesState, PhoneApp, Suggestion, ThreadRow,
};
use slint::{Color, ComponentHandle, SharedString};

/// What a query over the page will do.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Query {
    #[default]
    None,
    DeleteAll,
    DeleteThread,
    ClearBlocked,
}

#[derive(Default)]
pub struct State {
    pub mirror: Mirror,
    /// the conversations as last shown, and which of them the search shows
    threads: Vec<Thread>,
    shown: Vec<usize>,
    /// the open thread's number
    pub open: String,
    /// the new message's recipients: (name, number)
    recipients: Vec<(String, String)>,
    suggestions: Vec<(String, String)>,
    /// the bubble whose sheet is open, the blocked message whose sheet is open, the message whose details show
    held: Option<u32>,
    held_blocked: Option<u32>,
    details: Option<u32>,
    query: Query,
    /// Copy text keeps it here (the system has no clipboard yet); Paste puts it in the composer
    clipboard: String,
    /// Reply with message's texts
    pub replies: Vec<String>,
    /// MODEMD's message settings
    pub settings: SmsSettings,
}

pub fn bind(ctx: &Rc<Ctx>, ui: &PhoneApp) {
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
                dprintln!("neophone: messages not read ({})", e);
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

/// Load the quick replies and MODEMD's message settings (start, and Settings).
pub fn load(ctx: &Ctx) {
    let replies = store::replies();
    let settings = ctx.m.borrow_mut().modem.sms_settings();
    let mut m = ctx.m.borrow_mut();
    m.msgs.replies = replies;
    if let Ok(s) = settings {
        m.msgs.settings = s;
    }
}

/// Open the thread with `number` (from Contacts, Recents, a call); `write`: the composer takes the focus.
pub fn open_thread(ctx: &Ctx, number: &str, write: bool) {
    {
        let mut m = ctx.m.borrow_mut();
        m.msgs.open = String::from(number.trim());
        m.msgs.held = None;
        m.msgs.details = None;
    }
    let ui = ctx.ui();
    let g = ui.global::<MessagesState>();
    g.set_draft(SharedString::new());
    g.set_autofocus(write);
    ctx.go(Page::Messages(MessagesView::Thread));
}

fn color(rgb: u32) -> Color {
    Color::from_rgb_u8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

fn act(ctx: &Ctx, action: &str, value: &str) {
    let ui = ctx.ui();
    let g = ui.global::<MessagesState>();
    let id: u32 = value.parse().unwrap_or(0);
    let i: usize = value.parse().unwrap_or(usize::MAX);
    match action {
        "search" => {}
        "open" => {
            let number = {
                let m = ctx.m.borrow();
                m.msgs
                    .shown
                    .get(i)
                    .and_then(|&t| m.msgs.threads.get(t))
                    .map(|t| (t.number.clone(), t.failed, t.last))
            };
            let Some((number, failed, last)) = number else {
                return;
            };
            // a failed newest message: tap to retry (mockup 28)
            if failed {
                let id = ctx.m.borrow().msgs.mirror.msgs[last].entry.id;
                retry(ctx, id);
            }
            open_thread(ctx, &number, false);
            return;
        }
        "draft" => {}
        "send" => {
            send(ctx);
            return;
        }
        "hold" => ctx.m.borrow_mut().msgs.held = Some(id),
        "tap" => {
            let failed = ctx
                .m
                .borrow()
                .msgs
                .mirror
                .get(id)
                .is_some_and(|m| m.entry.state == st::FAILED);
            if failed {
                ctx.m.borrow_mut().msgs.held = Some(id);
            }
        }
        "bubble" => {
            let held = ctx.m.borrow_mut().msgs.held.take();
            if let Some(id) = held {
                bubble(ctx, id, value);
            }
        }
        "dismiss" => {
            let mut m = ctx.m.borrow_mut();
            m.msgs.held = None;
            m.msgs.held_blocked = None;
        }
        "details-close" => ctx.m.borrow_mut().msgs.details = None,
        "to" => suggest(ctx, value),
        "pick" => {
            let s = ctx.m.borrow().msgs.suggestions.get(i).cloned();
            if let Some((name, number)) = s {
                ctx.m.borrow_mut().msgs.recipients.push((name, number));
                g.set_to_text(SharedString::new());
                suggest(ctx, "");
            }
        }
        "unpick" => {
            let mut m = ctx.m.borrow_mut();
            if i < m.msgs.recipients.len() {
                m.msgs.recipients.remove(i);
            }
        }
        "confirm" => {
            let (q, open) = {
                let mut m = ctx.m.borrow_mut();
                (::core::mem::take(&mut m.msgs.query), m.msgs.open.clone())
            };
            match q {
                Query::DeleteAll => {
                    let ids: Vec<u32> = {
                        let m = ctx.m.borrow();
                        m.msgs
                            .threads
                            .iter()
                            .map(|t| m.msgs.mirror.msgs[t.last].entry.id)
                            .collect()
                    };
                    for id in ids {
                        edit(ctx, st::EDIT_DELETE_THREAD, id);
                    }
                    ctx.note("Deleted");
                }
                Query::DeleteThread => {
                    if let Some(id) = newest_of(ctx, &open) {
                        edit(ctx, st::EDIT_DELETE_THREAD, id);
                    }
                    ctx.go(Page::Messages(MessagesView::List));
                    ctx.note("Deleted");
                }
                Query::ClearBlocked => edit(ctx, st::EDIT_DELETE_BLOCKED, 0),
                Query::None => {}
            }
        }
        "cancel" => ctx.m.borrow_mut().msgs.query = Query::None,
        "blocked" => ctx.m.borrow_mut().msgs.held_blocked = Some(id),
        "blocked-item" => {
            let held = ctx.m.borrow_mut().msgs.held_blocked.take();
            let number = held.and_then(|id| {
                ctx.m
                    .borrow()
                    .msgs
                    .mirror
                    .get(id)
                    .map(|m| String::from(m.number()))
            });
            match (value, held, number) {
                ("Unblock sender", _, Some(n)) => prefs::unblock_number(ctx, &n),
                ("Delete", Some(id), _) => edit(ctx, st::EDIT_DELETE, id),
                _ => {}
            }
        }
        _ => {}
    }
    ctx.show();
}

/// The held bubble's sheet: Copy text, Forward, Delete, Details, Retry.
fn bubble(ctx: &Ctx, id: u32, item: &str) {
    let text = ctx.m.borrow().msgs.mirror.get(id).map(|m| m.text.clone());
    let Some(text) = text else { return };
    match item {
        "Copy text" => {
            ctx.m.borrow_mut().msgs.clipboard = text;
            ctx.note("Copied");
        }
        "Forward" => {
            ctx.m.borrow_mut().msgs.recipients.clear();
            let ui = ctx.ui();
            let g = ui.global::<MessagesState>();
            g.set_to_text(SharedString::new());
            g.set_draft(text.as_str().into());
            suggest(ctx, "");
            ctx.go(Page::Messages(MessagesView::New));
        }
        "Delete" => edit(ctx, st::EDIT_DELETE, id),
        "Details" => ctx.m.borrow_mut().msgs.details = Some(id),
        "Retry" => retry(ctx, id),
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

/// The contacts matching what is typed in To.
fn suggest(ctx: &Ctx, typed: &str) {
    let mut m = ctx.m.borrow_mut();
    let found: Vec<(String, String)> = messages::suggestions(&m.book, m.order, typed, 6)
        .into_iter()
        .map(|(_, name, label, number)| (format!("{name}\n{label}"), number))
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
    let to: Vec<String> = if ctx.page() == Page::Messages(MessagesView::New) {
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
            dprintln!("neophone: message not queued ({})", r);
            failed += 1;
        }
    }
    if failed == to.len() {
        ctx.note("Can't send to this number");
        return;
    }
    g.set_draft(SharedString::new());
    sync(ctx);
    if ctx.page() == Page::Messages(MessagesView::New) {
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
    } else {
        ctx.show();
    }
}

/// Send quick reply `text` to `number` (Reply with message): the note says to whom.
pub fn quick_reply(ctx: &Ctx, number: &str, text: &str) {
    let r = ctx.m.borrow_mut().modem.sms_send(number, text);
    if r < 0 {
        ctx.note(&format!("Message not sent ({r})"));
        return;
    }
    let who = people::name_of(ctx, number).unwrap_or_else(|| String::from(number));
    let first = String::from(who.split_whitespace().next().unwrap_or(&who));
    ctx.note(&format!("Message sent to {first}"));
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
        m.msgs.shown = messages::search(&m.msgs.mirror, &threads, &g.get_search());
        m.msgs.threads = threads;
    }
    let m = ctx.m.borrow();
    let s = &m.msgs;
    match view {
        MessagesView::List => {
            g.set_empty(s.threads.is_empty());
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
                    }
                })
                .collect();
            g.set_threads(model(rows));
        }
        MessagesView::Thread => {
            let (name, _) = messages::who(&m.book, m.order, &s.open);
            g.set_title(name.as_str().into());
            let idx = messages::thread(&s.mirror, &s.open);
            let bands = messages::bands(&s.mirror, &idx, now, offset);
            let bubbles: Vec<Bubble> = idx
                .iter()
                .zip(bands)
                .map(|(&i, band)| {
                    let e = &s.mirror.msgs[i].entry;
                    Bubble {
                        id: e.id as i32,
                        text: s.mirror.msgs[i].text.as_str().into(),
                        mine: !e.incoming,
                        time: messages::time(e.utc, offset).into(),
                        state: match e.state {
                            _ if e.incoming => 0,
                            st::SENDING => 1,
                            st::SENT => 2,
                            st::DELIVERED => 3,
                            _ => 4,
                        },
                        band: band.into(),
                    }
                })
                .collect();
            g.set_bubbles(model(bubbles));
        }
        MessagesView::New => {
            let names: Vec<&str> = s.recipients.iter().map(|(n, _)| n.as_str()).collect();
            g.set_recipients(strings(&names));
            let sugg: Vec<Suggestion> = s
                .suggestions
                .iter()
                .map(|(name_label, number)| {
                    let (name, label) = name_label.split_once('\n').unwrap_or((name_label, ""));
                    let id = m.book.lookup(number).map_or(0, |c| c.id);
                    Suggestion {
                        name: name.into(),
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
            let mut last_band = String::new();
            let rows: Vec<BlockedMessage> = idx
                .iter()
                .map(|&i| {
                    let msg = &s.mirror.msgs[i];
                    let e = &msg.entry;
                    let (name, contact) = messages::who(&m.book, m.order, msg.number());
                    let band =
                        String::from(match phone_core::recents::Section::of(e.utc, now, offset) {
                            phone_core::recents::Section::Today => "Today",
                            phone_core::recents::Section::Yesterday => "Yesterday",
                            _ => "Earlier",
                        });
                    let show_band = band != last_band;
                    last_band = band.clone();
                    BlockedMessage {
                        id: e.id as i32,
                        band: if show_band {
                            band.into()
                        } else {
                            SharedString::new()
                        },
                        title: name.as_str().into(),
                        snippet: msg.text.as_str().into(),
                        why: messages::why_text(e.why, contact.is_some()).into(),
                        when: contacts::recents::when(e.utc, now, offset).into(),
                        initials: initials(&name).into(),
                        tint: color(phone_core::people::desaturate(contact.map_or(0, tint))),
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
    // the sheets and dialogs
    let held = s.held.and_then(|id| s.mirror.get(id));
    g.set_bubble_open(held.is_some());
    let items: &[&str] = match held {
        Some(h) if h.entry.state == st::FAILED => {
            &["Retry", "Copy text", "Forward", "Delete", "Details"]
        }
        Some(_) => &["Copy text", "Forward", "Delete", "Details"],
        None => &[],
    };
    if held.is_some() {
        g.set_bubble_items(strings(items));
    }
    g.set_blocked_open(s.held_blocked.is_some());
    match s.details.and_then(|id| s.mirror.get(id)) {
        Some(msg) => {
            let (name, _) = messages::who(&m.book, m.order, msg.number());
            let lines = messages::details(msg, &name, now, offset);
            let keys: Vec<&str> = lines.iter().map(|(k, _)| *k).collect();
            let values: Vec<&str> = lines.iter().map(|(_, v)| v.as_str()).collect();
            g.set_details_keys(strings(&keys));
            g.set_details_values(strings(&values));
            g.set_details_open(true);
        }
        None => g.set_details_open(false),
    }
}

/// The question open, asked with the UI server's dialog.
pub fn query(ctx: &Ctx) -> Option<neoapp::Query> {
    let (q, detail) = match ctx.m.borrow().msgs.query {
        Query::None => return None,
        Query::DeleteAll => (
            "Delete all conversations?",
            "Every message is deleted from the phone.",
        ),
        Query::DeleteThread => ("Delete this conversation?", ""),
        Query::ClearBlocked => ("Delete all blocked messages?", ""),
    };
    Some(crate::ctx::ask(q, detail, "Delete"))
}

pub fn answer(ctx: &Ctx, yes: bool) {
    act(ctx, if yes { "confirm" } else { "cancel" }, "");
    ctx.show();
}

pub fn chrome(ctx: &Ctx) -> Chrome {
    let m = ctx.m.borrow();
    match m.page {
        Page::Messages(MessagesView::Thread) => {
            let (name, _) = messages::who(&m.book, m.order, &m.msgs.open);
            Chrome::titled(&name).with(tool::CALL, tool::PILL, "Call")
        }
        Page::Messages(MessagesView::New) => {
            Chrome::titled("New message").with(tool::NONE, tool::CANCEL, "")
        }
        Page::Messages(MessagesView::Blocked) => {
            let c = Chrome::titled("Blocked messages");
            if messages::blocked(&m.msgs.mirror).is_empty() {
                c
            } else {
                c.with(tool::DELETE, 0, "Clear")
            }
        }
        _ => Chrome::titled("Messages").with(tool::EDIT, 0, "New"),
    }
}

pub fn options(ctx: &Ctx) -> Vec<&'static str> {
    let m = ctx.m.borrow();
    let paste = !m.msgs.clipboard.is_empty();
    let mut v = Vec::new();
    match m.page {
        Page::Messages(MessagesView::List) => {
            v.extend([
                "Mark all as read",
                "Delete conversations",
                "Blocked messages",
                "Settings",
            ]);
        }
        Page::Messages(MessagesView::Thread) => {
            if paste {
                v.push("Paste");
            }
            v.push("Call");
            v.push(if m.book.lookup(&m.msgs.open).is_some() {
                "Contact card"
            } else {
                "Add to contacts"
            });
            v.extend(["Delete conversation", "Block number"]);
        }
        Page::Messages(MessagesView::New) if paste => v.push("Paste"),
        Page::Messages(MessagesView::Blocked) if !messages::blocked(&m.msgs.mirror).is_empty() => {
            v.push("Delete all")
        }
        _ => {}
    }
    v
}

pub fn option(ctx: &Ctx, item: &str) {
    let open = ctx.m.borrow().msgs.open.clone();
    match item {
        "Mark all as read" => edit(ctx, st::EDIT_ALL_READ, 0),
        "Delete conversations" => ctx.m.borrow_mut().msgs.query = Query::DeleteAll,
        "Blocked messages" => return ctx.go(Page::Messages(MessagesView::Blocked)),
        "Settings" => return ctx.go(Page::Settings(SettingsView::Main)),
        "Paste" => {
            let clip = ctx.m.borrow().msgs.clipboard.clone();
            let ui = ctx.ui();
            let g = ui.global::<MessagesState>();
            let mut d = String::from(g.get_draft().as_str());
            d.push_str(&clip);
            g.set_draft(d.as_str().into());
        }
        "Call" => return crate::dial::place_call(ctx, &open),
        "Contact card" => {
            people::open_for_number(ctx, &open);
            return;
        }
        "Add to contacts" => return people::new_with_number(ctx, &open),
        "Delete conversation" => ctx.m.borrow_mut().msgs.query = Query::DeleteThread,
        "Block number" => prefs::block_number(ctx, &open, true, true),
        "Delete all" => ctx.m.borrow_mut().msgs.query = Query::ClearBlocked,
        _ => {}
    }
    ctx.show();
}

/// The toolbar's middle button: New, Call, Clear.
pub fn action(ctx: &Ctx) {
    match ctx.page() {
        Page::Messages(MessagesView::List) => {
            {
                let mut m = ctx.m.borrow_mut();
                m.msgs.recipients.clear();
                m.msgs.suggestions.clear();
            }
            let ui = ctx.ui();
            let g = ui.global::<MessagesState>();
            g.set_to_text(SharedString::new());
            g.set_draft(SharedString::new());
            ctx.go(Page::Messages(MessagesView::New));
        }
        Page::Messages(MessagesView::Thread) => {
            let open = ctx.m.borrow().msgs.open.clone();
            crate::dial::place_call(ctx, &open);
        }
        Page::Messages(MessagesView::Blocked) => {
            ctx.m.borrow_mut().msgs.query = Query::ClearBlocked;
            ctx.show();
        }
        _ => {}
    }
}

/// Back closes a sheet, the details or a query first.
pub fn back(ctx: &Ctx) -> bool {
    if !wants_back(ctx) {
        return false;
    }
    {
        let mut m = ctx.m.borrow_mut();
        m.msgs.held = None;
        m.msgs.held_blocked = None;
        m.msgs.details = None;
        m.msgs.query = Query::None;
    }
    ctx.ui().global::<KeyboardFocus>().set_open(false);
    ctx.show();
    true
}

pub fn wants_back(ctx: &Ctx) -> bool {
    let m = ctx.m.borrow();
    m.msgs.held.is_some()
        || m.msgs.held_blocked.is_some()
        || m.msgs.details.is_some()
        || m.msgs.query != Query::None
}
