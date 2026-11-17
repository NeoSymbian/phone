// SPDX-License-Identifier: GPL-3.0-only
//! Phone's state and the moves every area shares: which page shows (another app's page opens that app: neoui ipc
//! OPEN), the note, the header and footer the app draws and the page the UI server knows (protocol 1.9: name, Back,
//! full screen, dialogs), the system keyboard, drawing.
//!
//! Areas (one module each, same shape): [`crate::dial`] (Recents, the keypad sheet, details, speed dial),
//! [`crate::call`] (the call screens), [`crate::prefs`] (Settings, Blocked numbers, Quick replies). Each has `State`, `bind` (its global's `act`), `show`
//! (model → UI), `chrome`, `options`/`option`, `action`, `back`. Contacts and Messages are apps of their own (NEOCONT,
//! NEOMSG): [`Ctx::contact`], [`Ctx::message`] open them.

use crate::{call, dial, prefs};
use alloc::string::String;
use alloc::vec::Vec;
use contacts::{Book, NameOrder};
use core::cell::{Cell, RefCell};
use e32::dprintln;
use modem_ipc::{LogEntry, SmsSettings, Status};
use neoapp::App;
use neoui_proto::ipc as proto;
use phone_common::modem::Modem;
use phone_common::ui::{Chrome, strings, tool};
use phone_core::page::{Page, PhoneView, SettingsView, app, number};
use phone_rules::block;
use phone_ui::{Icons, KeyboardFocus, Kit, Nav, PhoneApp};
use slint::{ComponentHandle, Model as _, SharedString};

/// What Phone keeps of Messages: the quick replies (Reply with message, Settings) and MODEMD's message settings
/// (Settings ▸ Messages).
#[derive(Default)]
pub struct Sms {
    pub replies: Vec<String>,
    pub settings: SmsSettings,
}

pub struct Model {
    pub page: Page,
    /// where the app returns when the call screens close
    pub before_call: Page,
    pub modem: Modem,
    /// MODEMD's latest status (None: no MODEMD yet)
    pub status: Option<Status>,
    pub book: Book,
    pub order: NameOrder,
    /// the call log, newest first, and the `Status::log_revision` it was read at
    pub log: Vec<LogEntry>,
    pub log_revision: Option<u32>,
    /// the blocklist as MODEMD has it
    pub block: block::List,
    pub dial: dial::State,
    pub call: call::State,
    pub prefs: prefs::State,
    pub msgs: Sms,
    pub visible: bool,
    /// the Return to call bar's clock
    pub call_bar: phone_common::CallBar,
    /// another app opened the call screens (OPEN): when the call ends, the user goes back there (DONE)
    pub call_opened: bool,
    /// Contacts opened speed dial to put a number on a key: leaving it goes back there (DONE)
    pub speed_opened: bool,
    /// another app opened Settings (OPEN): Back from its top goes back there (DONE)
    pub settings_opened: bool,
}

pub struct Ctx {
    pub app: App,
    pub ui: slint::Weak<PhoneApp>,
    pub m: RefCell<Model>,
    /// the page the UI server shows for us now (sent only when it changes)
    shown: RefCell<Option<neoapp::Page>>,
    /// the dialog the UI server shows for us now
    asked: RefCell<Option<neoapp::Query>>,
    /// the keyboard last asked of the UI server: (open, KeyboardFocus.seq, mode)
    keyboard: Cell<(bool, i32, i32)>,
}

impl Ctx {
    pub fn new(app: App, ui: &PhoneApp, m: Model) -> Self {
        Self {
            app,
            ui: ui.as_weak(),
            m: RefCell::new(m),
            shown: RefCell::new(None),
            asked: RefCell::new(None),
            keyboard: Cell::new((false, 0, 0)),
        }
    }

    pub fn ui(&self) -> PhoneApp {
        self.ui
            .upgrade()
            .expect("the window lives as long as the app")
    }

    pub fn page(&self) -> Page {
        self.m.borrow().page
    }

    /// Show `page` (and everything on it); another app's page opens that app (Contacts, Messages).
    pub fn go(&self, page: Page) {
        if page.app() != app::PHONE {
            return self.open(page, "");
        }
        let done = {
            let mut m = self.m.borrow_mut();
            if page == Page::Call && m.page != Page::Call {
                m.before_call = m.page;
            }
            let speed = Page::Phone(PhoneView::Speed);
            let done = (m.page == Page::Call && page != Page::Call && m.call_opened)
                || (m.page == speed && page != speed && m.speed_opened);
            if done {
                m.call_opened = false;
                m.speed_opened = false;
            }
            m.page = page;
            done
        };
        let ui = self.ui();
        let nav = ui.global::<Nav>();
        let (p, v) = page.nav();
        nav.set_page(p);
        nav.set_view(v.into());
        dprintln!("neophone: page {} view {}", p, v);
        self.show();
        // the call another app started ended: back to that app
        if done && let Err(e) = self.app.done() {
            dprintln!("neophone: done not sent ({})", e);
        }
    }

    /// Open another app's `page` with `arg` (neoui ipc OPEN).
    pub fn open(&self, page: Page, arg: &str) {
        if let Err(e) = phone_common::open(&self.app, page, arg) {
            dprintln!("neophone: {} not opened ({})", page.app(), e);
        }
    }

    /// Contacts on `number`'s card (a new contact with it when none matches).
    pub fn contact(&self, number: &str) {
        let _ = phone_common::open_number(&self.app, app::CONTACTS, number::contacts::CARD, number);
    }

    /// A new contact with `number` in Contacts.
    pub fn new_contact(&self, number: &str) {
        let _ = phone_common::open_number(&self.app, app::CONTACTS, number::contacts::NEW, number);
    }

    /// The conversation with `number` in Messages; `write`: the composer takes the focus.
    pub fn message(&self, number: &str, write: bool) {
        let page = if write {
            number::messages::WRITE
        } else {
            number::messages::THREAD
        };
        let _ = phone_common::open_number(&self.app, app::MESSAGES, page, number);
    }

    /// The name `number` shows under (exactly one contact matches).
    pub fn name_of(&self, number: &str) -> Option<String> {
        let m = self.m.borrow();
        phone_common::name_of(&m.book, m.order, number)
    }

    /// Back inside the app: the area's own step (closing a search, a dialog), else the page's parent. False when Back
    /// leaves the app (it is then not claimed, and the UI server goes home).
    pub fn back(&self) -> bool {
        let handled = match self.page() {
            // another app's pages (go() opens them there)
            Page::Contacts(_) | Page::Messages(_) => false,
            Page::Phone(_) => dial::back(self),
            Page::Call => call::back(self),
            Page::Settings(_) => prefs::back(self),
        };
        if handled {
            return true;
        }
        // Settings opened from Messages or Contacts: Back returns there, and Phone is at its root next time
        if self.page() == Page::Settings(SettingsView::Main) && self.m.borrow().settings_opened {
            self.m.borrow_mut().settings_opened = false;
            self.go(Page::Phone(PhoneView::Recents));
            if let Err(e) = self.app.done() {
                dprintln!("neophone: done not sent ({})", e);
            }
            return true;
        }
        match self.page().back() {
            Some(p) => {
                self.go(p);
                true
            }
            None => false,
        }
    }

    /// The current view's Options items (the UI server's sheet).
    fn options(&self) -> Vec<&'static str> {
        match self.page() {
            // another app's pages (go() opens them there)
            Page::Contacts(_) | Page::Messages(_) => Vec::new(),
            Page::Phone(_) => dial::options(self),
            Page::Call => call::options(self),
            Page::Settings(_) => prefs::options(self),
        }
    }

    /// Item `i` of the header's ⋮ menu.
    pub fn option_at(&self, i: usize) {
        if let Some(item) = self.options().get(i) {
            self.option(item);
        }
    }

    pub fn option(&self, item: &str) {
        match self.page() {
            // another app's pages (go() opens them there)
            Page::Contacts(_) | Page::Messages(_) => {}
            Page::Phone(_) => dial::option(self, item),
            Page::Call => call::option(self, item),
            Page::Settings(_) => prefs::option(self, item),
        }
    }

    /// The header's icon `i` (the action, then `Chrome::more`).
    pub fn header_action(&self, i: usize) {
        match self.chrome().header_action(i) {
            Some(None) => self.action(),
            Some(Some(name)) => self.option(name),
            None => {}
        }
    }

    /// The selection header's action `i` (`Chrome::selection`).
    pub fn selection_action(&self, i: usize) {
        let name = self.chrome().selection.get(i).map(|(_, n)| *n);
        if let Some(name) = name {
            self.option(name);
        }
    }

    /// Item `i` of the selection header's ⋮ (`Chrome::selection_menu`).
    pub fn selection_option(&self, i: usize) {
        let name = self.chrome().selection_menu.get(i).copied();
        if let Some(name) = name {
            self.option(name);
        }
    }

    /// The header's search field changed (each keystroke; "" when it closed).
    pub fn searched(&self, query: &str) {
        if let Page::Phone(_) = self.page() {
            dial::searched(self, query);
        }
    }

    /// The UI server's dialog was answered: `yes` is its first (confirming) button.
    pub fn answer(&self, yes: bool) {
        *self.asked.borrow_mut() = None; // the UI server closed it
        if let Page::Settings(_) = self.page() {
            prefs::answer(self, yes);
        }
    }

    /// The header's action, a form's ✓, or the big action in the content.
    pub fn action(&self) {
        match self.page() {
            // another app's pages (go() opens them there)
            Page::Contacts(_) | Page::Messages(_) => {}
            Page::Phone(_) => dial::action(self),
            Page::Call => call::action(self),
            Page::Settings(_) => prefs::action(self),
        }
    }

    /// Model → UI for the page showing (each area fills its global).
    pub fn show(&self) {
        match self.page() {
            // another app's pages (go() opens them there)
            Page::Contacts(_) | Page::Messages(_) => {}
            Page::Phone(_) => dial::show(self),
            Page::Call => call::show(self),
            Page::Settings(_) => prefs::show(self),
        }
    }

    /// A toast for 3 s (the shell's NOTE, above the key row or band).
    pub fn note(&self, text: &str) {
        if let Err(e) = self.app.note(&phone_common::ui::note_text(text), "") {
            dprintln!("neophone: note not shown ({})", e);
        }
    }

    /// The header, the UI server's page and the keyboard as the view needs them; then draw.
    pub fn sync(&self) -> Result<(), i32> {
        let ui = self.ui();
        self.header(&ui);
        let page = self.page_now(&ui);
        if self.shown.borrow().as_ref() != Some(&page) {
            if let Err(e) = self.app.page(&page) {
                dprintln!("neophone: page not set ({})", e);
            }
            *self.shown.borrow_mut() = Some(page);
        }
        let query = match self.page() {
            Page::Settings(_) => prefs::query(self),
            _ => None,
        };
        if *self.asked.borrow() != query {
            let r = match &query {
                Some(q) => self.app.query(q),
                None => self.app.close_query(),
            };
            if let Err(e) = r {
                dprintln!("neophone: dialog not shown ({})", e);
            }
            *self.asked.borrow_mut() = query;
        }
        // a field took the focus (or was tapped after the keyboard was put away): ask for its keyboard (letters,
        // number pad, phone pad); none focused: hide it
        let kf = ui.global::<KeyboardFocus>();
        let want = (kf.get_open(), kf.get_seq(), kf.get_mode());
        let had = self.keyboard.get();
        if want.0 != had.0 || (want.0 && (want.1, want.2) != (had.1, had.2)) {
            self.keyboard.set(want);
            let mode = if want.0 {
                want.2.max(1)
            } else {
                proto::KEYBOARD_HIDDEN
            };
            // how much of the surface it covers: Messages keeps its composer above it
            let h = self.app.keyboard(mode).unwrap_or(0);
            kf.set_text_h(h as f32);
        }
        self.app.draw()
    }

    /// The view's kit header (Nav), from its [`Chrome`]: the title; ← on a page with somewhere to go up to; a form's
    /// [✕ title ✓]; the action as an icon, or as a pill in the content; ⋮; the Phone | Contacts dots.
    fn header(&self, ui: &PhoneApp) {
        let chrome = self.chrome();
        let nav = ui.global::<Nav>();
        let empty = chrome.style & tool::EMPTY != 0;
        nav.set_title(chrome.title.as_str().into());
        // Return to call: a call goes on and its screen isn't showing
        let bar = {
            let mut m = self.m.borrow_mut();
            let m = &mut *m;
            if m.page != Page::Call {
                m.call_bar
                    .text(m.status.as_ref(), &m.book, m.order, e32::time::millis())
            } else {
                String::new()
            }
        };
        nav.set_call_bar(bar.as_str().into());

        nav.set_title_font(phone_common::ui::title_font(&chrome.title).into());
        // the keypad sheet is a layer over Recents, not a page: no ←
        nav.set_sub(self.page().back().is_some() && self.page() != Page::Phone(PhoneView::Dial));
        nav.set_form(chrome.style & tool::CANCEL != 0);
        nav.set_has_action(chrome.icon != tool::NONE && !empty);
        nav.set_action_tool(chrome.icon);
        nav.set_action_label(chrome.label.as_str().into());
        nav.set_pill(chrome.style & tool::PILL != 0);
        nav.set_full(self.page() == Page::Call);
        // the footer (Phone · Contacts · Messages) on the root view
        nav.set_footer(
            self.page() == Page::Phone(PhoneView::Recents)
                && chrome.selected == 0
                && !chrome.selecting
                && !nav.get_searching(),
        );
        let (missed, unread) = {
            let m = self.m.borrow();
            (
                phone_core::recents::new_missed(&m.log, m.prefs.missed_seen),
                m.status.map_or(0, |s| u32::from(s.sms_unread)),
            )
        };
        nav.set_missed(missed as i32);
        nav.set_unread(i32::from(unread as u16));
        // the header's icons (the action first), its search and its selection mode (kit Header)
        let icons = ui.global::<Icons>();
        let mut acts = Vec::new();
        if chrome.icon_action() {
            acts.push(icons.invoke_act(chrome.icon, chrome.label.as_str().into()));
        }
        for (t, n) in &chrome.more {
            acts.push(icons.invoke_act(*t, (*n).into()));
        }
        if nav.get_actions().iter().ne(acts.iter().cloned()) {
            nav.set_actions(phone_common::ui::model(acts));
        }
        nav.set_search(chrome.search.is_some());
        nav.set_search_hint(chrome.search.unwrap_or("Search").into());
        nav.set_selected(chrome.selected);
        nav.set_selecting(chrome.selecting);
        let sel_menu: Vec<&str> = chrome.selection_menu.clone();
        if nav
            .get_selection_menu()
            .iter()
            .ne(sel_menu.iter().map(|o| SharedString::from(*o)))
        {
            nav.set_selection_menu(strings(&sel_menu));
        }
        let sel: Vec<_> = chrome
            .selection
            .iter()
            .map(|(t, n)| icons.invoke_act(*t, (*n).into()))
            .collect();
        if nav.get_selection_actions().iter().ne(sel.iter().cloned()) {
            nav.set_selection_actions(phone_common::ui::model(sel));
        }
        let options = self.options();
        if nav
            .get_options()
            .iter()
            .ne(options.iter().map(|o| SharedString::from(*o)))
        {
            nav.set_options(strings(&options));
        }
    }

    fn chrome(&self) -> Chrome {
        match self.page() {
            // another app's pages (go() opens them there)
            Page::Contacts(_) | Page::Messages(_) => Chrome::default(),
            Page::Phone(_) => dial::chrome(self),
            Page::Call => call::chrome(self),
            Page::Settings(_) => prefs::chrome(self),
        }
    }

    /// What the UI server needs of the current view (protocol 1.8): its name; Back stays in the app while there is
    /// somewhere to go back to, something Back closes, or a kit layer (the ⋮ menu) open; the call screens full
    /// screen.
    fn page_now(&self, ui: &PhoneApp) -> neoapp::Page {
        neoapp::Page {
            title: self.chrome().title,
            can_back: self.page().back().is_some()
                || self.wants_back()
                || ui.global::<Kit>().get_layers() > 0,
            full: self.page() == Page::Call,
        }
    }

    /// An area has something Back closes on a root view (a search, a dialog).
    fn wants_back(&self) -> bool {
        match self.page() {
            // another app's pages (go() opens them there)
            Page::Contacts(_) | Page::Messages(_) => false,
            Page::Phone(_) => dial::wants_back(self),
            // Back hides the call screen (Return to call brings it back)
            Page::Call => true,
            Page::Settings(_) => prefs::wants_back(self),
        }
    }

    /// MODEMD sent a status: the call screens open or close, Recents re-reads the log.
    pub fn on_status(&self, status: Option<Status>) {
        let Some(s) = status else { return };
        let log_changed = {
            let mut m = self.m.borrow_mut();
            m.status = Some(s);
            m.log_revision != Some(s.log_revision)
        };
        if log_changed {
            let log = self.m.borrow_mut().modem.log();
            match log {
                Ok(log) => {
                    let mut m = self.m.borrow_mut();
                    m.log = log;
                    m.log_revision = Some(s.log_revision);
                }
                Err(e) => dprintln!("neophone: call log not read ({})", e),
            }
        }
        call::on_status(self);
        prefs::on_status(self);
        self.show();
    }

    /// Time-driven updates (the call timer, notes, DTMF pauses); the next wake in µs.
    pub fn tick(&self) {
        call::tick(self);
        dial::tick(self);
    }

    pub fn next_wake_us(&self) -> i32 {
        let now = e32::time::millis();
        // the Return to call bar's clock, every second
        let bar = self.m.borrow().call_bar.live().then_some(now + 1000);
        let at = [call::next_wake(self), dial::next_wake(self), bar]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(now + 60_000);
        let own = i32::try_from(at.saturating_sub(now).max(20) * 1000).unwrap_or(i32::MAX);
        phone_common::wait_us(&self.app, own)
    }
}

/// Load the quick replies and MODEMD's message settings (start, and Settings).
pub fn load_sms(ctx: &Ctx) {
    let replies = phone_common::store::replies();
    let settings = ctx.m.borrow_mut().modem.sms_settings();
    let mut m = ctx.m.borrow_mut();
    m.msgs.replies = replies;
    if let Ok(s) = settings {
        m.msgs.settings = s;
    }
}
