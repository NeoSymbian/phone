// SPDX-License-Identifier: GPL-3.0-only
//! The app's state and the moves every area shares: which page shows, the note, the page the UI server draws the
//! chrome for (neoui protocol 1.7: title, Back, the strip's buttons, the Options sheet, dialogs), the system
//! keyboard, drawing.
//!
//! Areas (one module each, same shape): [`crate::dial`] (Phone: dialer, Recents, details, speed dial),
//! [`crate::people`] (Contacts), [`crate::call`] (the call screens), [`crate::prefs`] (Settings, Blocked numbers),
//! [`crate::msgs`] (Messages).
//! Each has `State`, `bind` (its global's `act`), `show` (model → UI), `chrome`, `options`/`option`, `action`, `back`.

use crate::modem::Modem;
use crate::{call, dial, msgs, people, prefs};
use alloc::string::String;
use alloc::vec::Vec;
use contacts::{Book, NameOrder};
use core::cell::{Cell, RefCell};
use e32::dprintln;
use modem_ipc::{LogEntry, Status};
use neoapp::App;
use neoui_proto::ipc as proto;
use phone_core::page::{ContactsView, Page, PhoneView};
use phone_rules::block;
use phone_ui::{KeyboardFocus, Nav, PhoneApp};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

/// A dialog for the UI server: `text` (and `detail` on its own line, if it fits), a red `confirm` and Cancel.
pub fn ask(text: &str, detail: &str, confirm: &str) -> neoapp::Query {
    let mut t = String::from(text);
    if !detail.is_empty() && t.len() + 1 + detail.len() <= neoui_proto::page::TEXT_MAX {
        t.push('\n');
        t.push_str(detail);
    }
    while t.len() > neoui_proto::page::TEXT_MAX {
        t.pop();
    }
    neoapp::Query {
        text: t,
        buttons: alloc::vec![confirm.into(), "Cancel".into()],
        destructive: true,
    }
}

/// How long a note stays.
pub const NOTE_MS: u64 = 3000;

/// A view's title and primary action, as each area describes it (made into the UI server's page by [`Ctx::sync`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Chrome {
    pub title: String,
    /// `proto::TOOL_*`
    pub icon: i32,
    /// `proto::TOOLBAR_*` bits: `TOOLBAR_CANCEL` a form (the strip's Cancel), `TOOLBAR_EMPTY` no action
    pub style: i32,
    pub label: String,
}

impl Chrome {
    pub fn titled(title: &str) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }
    pub fn with(mut self, icon: i32, style: i32, label: &str) -> Self {
        self.icon = icon;
        self.style = style;
        self.label = label.into();
        self
    }
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
    pub people: people::State,
    pub call: call::State,
    pub prefs: prefs::State,
    pub msgs: msgs::State,
    pub note_until: u64,
    pub visible: bool,
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

pub fn model<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    ModelRc::from(alloc::rc::Rc::new(VecModel::from(items)))
}

pub fn strings(items: &[&str]) -> ModelRc<SharedString> {
    model(items.iter().map(|s| SharedString::from(*s)).collect())
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

    /// Show `page` (and everything on it).
    pub fn go(&self, page: Page) {
        {
            let mut m = self.m.borrow_mut();
            if page == Page::Call && m.page != Page::Call {
                m.before_call = m.page;
            }
            m.page = page;
        }
        let ui = self.ui();
        let nav = ui.global::<Nav>();
        let (p, v) = page.nav();
        nav.set_page(p);
        nav.set_view(v.into());
        dprintln!("neophone: page {} view {}", p, v);
        self.show();
    }

    /// Back inside the app: the area's own step (closing a search, a dialog), else the page's parent. False when Back
    /// leaves the app (it is then not claimed, and the UI server goes home).
    pub fn back(&self) -> bool {
        let handled = match self.page() {
            Page::Phone(_) => dial::back(self),
            Page::Contacts(_) => people::back(self),
            Page::Call => call::back(self),
            Page::Settings(_) => prefs::back(self),
            Page::Messages(_) => msgs::back(self),
        };
        if handled {
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
            Page::Phone(_) => dial::options(self),
            Page::Contacts(_) => people::options(self),
            Page::Call => call::options(self),
            Page::Settings(_) => prefs::options(self),
            Page::Messages(_) => msgs::options(self),
        }
    }

    /// Item `i` of the sheet the UI server showed.
    pub fn option_at(&self, i: u16) {
        let item = self
            .shown
            .borrow()
            .as_ref()
            .and_then(|p| p.options.get(usize::from(i)).cloned());
        if let Some(item) = item {
            self.option(&item);
        }
    }

    pub fn option(&self, item: &str) {
        match self.page() {
            Page::Phone(_) => dial::option(self, item),
            Page::Contacts(_) => people::option(self, item),
            Page::Call => call::option(self, item),
            Page::Settings(_) => prefs::option(self, item),
            Page::Messages(_) => msgs::option(self, item),
        }
    }

    /// The UI server's dialog was answered: `yes` is its first (confirming) button.
    pub fn answer(&self, yes: bool) {
        *self.asked.borrow_mut() = None; // the UI server closed it
        match self.page() {
            Page::Contacts(_) => people::answer(self, yes),
            Page::Settings(_) => prefs::answer(self, yes),
            Page::Messages(_) => msgs::answer(self, yes),
            Page::Phone(_) | Page::Call => {}
        }
    }

    /// The strip's primary action.
    pub fn action(&self) {
        match self.page() {
            Page::Phone(_) => dial::action(self),
            Page::Contacts(_) => people::action(self),
            Page::Call => call::action(self),
            Page::Settings(_) => prefs::action(self),
            Page::Messages(_) => msgs::action(self),
        }
    }

    /// Model → UI for the page showing (each area fills its global).
    pub fn show(&self) {
        match self.page() {
            Page::Phone(_) => dial::show(self),
            Page::Contacts(_) => people::show(self),
            Page::Call => call::show(self),
            Page::Settings(_) => prefs::show(self),
            Page::Messages(_) => msgs::show(self),
        }
    }

    /// A note over the page for [`NOTE_MS`].
    pub fn note(&self, text: &str) {
        self.m.borrow_mut().note_until = e32::time::millis() + NOTE_MS;
        self.ui().global::<Nav>().set_note(text.into());
    }

    /// The UI server's page and the keyboard as the view needs them; then draw.
    pub fn sync(&self) -> Result<(), i32> {
        let now = e32::time::millis();
        let ui = self.ui();
        let nav = ui.global::<Nav>();
        if !nav.get_note().is_empty() && now >= self.m.borrow().note_until {
            nav.set_note("".into());
        }
        let page = self.page_now();
        if self.shown.borrow().as_ref() != Some(&page) {
            if let Err(e) = self.app.page(&page) {
                dprintln!("neophone: page not set ({})", e);
            }
            *self.shown.borrow_mut() = Some(page);
        }
        let query = match self.page() {
            Page::Contacts(_) => people::query(self),
            Page::Settings(_) => prefs::query(self),
            Page::Messages(_) => msgs::query(self),
            Page::Phone(_) | Page::Call => None,
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

    /// What the UI server draws around the current view: its title; Back stays in the app while there is somewhere to
    /// go back to (or something Back closes); the primary action; Cancel on a form; the Options items; the call
    /// screens full screen; the Phone | Contacts dots.
    fn page_now(&self) -> neoapp::Page {
        let chrome = match self.page() {
            Page::Phone(_) => dial::chrome(self),
            Page::Contacts(_) => people::chrome(self),
            Page::Call => call::chrome(self),
            Page::Settings(_) => prefs::chrome(self),
            Page::Messages(_) => msgs::chrome(self),
        };
        let empty = chrome.style & proto::TOOLBAR_EMPTY != 0;
        neoapp::Page {
            primary: (chrome.icon != proto::TOOL_NONE && !empty).then_some(neoapp::Action {
                icon: chrome.icon,
                label: chrome.label,
            }),
            title: chrome.title,
            can_back: self.page().back().is_some() || self.wants_back(),
            cancel: chrome.style & proto::TOOLBAR_CANCEL != 0,
            options: self.options().into_iter().map(String::from).collect(),
            full: self.page() == Page::Call,
            dots: match self.page() {
                Page::Phone(PhoneView::Dial) => Some((2, 0)),
                Page::Contacts(ContactsView::List) => Some((2, 1)),
                _ => None,
            },
        }
    }

    /// An area has something Back closes on a root view (a search, a dialog).
    fn wants_back(&self) -> bool {
        match self.page() {
            Page::Phone(_) => dial::wants_back(self),
            Page::Contacts(_) => people::wants_back(self),
            Page::Call => false,
            Page::Settings(_) => prefs::wants_back(self),
            Page::Messages(_) => msgs::wants_back(self),
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
        msgs::on_status(self);
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
        let mut at = [call::next_wake(self), dial::next_wake(self)]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(now + 60_000);
        let note = self.m.borrow().note_until;
        if note > now {
            at = at.min(note);
        }
        i32::try_from(at.saturating_sub(now).max(20) * 1000).unwrap_or(i32::MAX)
    }
}
