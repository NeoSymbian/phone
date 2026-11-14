// SPDX-License-Identifier: GPL-3.0-only
//! The app's state and the moves every area shares: which page shows, the note, the title and toolbar button
//! (neoui ipc 1.4), the claimed Back/Options, the system keyboard, drawing.
//!
//! Areas (one module each, same shape): [`crate::dial`] (Phone: dialer, Recents, details, speed dial),
//! [`crate::people`] (Contacts), [`crate::call`] (the call screens), [`crate::prefs`] (Settings, Blocked numbers).
//! Each has `State`, `bind` (its global's `act`), `show` (model → UI), `chrome`, `options`/`option`, `action`, `back`.

use crate::modem::Modem;
use crate::{call, dial, people, prefs};
use alloc::string::String;
use alloc::vec::Vec;
use contacts::{Book, NameOrder};
use core::cell::{Cell, RefCell};
use e32::dprintln;
use modem_ipc::{LogEntry, Status};
use neoapp::App;
use neoui_proto::ipc as proto;
use phone_core::page::Page;
use phone_rules::block;
use phone_ui::{KeyboardFocus, Nav, PhoneApp};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

/// How long a note stays.
pub const NOTE_MS: u64 = 3000;

/// The UI server's title and the toolbar's middle button for the current view.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Chrome {
    pub title: String,
    /// `proto::TOOL_*`
    pub icon: i32,
    /// `proto::TOOLBAR_*` bits
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
    pub note_until: u64,
    pub visible: bool,
}

pub struct Ctx {
    pub app: App,
    pub ui: slint::Weak<PhoneApp>,
    pub m: RefCell<Model>,
    /// what the UI server shows for us now (sent only when it changes)
    chrome: RefCell<Option<Chrome>>,
    nav_claim: Cell<i32>,
    keyboard: Cell<bool>,
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
            chrome: RefCell::new(None),
            nav_claim: Cell::new(-1),
            keyboard: Cell::new(false),
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
        nav.set_options_open(false);
        dprintln!("neophone: page {} view {}", p, v);
        self.show();
    }

    /// Back inside the app: the area's own step (closing a search, a dialog), else the page's parent. False when Back
    /// leaves the app (it is then not claimed, and the UI server goes home).
    pub fn back(&self) -> bool {
        let ui = self.ui();
        if ui.global::<Nav>().get_options_open() {
            ui.global::<Nav>().set_options_open(false);
            return true;
        }
        let handled = match self.page() {
            Page::Phone(_) => dial::back(self),
            Page::Contacts(_) => people::back(self),
            Page::Call => call::back(self),
            Page::Settings(_) => prefs::back(self),
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

    /// Options: open (or close) the sheet with the current view's items.
    pub fn options(&self) {
        let items = match self.page() {
            Page::Phone(_) => dial::options(self),
            Page::Contacts(_) => people::options(self),
            Page::Call => call::options(self),
            Page::Settings(_) => prefs::options(self),
        };
        let ui = self.ui();
        let nav = ui.global::<Nav>();
        let open = !nav.get_options_open() && !items.is_empty();
        nav.set_options(strings(&items));
        nav.set_options_open(open);
    }

    pub fn option(&self, item: &str) {
        self.ui().global::<Nav>().set_options_open(false);
        match self.page() {
            Page::Phone(_) => dial::option(self, item),
            Page::Contacts(_) => people::option(self, item),
            Page::Call => call::option(self, item),
            Page::Settings(_) => prefs::option(self, item),
        }
    }

    /// The toolbar's middle button.
    pub fn action(&self) {
        match self.page() {
            Page::Phone(_) => dial::action(self),
            Page::Contacts(_) => people::action(self),
            Page::Call => call::action(self),
            Page::Settings(_) => prefs::action(self),
        }
    }

    /// Model → UI for the page showing (each area fills its global).
    pub fn show(&self) {
        match self.page() {
            Page::Phone(_) => dial::show(self),
            Page::Contacts(_) => people::show(self),
            Page::Call => call::show(self),
            Page::Settings(_) => prefs::show(self),
        }
    }

    /// A note over the page for [`NOTE_MS`].
    pub fn note(&self, text: &str) {
        self.m.borrow_mut().note_until = e32::time::millis() + NOTE_MS;
        self.ui().global::<Nav>().set_note(text.into());
    }

    /// The UI server's chrome, the claimed buttons and the keyboard as the view needs them; then draw.
    pub fn sync(&self) -> Result<(), i32> {
        let now = e32::time::millis();
        let ui = self.ui();
        let nav = ui.global::<Nav>();
        if !nav.get_note().is_empty() && now >= self.m.borrow().note_until {
            nav.set_note("".into());
        }
        let chrome = match self.page() {
            Page::Phone(_) => dial::chrome(self),
            Page::Contacts(_) => people::chrome(self),
            Page::Call => call::chrome(self),
            Page::Settings(_) => prefs::chrome(self),
        };
        if self.chrome.borrow().as_ref() != Some(&chrome) {
            if let Err(e) = self.app.title(&chrome.title) {
                dprintln!("neophone: title not set ({})", e);
            }
            if let Err(e) = self.app.toolbar(chrome.icon, chrome.style, &chrome.label) {
                dprintln!("neophone: toolbar not set ({})", e);
            }
            *self.chrome.borrow_mut() = Some(chrome);
        }
        // Back is ours while there is somewhere to go back to inside the app (or a sheet to close)
        let back = nav.get_options_open() || self.page().back().is_some() || self.wants_back();
        let claim = i32::from(back) | 2;
        if self.nav_claim.get() != claim {
            self.nav_claim.set(claim);
            let _ = self.app.claim_nav(back, true);
        }
        let kb = ui.global::<KeyboardFocus>().get_open();
        if self.keyboard.get() != kb {
            self.keyboard.set(kb);
            let mode = if kb {
                proto::KEYBOARD_TEXT
            } else {
                proto::KEYBOARD_HIDDEN
            };
            let _ = self.app.keyboard(mode);
        }
        self.app.draw()
    }

    /// An area has something Back closes on a root view (a search, a dialog).
    fn wants_back(&self) -> bool {
        match self.page() {
            Page::Phone(_) => dial::wants_back(self),
            Page::Contacts(_) => people::wants_back(self),
            Page::Call => false,
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
