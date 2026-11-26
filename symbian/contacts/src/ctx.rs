// SPDX-License-Identifier: GPL-3.0-only
//! Contacts's state and the moves its area shares with the frame: which page shows (another app's page opens that app:
//! neoui ipc OPEN), the note, the header and footer the app draws and the page the UI server knows (protocol 1.9:
//! name, Back, dialogs), the system keyboard, drawing. The area is [`crate::people`] (`State`, `bind`, `show`,
//! `chrome`, `options`/`option`, `action`, `back`, `query`/`answer`).

use crate::people;
use alloc::string::String;
use alloc::vec::Vec;
use contacts::{Book, NameOrder};
use contacts_ui::{ContactsApp, Icons, KeyboardFocus, Kit, Nav};
use core::cell::{Cell, RefCell};
use e32::dprintln;
use modem_ipc::{LogEntry, Status};
use neoapp::App;
use neoui_proto::ipc as proto;
use phone_common::modem::Modem;
use phone_common::ui::{Chrome, strings, tool};
use phone_core::page::{ContactsView, Page, app, number};
use phone_rules::block;
use slint::{ComponentHandle, Model as _, SharedString};

pub struct Model {
    pub page: Page,
    pub modem: Modem,
    /// MODEMD's latest status (None: no MODEMD yet)
    pub status: Option<Status>,
    pub book: Book,
    pub order: NameOrder,
    /// the call log (for the footer's missed calls), newest first, and the `Status::log_revision` it was read at
    pub log: Vec<LogEntry>,
    pub log_revision: Option<u32>,
    /// the blocklist as MODEMD has it
    pub block: block::List,
    pub prefs: phone_common::prefs::State,
    pub people: people::State,
    pub visible: bool,
    /// the Return to call bar's clock
    pub call_bar: phone_common::CallBar,
}

pub struct Ctx {
    pub app: App,
    pub ui: slint::Weak<ContactsApp>,
    pub m: RefCell<Model>,
    /// the page the UI server shows for us now (sent only when it changes)
    shown: RefCell<Option<neoapp::Page>>,
    /// the dialog the UI server shows for us now
    asked: RefCell<Option<neoapp::Query>>,
    /// the keyboard last asked of the UI server: (open, KeyboardFocus.seq, mode)
    keyboard: Cell<(bool, i32, i32)>,
}

impl Ctx {
    pub fn new(app: App, ui: &ContactsApp, m: Model) -> Self {
        Self {
            app,
            ui: ui.as_weak(),
            m: RefCell::new(m),
            shown: RefCell::new(None),
            asked: RefCell::new(None),
            keyboard: Cell::new((false, 0, 0)),
        }
    }

    pub fn ui(&self) -> ContactsApp {
        self.ui
            .upgrade()
            .expect("the window lives as long as the app")
    }

    pub fn page(&self) -> Page {
        self.m.borrow().page
    }

    /// Show `page` (and everything on it); another app's page opens that app.
    pub fn go(&self, page: Page) {
        if page.app() != app::CONTACTS {
            return self.open(page, "");
        }
        self.m.borrow_mut().page = page;
        let ui = self.ui();
        let nav = ui.global::<Nav>();
        let (p, v) = page.nav();
        nav.set_page(p);
        nav.set_view(v.into());
        dprintln!("neocont: page {} view {}", p, v);
        self.show();
    }

    /// Back from a page another app opened us on (`from` is a page of that app): back to it (DONE), and this app is
    /// on its list again next time; a page of ours: go there.
    pub fn back_to(&self, from: Page) {
        if from.app() == app::CONTACTS {
            return self.go(from);
        }
        self.go(Page::Contacts(ContactsView::List));
        if let Err(e) = self.app.done() {
            dprintln!("neocont: done not sent ({})", e);
        }
    }

    /// Open another app's `page` with `arg` (neoui ipc OPEN).
    pub fn open(&self, page: Page, arg: &str) {
        if let Err(e) = phone_common::open(&self.app, page, arg) {
            dprintln!("neocont: {} not opened ({})", page.app(), e);
        }
    }

    /// Call `number` (Phone dials it and shows the call; when it ends, the user is back here).
    pub fn call(&self, number: &str) {
        let _ = phone_common::open_number(&self.app, app::PHONE, number::phone::CALL, number);
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

    /// Show a blocklist edit's note (phone_common::block); true when it was saved.
    pub fn noted(&self, r: Result<String, String>) -> bool {
        let (ok, n) = match r {
            Ok(n) => (true, n),
            Err(n) => (false, n),
        };
        if !n.is_empty() {
            self.note(&n);
        }
        ok
    }

    /// Block `number` (calls and/or messages) in MODEMD's blocklist, with a note.
    pub fn block_number(&self, number: &str, calls: bool, messages: bool) {
        let r = {
            let mut m = self.m.borrow_mut();
            let m = &mut *m;
            phone_common::block::block_number(&mut m.modem, &mut m.block, number, calls, messages)
        };
        self.noted(r);
    }

    /// Remove every rule matching `number` exactly, with a note.
    pub fn unblock_number(&self, number: &str) {
        let r = {
            let mut m = self.m.borrow_mut();
            let m = &mut *m;
            phone_common::block::unblock_number(&mut m.modem, &mut m.block, number)
        };
        self.noted(r);
    }

    /// What blocks `number` (calls, messages), if a number rule does.
    pub fn blocked(&self, number: &str) -> Option<(bool, bool)> {
        phone_common::block::blocked(&self.m.borrow().block, number)
    }

    /// Back inside the app: the area's own step (closing a search, a dialog), else the page's parent. False when Back
    /// leaves the app (it is then not claimed: the UI server goes to the app that opened it, or home).
    pub fn back(&self) -> bool {
        if people::back(self) {
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

    /// Item `i` of the header's ⋮ menu.
    pub fn option_at(&self, i: usize) {
        if let Some(item) = people::options(self).get(i) {
            people::option(self, item);
        }
    }

    /// The header's icon `i` (the action, then `Chrome::more`).
    pub fn header_action(&self, i: usize) {
        match people::chrome(self).header_action(i) {
            Some(None) => self.action(),
            Some(Some(name)) => people::option(self, name),
            None => {}
        }
    }

    /// The selection header's action `i` (`Chrome::selection`).
    pub fn selection_action(&self, i: usize) {
        let name = people::chrome(self).selection.get(i).map(|(_, n)| *n);
        if let Some(name) = name {
            people::option(self, name);
        }
    }

    /// Item `i` of the selection header's ⋮ (`Chrome::selection_menu`).
    pub fn selection_option(&self, i: usize) {
        let name = people::chrome(self).selection_menu.get(i).copied();
        if let Some(name) = name {
            people::option(self, name);
        }
    }

    /// The header's search field changed (each keystroke; "" when it closed).
    pub fn searched(&self, query: &str) {
        people::searched(self, query);
    }

    /// The UI server's dialog was answered: `yes` is its first (confirming) button.
    pub fn answer(&self, yes: bool) {
        *self.asked.borrow_mut() = None; // the UI server closed it
        people::answer(self, yes);
    }

    /// The header's action or a form's ✓.
    pub fn action(&self) {
        people::action(self);
    }

    /// Model → UI.
    pub fn show(&self) {
        people::show(self);
    }

    /// A toast for 3 s (the shell's NOTE, above the key row or band).
    pub fn note(&self, text: &str) {
        if let Err(e) = self.app.note(&phone_common::ui::note_text(text), "") {
            dprintln!("neocont: note not shown ({})", e);
        }
    }

    /// The header, the UI server's page and the keyboard as the view needs them; then draw.
    pub fn sync(&self) -> Result<(), i32> {
        let ui = self.ui();
        self.header(&ui);
        let page = self.page_now(&ui);
        if self.shown.borrow().as_ref() != Some(&page) {
            if let Err(e) = self.app.page(&page) {
                dprintln!("neocont: page not set ({})", e);
            }
            *self.shown.borrow_mut() = Some(page);
        }
        let query = people::query(self);
        if *self.asked.borrow() != query {
            let r = match &query {
                Some(q) => self.app.query(q),
                None => self.app.close_query(),
            };
            if let Err(e) = r {
                dprintln!("neocont: dialog not shown ({})", e);
            }
            *self.asked.borrow_mut() = query;
        }
        // a field took the focus (or was tapped after the keyboard was put away): ask for its keyboard; none focused:
        // hide it
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
            // how much of the surface it covers: the page keeps its field above it
            let h = self.app.keyboard(mode).unwrap_or(0);
            kf.set_text_h(h as f32);
        }
        self.app.draw()
    }

    /// The view's kit header and footer (Nav), from its [`Chrome`].
    fn header(&self, ui: &ContactsApp) {
        let chrome = people::chrome(self);
        let nav = ui.global::<Nav>();
        let empty = chrome.style & tool::EMPTY != 0;
        nav.set_title(chrome.title.as_str().into());
        // Return to call: a call goes on and its screen isn't showing
        let bar = {
            let mut m = self.m.borrow_mut();
            let m = &mut *m;
            if true {
                m.call_bar
                    .text(m.status.as_ref(), &m.book, m.order, e32::time::millis())
            } else {
                String::new()
            }
        };
        nav.set_call_bar(bar.as_str().into());

        nav.set_sub(self.page().back().is_some());
        nav.set_form(chrome.style & tool::CANCEL != 0);
        nav.set_has_action(chrome.icon != tool::NONE && !empty);
        nav.set_action_tool(chrome.icon);
        nav.set_action_label(chrome.label.as_str().into());
        nav.set_pill(chrome.style & tool::PILL != 0);
        // the footer (Phone · Contacts · Messages) on the root view
        nav.set_footer(
            self.page() == Page::Contacts(ContactsView::List)
                && chrome.selected == 0
                && !chrome.selecting
                && !nav.get_searching(),
        );
        let (missed, unread) = {
            let m = self.m.borrow();
            (
                phone_core::recents::new_missed(&m.log, m.prefs.missed_seen),
                m.status.map_or(0, |s| s.sms_unread),
            )
        };
        nav.set_missed(missed as i32);
        nav.set_unread(i32::from(unread));
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
        nav.set_search_hint(people::search_hint(self).as_str().into());
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
        let options = people::options(self);
        if nav
            .get_options()
            .iter()
            .ne(options.iter().map(|o| SharedString::from(*o)))
        {
            nav.set_options(strings(&options));
        }
    }

    /// What the UI server needs of the current view: its name; Back stays in the app while there is somewhere to go
    /// back to, something Back closes, or a kit layer (the ⋮ menu) open.
    fn page_now(&self, ui: &ContactsApp) -> neoapp::Page {
        let chrome: Chrome = people::chrome(self);
        neoapp::Page {
            title: chrome.title,
            can_back: self.page().back().is_some()
                || people::wants_back(self)
                || ui.global::<Kit>().get_layers() > 0,
            full: false,
            awake: false,
        }
    }

    /// MODEMD sent a status: the call log (the footer's missed calls) is read again when it changed.
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
                Err(e) => dprintln!("neocont: call log not read ({})", e),
            }
        }

        self.show();
    }

    /// Read the settings (`phone.*`, `contacts.*`) into the model.
    pub fn reload_prefs(&self) {
        let (state, order) = phone_common::prefs::read(&self.app);
        let mut m = self.m.borrow_mut();
        m.prefs = state;
        m.order = order;
    }
}
