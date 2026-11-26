// SPDX-License-Identifier: GPL-3.0-only
//! Contacts (NEOCONT.EXE; NEOPHONE v2 boards p2-contacts-*): the list with the header's search, favourites and the
//! per-script letter strip, selection mode, the card (favourite, block, speed dial, vCard, my card), the editor with
//! its number-type sheet, and the Delete and Block queries. The model is the host-tested `contacts` crate and `phone_core::people`; every change is
//! saved at once ([`store::save`]) and MODEMD gets the new known-number hashes ([`send_known`]).

use crate::ctx::Ctx;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use contacts::index::{self, Script};
use contacts::{Birthday, Contact, Label, NUMBERS_MAX, NameOrder, Refused, vcard};
use contacts_ui::{
    ContactEditNumber, ContactHit, ContactMark, ContactNumber, ContactRow, ContactSection,
    ContactsApp, ContactsState, KeyboardFocus, Nav,
};
use e32::dprintln;
use e32::fs::{File, Fs, mode};
use neoui_proto::settings::Value;
use phone_common::store;
use phone_common::ui::{Chrome, model, strings, tool};
use phone_core::page::{ContactsView, MessagesView, Page, PhoneView, SettingsView};
use phone_core::people as logic;
use slint::{Color, ComponentHandle, Model as _, SharedString};

/// Whose card or editor shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Who {
    #[default]
    Mine,
    Contact(u32),
    /// the editor's new contact
    New,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Dialog {
    #[default]
    None,
    Delete,
    Block,
    DeleteMarked,
    /// leave the editor (or a dialog) without saving: ✕, Back, or another app's OPEN
    Discard,
}

/// Another app's OPEN, held while the Discard query asks (neoui ipc OPEN: the page number, the page, its argument)
/// and the dialog it replaced (shown again on Cancel).
struct Pending {
    n: u16,
    page: Page,
    arg: String,
    before: Dialog,
}

/// The heights (dp) the letter strip's room comes from: the header and the footer, and the strip's margins.
const BARS_H: f32 = 48.0 + 48.0;
const STRIP_MARGINS: f32 = 12.0 + 12.0;

/// The card's choice sheet: Send as vCard (Message / Save as vCard file), or which number goes on a speed-dial key.
#[derive(Clone, Default, PartialEq)]
enum Choice {
    #[default]
    None,
    Share,
    Speed(Vec<String>),
}

pub struct State {
    card: Who,
    editing: Who,
    /// where Back from the card goes when it was opened from outside Contacts (call details)
    card_from: Option<Page>,
    /// where Back from the list goes after another app's IMPORT (Files' Add to Contacts)
    list_from: Option<Page>,
    /// where the editor's Cancel goes
    edit_from: Page,
    dialog: Dialog,
    /// the block query's Calls and Messages
    checks: [bool; 2],
    /// the number-type picker's row
    picking: Option<usize>,
    /// the card's choice sheet
    choice: Choice,
    marked: Vec<u32>,
    /// the header's search text
    query: String,
    /// bumped by every change to the book; the list is rebuilt when it (or the name order, or the app's height)
    /// changed
    rev: u32,
    shown: Option<(u32, NameOrder, usize)>,
    /// the view the last show drew
    last_view: Option<ContactsView>,
    /// the editor's fields as it opened (✕ asks first when they changed)
    start: Contact,
    pending: Option<Pending>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            card: Who::Mine,
            editing: Who::New,
            card_from: None,
            list_from: None,
            edit_from: Page::Contacts(ContactsView::List),
            dialog: Dialog::None,
            checks: [true, true],
            picking: None,
            choice: Choice::None,
            marked: Vec::new(),
            query: String::new(),
            rev: 0,
            shown: None,
            last_view: None,
            start: Contact::default(),
            pending: None,
        }
    }
}

fn color(rgb: u32) -> Color {
    Color::from_rgb_u8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

fn chr(c: char) -> SharedString {
    SharedString::from(&*c.encode_utf8(&mut [0; 4]))
}

fn view(ctx: &Ctx) -> Option<ContactsView> {
    match ctx.page() {
        Page::Contacts(v) => Some(v),
        _ => None,
    }
}

/// The book changed: the list is rebuilt at the next show.
fn changed(ctx: &Ctx) {
    let mut m = ctx.m.borrow_mut();
    m.people.rev = m.people.rev.wrapping_add(1);
}

/// Save the book; a failure becomes the note (true when saved). Phone and Messages read it again
/// (`contacts.rev` changed).
fn save(ctx: &Ctx) -> bool {
    let r = store::save(&ctx.m.borrow().book);
    if let Err(e) = r {
        dprintln!("neocont: contacts not saved ({})", e);
        ctx.note(&format!("Contacts not saved ({e})"));
        return false;
    }
    let rev = match ctx.app.setting_raw(phone_common::CONTACTS_REV) {
        Ok(Value::Int(n)) => n.wrapping_add(1),
        _ => 1,
    };
    if let Err(e) = ctx
        .app
        .set_setting_raw(phone_common::CONTACTS_REV, &Value::Int(rev))
    {
        dprintln!("neocont: contacts.rev not set ({})", e);
    }
    true
}

fn contact(m: &crate::ctx::Model, who: Who) -> Option<&Contact> {
    match who {
        Who::Mine => m.book.my_card.as_ref(),
        Who::Contact(id) => m.book.get(id),
        Who::New => None,
    }
}

/// The card's contact (a clone, so nothing stays borrowed).
fn card(ctx: &Ctx) -> Option<Contact> {
    let m = ctx.m.borrow();
    contact(&m, m.people.card).cloned()
}

pub fn bind(ctx: &Rc<Ctx>, ui: &ContactsApp) {
    let labels: Vec<&str> = Label::ALL.iter().map(|l| l.text()).collect();
    ui.global::<ContactsState>().set_labels(strings(&labels));
    let ctx = Rc::downgrade(ctx);
    ui.global::<ContactsState>().on_act(move |action, value| {
        let Some(ctx) = ctx.upgrade() else { return };
        act(&ctx, &action, &value);
    });
}

fn act(ctx: &Ctx, action: &str, value: &str) {
    let ui = ctx.ui();
    let st = ui.global::<ContactsState>();
    let n = value.parse::<i32>().unwrap_or(-1);
    match action {
        "open" if n < 0 => {
            if ctx.m.borrow().book.my_card.is_some() {
                open_card(ctx, Who::Mine, None);
            } else {
                edit(ctx, Who::Mine, "");
            }
        }
        "open" => open_card(ctx, Who::Contact(n as u32), None),
        // a long press on a row: selection mode with it marked
        "hold" => {
            ctx.m.borrow_mut().people.marked = alloc::vec![n as u32];
            changed(ctx);
            ctx.go(Page::Contacts(ContactsView::Mark));
        }
        "new" => edit(ctx, Who::New, ""),
        "call" => ctx.call(value),
        "unblock" => unblock(ctx),
        "mark" => mark(ctx, n as u32),
        "label" => {
            ctx.m.borrow_mut().people.picking = usize::try_from(n).ok();
            ctx.show();
        }
        "pick" => {
            let row = ctx.m.borrow_mut().people.picking.take();
            let rows = st.get_edit_numbers();
            if let Some(r) = row
                && let Some(mut e) = rows.row_data(r)
            {
                e.kind = n.max(0);
                rows.set_row_data(r, e);
            }
            ctx.show();
        }
        "choice" => {
            let choice = core::mem::take(&mut ctx.m.borrow_mut().people.choice);
            let c = card(ctx);
            match (choice, c) {
                (Choice::Share, Some(c)) if n == 0 => {
                    let text =
                        logic::share_text(&c, ctx.m.borrow().order, neoui_proto::ipc::ARG_MAX);
                    ctx.open(Page::Messages(MessagesView::New), &text);
                }
                (Choice::Share, Some(c)) if n == 1 => save_vcard(ctx, &c),
                (Choice::Speed(numbers), _) => {
                    if let Some(number) = usize::try_from(n).ok().and_then(|i| numbers.get(i)) {
                        ctx.open(Page::Phone(PhoneView::Speed), number);
                    }
                }
                _ => {}
            }
            ctx.show();
        }
        "choice-cancel" => {
            ctx.m.borrow_mut().people.choice = Choice::None;
            ctx.show();
        }
        "pick-cancel" => {
            ctx.m.borrow_mut().people.picking = None;
            ctx.show();
        }
        "add-number" => {
            let mut rows: Vec<ContactEditNumber> = st.get_edit_numbers().iter().collect();
            if rows.len() < NUMBERS_MAX {
                rows.push(ContactEditNumber {
                    kind: default_label(ctx) as i32,
                    number: "".into(),
                });
                st.set_edit_numbers(model(rows));
            }
        }
        "remove-number" => {
            let mut rows: Vec<ContactEditNumber> = st.get_edit_numbers().iter().collect();
            if let Ok(i) = usize::try_from(n)
                && i < rows.len()
            {
                rows.remove(i);
                st.set_edit_numbers(model(rows));
            }
        }
        "confirm" => confirm(ctx),
        "cancel" => {
            ctx.m.borrow_mut().people.dialog = Dialog::None;
            ctx.show();
        }
        "check" => {
            if let Ok(i @ 0..=1) = usize::try_from(n) {
                let c = &mut ctx.m.borrow_mut().people.checks;
                c[i] = !c[i];
            }
            ctx.show();
        }
        "message" => ctx.message(value, false),
        // shortcut: no Email or Photos app yet (the owner's decision: the buttons show and say so); open them once
        // there are
        "email" => ctx.note("Needs an Email app"),
        "photo" => ctx.note("Needs the Photos app"),
        _ => {}
    }
}

fn default_label(ctx: &Ctx) -> Label {
    Label::from_u8(ctx.m.borrow().prefs.default_label).unwrap_or_default()
}

fn open_card(ctx: &Ctx, who: Who, from: Option<Page>) {
    {
        let mut m = ctx.m.borrow_mut();
        m.people.card = who;
        m.people.card_from = from;
    }
    ctx.go(Page::Contacts(ContactsView::Card));
}

/// Open the editor on `who` (a new contact gets `number`, if any).
fn edit(ctx: &Ctx, who: Who, number: &str) {
    let label = default_label(ctx);
    let c = {
        let mut m = ctx.m.borrow_mut();
        m.people.edit_from = m.page;
        m.people.editing = who;
        m.people.picking = None;
        contact(&m, who).cloned().unwrap_or_default()
    };
    let mut rows: Vec<ContactEditNumber> = c
        .numbers
        .iter()
        .map(|(l, n)| ContactEditNumber {
            kind: *l as i32,
            number: n.as_str().into(),
        })
        .collect();
    if rows.is_empty() {
        rows.push(ContactEditNumber {
            kind: label as i32,
            number: number.into(),
        });
    }
    let ui = ctx.ui();
    let st = ui.global::<ContactsState>();
    st.set_edit_first(c.first.as_str().into());
    st.set_edit_last(c.last.as_str().into());
    st.set_edit_company(c.company.as_str().into());
    st.set_edit_email(c.email.as_str().into());
    st.set_edit_note(c.note.as_str().into());
    st.set_edit_numbers(model(rows));
    let b = c.birthday;
    st.set_edit_bday_day(b.map_or(0, |b| i32::from(b.day)));
    st.set_edit_bday_month(b.map_or(1, |b| i32::from(b.month)));
    st.set_edit_bday_year(b.and_then(|b| b.year).map_or(0, i32::from));
    st.set_dating(false);
    ctx.m.borrow_mut().people.start = form(ctx);
    ctx.go(Page::Contacts(ContactsView::Edit));
}

/// The editor's fields as a contact (what Save stores).
fn form(ctx: &Ctx) -> Contact {
    let ui = ctx.ui();
    let st = ui.global::<ContactsState>();
    Contact {
        first: st.get_edit_first().into(),
        last: st.get_edit_last().into(),
        company: st.get_edit_company().into(),
        email: st.get_edit_email().into(),
        note: st.get_edit_note().into(),
        numbers: st
            .get_edit_numbers()
            .iter()
            .map(|e| {
                let l = u8::try_from(e.kind).ok().and_then(Label::from_u8);
                (l.unwrap_or_default(), String::from(e.number.as_str()))
            })
            .collect(),
        birthday: (st.get_edit_bday_day() > 0)
            .then(|| {
                let year = u16::try_from(st.get_edit_bday_year())
                    .ok()
                    .filter(|y| *y > 0);
                let (m, d) = (st.get_edit_bday_month(), st.get_edit_bday_day());
                Birthday::new(u8::try_from(m).ok()?, u8::try_from(d).ok()?, year)
            })
            .flatten(),
        ..Default::default()
    }
}

/// The editor is open and its fields differ from what it opened with.
fn dirty(ctx: &Ctx) -> bool {
    view(ctx) == Some(ContactsView::Edit) && logic::edited(&ctx.m.borrow().people.start, &form(ctx))
}

/// Leave the editor without saving: back where it was opened from.
fn leave_editor(ctx: &Ctx) {
    let from = ctx.m.borrow().people.edit_from;
    ctx.back_to(from);
}

/// The editor's Save: check, store, show the card.
fn save_edit(ctx: &Ctx) {
    let mut c = form(ctx);
    if !logic::worth_saving(&c) {
        ctx.note("Add a name or a number");
        return;
    }
    let (who, from) = {
        let m = ctx.m.borrow();
        (m.people.editing, m.people.edit_from)
    };
    let saved = {
        let mut m = ctx.m.borrow_mut();
        match who {
            Who::Mine => {
                m.book.set_my_card(Some(c));
                Ok(Who::Mine)
            }
            Who::New => m.book.add(c).map(Who::Contact),
            Who::Contact(id) => {
                c.id = id;
                c.favourite = m.book.get(id).is_some_and(|x| x.favourite);
                m.book.update(c).map(|()| Who::Contact(id))
            }
        }
    };
    match saved {
        Ok(who) => {
            changed(ctx);
            let ok = save(ctx);
            send_known(ctx);
            // back where the card was opened from: another app (a new contact it asked for), or whoever opened the
            // card that was edited
            let back = if matches!(from, Page::Contacts(_)) {
                ctx.m.borrow().people.card_from
            } else {
                Some(from)
            };
            open_card(ctx, who, back);
            if ok {
                ctx.note("Saved");
            }
        }
        Err(Refused::Full) => ctx.note("Contacts are full"),
        Err(_) => ctx.note("Not saved"),
    }
}

fn mark(ctx: &Ctx, id: u32) {
    let on = {
        let mut m = ctx.m.borrow_mut();
        let marked = &mut m.people.marked;
        match marked.iter().position(|&x| x == id) {
            Some(i) => {
                marked.remove(i);
                false
            }
            None => {
                marked.push(id);
                true
            }
        }
    };
    // flip the one row in place: rebuilding a long list per tap is slow
    let ui = ctx.ui();
    let st = ui.global::<ContactsState>();
    for s in st.get_sections().iter() {
        if let Some(i) = s.people.iter().position(|p| p.id == id as i32) {
            let p = s.people.row_data(i).unwrap();
            s.people.set_row_data(i, ContactRow { marked: on, ..p });
        }
    }
}

/// What blocks any of `c`'s numbers (calls, messages), if anything does.
fn blocked_any(ctx: &Ctx, c: &Contact) -> Option<(bool, bool)> {
    c.numbers
        .iter()
        .filter_map(|(_, n)| ctx.blocked(n))
        .reduce(|(a, b), (x, y)| (a || x, b || y))
        .filter(|&(calls, messages)| calls || messages)
}

/// Unblock every number of the card's contact.
fn unblock(ctx: &Ctx) {
    if let Some(c) = card(ctx) {
        for (_, n) in &c.numbers {
            ctx.unblock_number(n);
        }
    }
    ctx.show();
}

fn ask(ctx: &Ctx, d: Dialog) {
    {
        let mut m = ctx.m.borrow_mut();
        m.people.dialog = d;
        m.people.checks = [true, true];
    }
    ctx.show();
}

/// The query's red button.
fn confirm(ctx: &Ctx) {
    let d = core::mem::take(&mut ctx.m.borrow_mut().people.dialog);
    match d {
        Dialog::None => {}
        Dialog::Block => {
            let [calls, messages] = ctx.m.borrow().people.checks;
            if let Some(c) = card(ctx)
                && (calls || messages)
            {
                for (_, n) in &c.numbers {
                    ctx.block_number(n, calls, messages);
                }
            }
            ctx.show();
        }
        Dialog::Delete => {
            let who = ctx.m.borrow().people.card;
            let gone = match who {
                Who::Mine => ctx.m.borrow_mut().book.my_card.take(),
                Who::Contact(id) => {
                    let c = ctx.m.borrow().book.get(id).cloned();
                    ctx.m.borrow_mut().book.remove(id);
                    c
                }
                Who::New => None,
            };
            forget(ctx, gone.into_iter().collect());
        }
        Dialog::DeleteMarked => {
            let ids = core::mem::take(&mut ctx.m.borrow_mut().people.marked);
            let gone: Vec<Contact> = ids
                .iter()
                .filter_map(|&id| {
                    let c = ctx.m.borrow().book.get(id).cloned();
                    ctx.m.borrow_mut().book.remove(id);
                    c
                })
                .collect();
            forget(ctx, gone);
        }
        Dialog::Discard => {
            ctx.m.borrow_mut().people.picking = None;
            let pending = ctx.m.borrow_mut().people.pending.take();
            match pending {
                Some(p) => crate::opened(ctx, p.n, p.page, &p.arg),
                None => leave_editor(ctx),
            }
        }
    }
}

/// After a delete: clear the deleted contacts' speed-dial keys, save, tell MODEMD, back to the list.
fn forget(ctx: &Ctx, gone: Vec<Contact>) {
    let mut speed = ctx.m.borrow().prefs.speed.clone();
    let mut cleared = false;
    for c in &gone {
        cleared |= logic::clear_speed(&mut speed, c);
    }
    if cleared {
        match phone_common::prefs::save_speed(&ctx.app, &speed) {
            Ok(()) => ctx.m.borrow_mut().prefs.speed = speed,
            Err(e) => dprintln!("neocont: speed dial not saved ({})", e),
        }
    }
    changed(ctx);
    let ok = save(ctx);
    send_known(ctx);
    ctx.m.borrow_mut().people.marked.clear();
    // a card another app opened: back there
    let from = ctx.m.borrow_mut().people.card_from.take();
    match from {
        Some(from) => ctx.back_to(from),
        None => ctx.go(Page::Contacts(ContactsView::List)),
    }
    if ok {
        ctx.note(&match gone.len() {
            1 => String::from("Deleted"),
            n => format!("{n} contacts deleted"),
        });
    }
}

/// Export `contacts` as `C:\Data\contacts.vcf`, with a note.
fn export(ctx: &Ctx, contacts: &[Contact]) {
    if contacts.is_empty() {
        ctx.note("No contacts to export");
        return;
    }
    let text = vcard::export(contacts);
    match store::export(&text) {
        Ok(()) => ctx.note(&format!(
            "{} saved to {}",
            match contacts.len() {
                1 => String::from("vCard"),
                n => format!("{n} vCards"),
            },
            store::EXPORT_PATH
        )),
        Err(e) => ctx.note(&format!("Export failed ({e})")),
    }
}

/// Send as vCard ▸ Save as vCard file: the card as `C:\Data\<name>.vcf` (not over the full export), with a note saying
/// where.
fn save_vcard(ctx: &Ctx, c: &Contact) {
    let name = c.title(ctx.m.borrow().order);
    let path = format!("{}{}", store::EXPORT_DIR, logic::vcard_file(&name));
    let text = vcard::export(core::slice::from_ref(c));
    let r = (|| {
        let fs = Fs::connect()?;
        store::mkdirs(&fs, &[store::EXPORT_DIR])?;
        File::replace(&fs, &path, mode::SHARE_EXCLUSIVE)?.write(text.as_bytes())
    })();
    match r {
        Ok(()) => ctx.note(&format!("vCard saved to {path}")),
        Err(e) => ctx.note(&format!("vCard not saved ({e})")),
    }
}

/// Every `.vcf` in `E:\` and `C:\Data\` (not our own export) into the book.
fn import(ctx: &Ctx) {
    let files: Vec<(String, Vec<u8>)> = store::vcards()
        .into_iter()
        .filter(|(p, _)| !p.eq_ignore_ascii_case(store::EXPORT_PATH))
        .collect();
    if files.is_empty() {
        ctx.note("No .vcf files in E:\\ or C:\\Data\\");
        return;
    }
    let (mut added, mut skipped) = (0, 0);
    for (_, bytes) in &files {
        let found = vcard::import(&String::from_utf8_lossy(bytes));
        let (a, s) = logic::merge(&mut ctx.m.borrow_mut().book, found.contacts);
        added += a;
        skipped += s + found.skipped;
    }
    dprintln!("neocont: vCards imported {} ({} skipped)", added, skipped);
    changed(ctx);
    let ok = added == 0 || save(ctx);
    send_known(ctx);
    ctx.show();
    if ok {
        ctx.note(&format!("{added} added"));
    }
}

/// Another app opened Contacts on IMPORT with a .vcf `path` (Files' Add to Contacts): its contacts join the book
/// (the ones it has are skipped, [`contacts::Book::has_same`]), the list shows with a note, and Back returns to that
/// app.
pub fn import_file(ctx: &Ctx, path: &str) {
    ctx.go(Page::Contacts(ContactsView::List));
    ctx.m.borrow_mut().people.list_from = Some(Page::Phone(PhoneView::Recents));
    let bytes = match Fs::connect().and_then(|fs| store::read(&fs, path)) {
        Ok(b) => b,
        Err(e) => {
            dprintln!("neocont: {} not read ({})", path, e);
            ctx.note(&format!("Can't read the vCard ({e})"));
            return;
        }
    };
    let found = vcard::import(&String::from_utf8_lossy(&bytes));
    let (added, known) = logic::merge(&mut ctx.m.borrow_mut().book, found.contacts);
    let unreadable = found.skipped;
    dprintln!(
        "neocont: {} imported {} ({} known, {} unreadable)",
        path,
        added,
        known,
        unreadable
    );
    changed(ctx);
    let ok = added == 0 || save(ctx);
    send_known(ctx);
    ctx.show();
    if ok {
        let mut text = match (added, known) {
            (1, 0) => String::from("1 contact added"),
            (a, 0) => format!("{a} contacts added"),
            (a, k) => format!("{a} added · {k} already in Contacts"),
        };
        if unreadable > 0 {
            text.push_str(&format!(" · {unreadable} unreadable"));
        }
        ctx.note(&text);
    }
}

/// The app went to the background: an IMPORT's way back is over (a later visit to the list stays in Contacts).
pub fn hidden(ctx: &Ctx) {
    ctx.m.borrow_mut().people.list_from = None;
}

pub fn show(ctx: &Ctx) {
    let Some(v) = view(ctx) else {
        ctx.m.borrow_mut().people.last_view = None;
        return;
    };
    // a field removed with its view never reports losing focus: the keyboard goes with the view
    if ctx.m.borrow_mut().people.last_view.replace(v) != Some(v) {
        let ui = ctx.ui();
        ui.global::<KeyboardFocus>().set_open(false);
        // the header's search belongs to the list: a card opened from its results starts a fresh header
        if v != ContactsView::List && ui.global::<Nav>().get_searching() {
            ui.global::<Nav>().set_searching(false);
            ui.global::<Nav>().set_query("".into());
            ctx.m.borrow_mut().people.query.clear();
        }
    }
    match v {
        ContactsView::List | ContactsView::Mark => {
            show_list(ctx);
            show_results(ctx);
        }
        ContactsView::Card => show_card(ctx),
        // the editor's fields are filled when it opens: a show must not overwrite what is being typed
        ContactsView::Edit => {}
    }
    show_dialog(ctx);
}

fn show_list(ctx: &Ctx) {
    let ui = ctx.ui();
    let st = ui.global::<ContactsState>();
    let m = ctx.m.borrow();
    let key = (m.people.rev, m.order, ctx.app.height());
    if m.people.shown == Some(key) {
        return;
    }
    let order = m.order;
    let row = |c: &Contact, name: String| ContactRow {
        id: c.id as i32,
        initials: logic::initials(&c.title(order)).as_str().into(),
        name: name.as_str().into(),
        tint: color(logic::tint(c.id)),
        marked: m.people.marked.contains(&c.id),
    };
    // shortcut: the phone's language is taken as English (Latin first) until neoui has a language setting
    let sections = index::sections(&m.book, order, Script::Latin);
    let tops = logic::tops(&sections);
    let favourites: Vec<ContactRow> = m
        .book
        .sorted(order)
        .into_iter()
        .filter_map(|id| m.book.get(id))
        .take_while(|c| c.favourite)
        .map(|c| {
            let t = c.title(order);
            let first = t.split_whitespace().next().unwrap_or("").into();
            row(c, first)
        })
        .collect();
    let list_h = ctx.app.height() as f32 / ctx.app.dp - BARS_H;
    let marks = index::side_index(&sections, logic::index_room(list_h - STRIP_MARGINS));
    let letter = |i: usize| chr(sections[i].letter);
    let spans = logic::index_spans(&marks, &tops);
    st.set_index(model(
        marks
            .iter()
            .zip(spans)
            .map(|(k, (start, end))| ContactMark {
                text: chr(k.text),
                letters: model(k.sections.iter().map(|&i| letter(i)).collect()),
                tops: model(k.sections.iter().map(|&i| tops[i]).collect()),
                start,
                end,
            })
            .collect(),
    ));
    st.set_sections(model(
        sections
            .iter()
            .zip(&tops)
            .enumerate()
            .map(|(i, (s, &top))| ContactSection {
                letter: letter(i),
                top,
                people: model(
                    s.ids
                        .iter()
                        .filter_map(|&id| m.book.get(id))
                        .map(|c| row(c, c.title(order)))
                        .collect(),
                ),
            })
            .collect(),
    ));
    st.set_favourites(model(favourites));
    st.set_total(m.book.contacts.len() as i32);
    drop(m);
    ctx.m.borrow_mut().people.shown = Some(key);
}

fn show_results(ctx: &Ctx) {
    let ui = ctx.ui();
    let st = ui.global::<ContactsState>();
    let m = ctx.m.borrow();
    let q = m.people.query.as_str();
    st.set_has_query(!q.is_empty());
    if q.is_empty() {
        st.set_results(model(Vec::new()));
        return;
    }
    let hits: Vec<ContactHit> = m
        .book
        .search(q, m.order)
        .into_iter()
        .filter_map(|id| m.book.get(id))
        .map(|c| {
            let h = logic::hit(c, q, m.order);
            let [np, nh, nt] = h.name.map(|s| SharedString::from(s.as_str()));
            let [dp, dh, dt] = h.detail.map(|s| SharedString::from(s.as_str()));
            ContactHit {
                id: c.id as i32,
                initials: logic::initials(&c.title(m.order)).as_str().into(),
                tint: color(logic::tint(c.id)),
                name_pre: np,
                name_hit: nh,
                name_post: nt,
                detail_pre: dp,
                detail_hit: dh,
                detail_post: dt,
            }
        })
        .collect();
    st.set_results(model(hits));
}

fn show_card(ctx: &Ctx) {
    let Some(c) = card(ctx) else { return };
    let (mine, order, speed) = {
        let m = ctx.m.borrow();
        (
            m.people.card == Who::Mine,
            m.order,
            logic::speed_key(&m.prefs.speed, &c),
        )
    };
    let blocked = blocked_any(ctx, &c);
    let title = c.title(order);
    let tint = if mine { 0x2f80ed } else { logic::tint(c.id) };
    let ui = ctx.ui();
    let st = ui.global::<ContactsState>();
    st.set_card_mine(mine);
    st.set_card_name(title.as_str().into());
    st.set_card_company(c.company.as_str().into());
    st.set_card_initials(logic::initials(&title).as_str().into());
    st.set_card_tint(color(tint));
    st.set_card_numbers(model(
        c.numbers
            .iter()
            .map(|(l, n)| ContactNumber {
                label: l.text().into(),
                number: n.as_str().into(),
            })
            .collect(),
    ));
    st.set_card_email(c.email.as_str().into());
    st.set_card_note(c.note.as_str().into());
    st.set_card_speed(speed.map_or(0, i32::from));
    let now = e32::time::now();
    let today = (
        u16::try_from(now.year).unwrap_or(0),
        now.month as u8,
        now.day as u8,
    );
    st.set_card_birthday(
        c.birthday
            .map(|b| logic::birthday_text(&b, today))
            .unwrap_or_default()
            .as_str()
            .into(),
    );
    st.set_card_blocked(blocked.map_or("", |(a, b)| logic::block_text(a, b)).into());
}

fn show_dialog(ctx: &Ctx) {
    let (d, checks, picking, marked) = {
        let m = ctx.m.borrow();
        (
            m.people.dialog,
            m.people.checks,
            m.people.picking,
            m.people.marked.len(),
        )
    };
    let c = card(ctx);
    let (name, key) = {
        let m = ctx.m.borrow();
        (
            c.as_ref().map(|c| c.title(m.order)).unwrap_or_default(),
            c.as_ref().and_then(|c| logic::speed_key(&m.prefs.speed, c)),
        )
    };
    let ui = ctx.ui();
    let st = ui.global::<ContactsState>();
    st.set_picking(picking.map_or(-1, |p| p as i32));
    let choice = ctx.m.borrow().people.choice.clone();
    let (title, rows): (&str, Vec<String>) = match (&choice, &c) {
        (Choice::Share, Some(_)) => (
            "Send as vCard",
            alloc::vec!["Message".into(), "Save as vCard file".into()],
        ),
        (Choice::Speed(numbers), Some(c)) => (
            "Speed dial",
            numbers
                .iter()
                .map(|n| {
                    let label = c
                        .numbers
                        .iter()
                        .find(|(_, x)| x == n)
                        .map_or("", |(l, _)| l.text());
                    format!("{label} · {n}")
                })
                .collect(),
        ),
        _ => ("", Vec::new()),
    };
    st.set_choice_title(title.into());
    st.set_choice_detail(name.as_str().into());
    st.set_choices(strings(
        &rows.iter().map(String::as_str).collect::<Vec<_>>(),
    ));
    // Block has check boxes: the app draws it; the deletes are the UI server's dialog ([`query`])
    st.set_dialog_open(d == Dialog::Block);
    let (question, detail, button, boxes): (String, String, &str, &[&str]) = match d {
        Dialog::None => return,
        Dialog::Delete => (
            format!("Delete {name}?"),
            logic::delete_detail(key),
            "Delete",
            &[],
        ),
        Dialog::DeleteMarked => (
            format!("Delete {marked} contacts?"),
            String::new(),
            "Delete",
            &[],
        ),
        Dialog::Discard => ("Discard changes?".into(), String::new(), "Discard", &[]),
        Dialog::Block => (
            format!("Block {name}?"),
            "Blocked calls are rejected as busy and never ring. You can still call them.".into(),
            "Block",
            &["Calls", "Messages"],
        ),
    };
    st.set_dialog_question(question.as_str().into());
    st.set_dialog_detail(detail.as_str().into());
    st.set_dialog_confirm(button.into());
    st.set_dialog_checks(strings(boxes));
    st.set_dialog_checked(model(checks[..boxes.len()].to_vec()));
}

/// The delete questions, asked with the UI server's dialog.
pub fn query(ctx: &Ctx) -> Option<neoapp::Query> {
    let d = ctx.m.borrow().people.dialog;
    let name = || {
        let m = ctx.m.borrow();
        card(ctx).map(|c| c.title(m.order)).unwrap_or_default()
    };
    match d {
        Dialog::Delete => {
            let key = {
                let m = ctx.m.borrow();
                card(ctx).and_then(|c| logic::speed_key(&m.prefs.speed, &c))
            };
            Some(phone_common::ui::ask(
                &format!("Delete {}?", name()),
                &logic::delete_detail(key),
                "Delete",
            ))
        }
        Dialog::DeleteMarked => {
            let (marked, order) = {
                let m = ctx.m.borrow();
                (m.people.marked.clone(), m.order)
            };
            let one = match marked[..] {
                [id] => ctx.m.borrow().book.get(id).cloned(),
                _ => None,
            };
            Some(match one {
                Some(c) => {
                    let key = logic::speed_key(&ctx.m.borrow().prefs.speed, &c);
                    phone_common::ui::ask(
                        &format!("Delete {}?", c.title(order)),
                        &logic::delete_detail(key),
                        "Delete",
                    )
                }
                None => phone_common::ui::ask(
                    &format!("Delete {} contacts?", marked.len()),
                    "",
                    "Delete",
                ),
            })
        }
        Dialog::Discard => Some(phone_common::ui::ask("Discard changes?", "", "Discard")),
        Dialog::None | Dialog::Block => None,
    }
}

pub fn answer(ctx: &Ctx, yes: bool) {
    if yes {
        confirm(ctx);
    } else {
        cancel(ctx);
    }
    ctx.show();
}

/// A query's Cancel: a held OPEN is dropped and the dialog it replaced comes back.
fn cancel(ctx: &Ctx) {
    let mut m = ctx.m.borrow_mut();
    m.people.dialog = match m.people.pending.take() {
        Some(p) if p.before != Dialog::Discard => p.before,
        _ => Dialog::None,
    };
}

pub fn chrome(ctx: &Ctx) -> Chrome {
    let m = ctx.m.borrow();
    let p = &m.people;
    let Page::Contacts(v) = m.page else {
        return Chrome::titled("Contacts");
    };
    match v {
        // 🔍 first, as the board draws it (the kit's own search icon would come after the actions)
        ContactsView::List => Chrome {
            more: alloc::vec![(tool::PERSON_ADD, "New contact")],
            ..Chrome::titled("Contacts").with(tool::SEARCH, 0, "Search contacts")
        },
        // [✕ N selected · 🗑 · ⋮], shown at 0 too (p2-contacts-select)
        ContactsView::Mark => Chrome {
            selected: p.marked.len() as i32,
            selecting: true,
            selection: alloc::vec![(tool::DELETE, "Delete marked")],
            selection_menu: alloc::vec!["Select all", "Export"],
            ..Chrome::titled("Select contacts")
        },
        ContactsView::Card => {
            let c = contact(&m, p.card);
            let mut more = Vec::new();
            if p.card != Who::Mine {
                let star = c.is_some_and(|c| c.favourite);
                more.push(if star {
                    (tool::STAR_FULL, "Remove from favourites")
                } else {
                    (tool::STAR, "Add to favourites")
                });
            }
            more.push((tool::EDIT, "Edit"));
            Chrome {
                more,
                ..Chrome::titled(&match p.card {
                    Who::Mine => String::from("My card"),
                    _ => c.map(|c| c.title(m.order)).unwrap_or_default(),
                })
            }
        }
        ContactsView::Edit => Chrome::titled(match p.editing {
            Who::New => "New contact",
            Who::Contact(_) => "Edit contact",
            Who::Mine => "My card",
        })
        .with(tool::CHECK, tool::CANCEL, "Save"),
    }
}

pub fn options(ctx: &Ctx) -> Vec<&'static str> {
    let Some(v) = view(ctx) else {
        return Vec::new();
    };
    match v {
        ContactsView::List => {
            alloc::vec!["Mark several", "Import vCards", "Export vCards", "Settings"]
        }
        ContactsView::Card => {
            let Some(c) = card(ctx) else {
                return Vec::new();
            };
            if ctx.m.borrow().people.card == Who::Mine {
                return alloc::vec!["Delete", "Send as vCard"];
            }
            let mut v = alloc::vec!["Delete"];
            if !c.numbers.is_empty() {
                v.push("Speed dial");
                v.push(if blocked_any(ctx, &c).is_some() {
                    "Unblock contact"
                } else {
                    "Block contact"
                });
            }
            v.extend(["Send as vCard", "Set as my card"]);
            v
        }
        ContactsView::Mark | ContactsView::Edit => Vec::new(),
    }
}

pub fn option(ctx: &Ctx, item: &str) {
    match item {
        "New contact" => edit(ctx, Who::New, ""),
        "Add to favourites" | "Remove from favourites" => {
            let Who::Contact(id) = ctx.m.borrow().people.card else {
                return;
            };
            if let Some(c) = ctx
                .m
                .borrow_mut()
                .book
                .contacts
                .iter_mut()
                .find(|c| c.id == id)
            {
                c.favourite = !c.favourite;
            }
            changed(ctx);
            save(ctx);
            ctx.show();
        }
        "Delete marked" if ctx.m.borrow().people.marked.is_empty() => ctx.note("Nothing selected"),
        "Delete marked" => ask(ctx, Dialog::DeleteMarked),
        "Select all" => {
            {
                let mut m = ctx.m.borrow_mut();
                m.people.marked = m.book.contacts.iter().map(|c| c.id).collect();
            }
            changed(ctx);
            ctx.show();
        }
        "Mark several" => {
            ctx.m.borrow_mut().people.marked.clear();
            changed(ctx);
            ctx.go(Page::Contacts(ContactsView::Mark));
        }
        "Import vCards" => import(ctx),
        "Export vCards" => {
            let all: Vec<Contact> = {
                let m = ctx.m.borrow();
                m.book
                    .my_card
                    .iter()
                    .chain(&m.book.contacts)
                    .cloned()
                    .collect()
            };
            export(ctx, &all);
        }
        // Phone's settings, at their Contacts section
        "Settings" => ctx.open(Page::Settings(SettingsView::Main), "contacts"),
        "Export" => {
            let marked: Vec<Contact> = {
                let m = ctx.m.borrow();
                let ids = &m.people.marked;
                m.book
                    .contacts
                    .iter()
                    .filter(|c| ids.contains(&c.id))
                    .cloned()
                    .collect()
            };
            export(ctx, &marked);
        }
        "Delete" => ask(ctx, Dialog::Delete),
        "Edit" => {
            let who = ctx.m.borrow().people.card;
            edit(ctx, who, "");
        }
        "Block contact" => ask(ctx, Dialog::Block),
        "Unblock contact" => unblock(ctx),
        "Speed dial" => {
            let numbers: Vec<String> = card(ctx)
                .map(|c| c.numbers.into_iter().map(|(_, n)| n).collect())
                .unwrap_or_default();
            match numbers.as_slice() {
                [] => {}
                // Phone's speed dial, with this number to put on a key
                [n] => ctx.open(Page::Phone(PhoneView::Speed), n),
                _ => {
                    ctx.m.borrow_mut().people.choice = Choice::Speed(numbers);
                    ctx.show();
                }
            }
        }
        "Send as vCard" => {
            if card(ctx).is_some() {
                ctx.m.borrow_mut().people.choice = Choice::Share;
                ctx.show();
            }
        }
        "Set as my card" => {
            if let Some(c) = card(ctx) {
                ctx.m.borrow_mut().book.set_my_card(Some(Contact {
                    favourite: false,
                    ..c
                }));
                changed(ctx);
                if save(ctx) {
                    ctx.note("Set as my card");
                }
                ctx.show();
            }
        }
        _ => {}
    }
}

/// The header's action (List: 🔍) or the editor's ✓.
pub fn action(ctx: &Ctx) {
    match view(ctx) {
        Some(ContactsView::List) => ctx.ui().global::<Nav>().set_searching(true),
        Some(ContactsView::Edit) => save_edit(ctx),
        _ => {}
    }
}

/// The header's search text changed ("" when it closed).
pub fn searched(ctx: &Ctx, query: &str) {
    ctx.m.borrow_mut().people.query = query.into();
    show_results(ctx);
}

pub fn back(ctx: &Ctx) -> bool {
    let (dialog, picking) = {
        let m = ctx.m.borrow();
        (m.people.dialog, m.people.picking)
    };
    if dialog != Dialog::None || picking.is_some() {
        cancel(ctx);
        ctx.m.borrow_mut().people.picking = None;
        ctx.show();
        return true;
    }
    match view(ctx) {
        // Back is Cancel: nothing is saved (asked first when something changed)
        Some(ContactsView::Edit) => {
            if dirty(ctx) {
                ask(ctx, Dialog::Discard);
            } else {
                leave_editor(ctx);
            }
            true
        }
        // opened from another app's page (Phone's call details): back there
        Some(ContactsView::Card) => {
            let from = ctx.m.borrow_mut().people.card_from.take();
            match from {
                Some(p) => {
                    ctx.back_to(p);
                    true
                }
                None => false,
            }
        }
        Some(ContactsView::List) => {
            let from = ctx.m.borrow_mut().people.list_from.take();
            match from {
                Some(p) => {
                    ctx.back_to(p);
                    true
                }
                None => false,
            }
        }
        Some(ContactsView::Mark) => {
            ctx.m.borrow_mut().people.marked.clear();
            changed(ctx);
            false
        }
        _ => false,
    }
}

pub fn wants_back(ctx: &Ctx) -> bool {
    let m = ctx.m.borrow();
    m.people.dialog != Dialog::None || m.people.picking.is_some()
}

/// Another app's OPEN (page `n`, `page`, `arg`) while a dialog or unsaved edits are open: held, and the Discard
/// query asks first; true when held.
pub fn hold_open(ctx: &Ctx, n: u16, page: Page, arg: &str) -> bool {
    let before = {
        let m = ctx.m.borrow();
        m.people.dialog
    };
    let busy = before != Dialog::None || ctx.m.borrow().people.picking.is_some() || dirty(ctx);
    if !busy {
        return false;
    }
    {
        let mut m = ctx.m.borrow_mut();
        m.people.pending = Some(Pending {
            n,
            page,
            arg: arg.into(),
            before,
        });
        m.people.dialog = Dialog::Discard;
        m.people.picking = None;
    }
    ctx.show();
    true
}

/// The header search's placeholder (p2-contacts-search).
pub fn search_hint(ctx: &Ctx) -> String {
    match ctx.m.borrow().book.contacts.len() {
        1 => String::from("Search 1 contact"),
        n => format!("Search {n} contacts"),
    }
}

/// Send MODEMD the contacts' known-number hashes (for "block unknown numbers").
pub fn send_known(ctx: &Ctx) {
    let mut m = ctx.m.borrow_mut();
    let keys = logic::known_keys(&m.book);
    let r = m.modem.set_known(&keys);
    if r != 0 && r != phone_common::modem::NOT_READY {
        dprintln!("neocont: known numbers not sent ({})", r);
    }
}

/// Another app opened Contacts on `number` (neoui ipc OPEN: Phone's call details, a message's sender): its card, or
/// `new` (or no contact has it) a new contact with it. Back (and Cancel) from there returns to that app.
pub fn opened_on(ctx: &Ctx, number: &str, new: bool) {
    // any page of another app: back_to() hands the user back to whoever opened us (DONE)
    let away = Page::Phone(PhoneView::Recents);
    let id = ctx.m.borrow().book.lookup(number).map(|c| c.id);
    match id {
        Some(id) if !new => open_card(ctx, Who::Contact(id), Some(away)),
        _ => {
            edit(ctx, Who::New, number);
            ctx.m.borrow_mut().people.edit_from = away;
        }
    }
}
