// SPDX-License-Identifier: Apache-2.0
//! Pages and views of Phone, Contacts and Messages: three apps (NEOPHONE, NEOCONT, NEOMSG) that open each other's
//! pages (neoui ipc OPEN, `AppEvent::Open`). Each page belongs to one app ([`Page::app`]); a move to another app's page
//! opens that app on it, by its number there ([`Page::number`]) with an argument (a number). The UI server opens
//! Phone's pages by the same numbers (neoui `host::PAGES`, `AppEvent::Page`). Inside Phone, Recents and Speed dial are
//! views of the dialer.

/// The apps' launcher keys (neoui `host::APPS`).
pub mod app {
    pub const PHONE: &str = "phone";
    pub const CONTACTS: &str = "contacts";
    pub const MESSAGES: &str = "messages";
}

/// Each app's page numbers (its own numbering, for OPEN and the UI server).
pub mod number {
    /// Phone (neoui `host::PAGES`: recents, dial, call, settings, speed).
    pub mod phone {
        pub const RECENTS: u16 = 0;
        pub const DIAL: u16 = 1;
        /// arg: the number to call ("" shows the call screens)
        pub const CALL: u16 = 2;
        /// arg: the section to show ("messages", "contacts"; "" the top)
        pub const SETTINGS: u16 = 3;
        /// arg: a number to put on a key (a contact card's Speed dial)
        pub const SPEED: u16 = 4;
    }
    pub mod contacts {
        pub const LIST: u16 = 0;
        /// arg: a number whose contact shows (none matches: a new contact with it)
        pub const CARD: u16 = 1;
        /// arg: the new contact's number
        pub const NEW: u16 = 2;
    }
    pub mod messages {
        pub const LIST: u16 = 0;
        /// arg: the conversation's number
        pub const THREAD: u16 = 1;
        /// arg: the conversation's number; the composer takes the focus (Reply)
        pub const WRITE: u16 = 2;
        pub const NEW: u16 = 3;
    }
}

/// What the app shows: `Nav.page` (0 Phone, 1 Contacts, 3 call, 4 Settings, 6 Messages) and `Nav.view`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Phone(PhoneView),
    Contacts(ContactsView),
    Call,
    Settings(SettingsView),
    Messages(MessagesView),
}

/// Phone's views: Recents is the root; the dialer is the keypad sheet over it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhoneView {
    /// Recents with the keypad sheet open
    Dial,
    Recents,
    Detail,
    Speed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactsView {
    List,
    Card,
    Edit,
    Mark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsView {
    Main,
    Blocked,
    /// the quick replies of "Reply with message"
    Replies,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessagesView {
    List,
    Thread,
    New,
    Blocked,
}

impl Page {
    /// The page app `app`'s number `n` opens (`None`: not one of its pages).
    pub fn from_number(app: &str, n: u16) -> Option<Self> {
        use number::{contacts, messages, phone};
        Some(match (app, n) {
            (app::PHONE, phone::RECENTS) => Page::Phone(PhoneView::Recents),
            (app::PHONE, phone::DIAL) => Page::Phone(PhoneView::Dial),
            (app::PHONE, phone::CALL) => Page::Call,
            (app::PHONE, phone::SETTINGS) => Page::Settings(SettingsView::Main),
            (app::PHONE, phone::SPEED) => Page::Phone(PhoneView::Speed),
            (app::CONTACTS, contacts::LIST) => Page::Contacts(ContactsView::List),
            (app::CONTACTS, contacts::CARD) => Page::Contacts(ContactsView::Card),
            (app::CONTACTS, contacts::NEW) => Page::Contacts(ContactsView::Edit),
            (app::MESSAGES, messages::LIST) => Page::Messages(MessagesView::List),
            (app::MESSAGES, messages::THREAD | messages::WRITE) => {
                Page::Messages(MessagesView::Thread)
            }
            (app::MESSAGES, messages::NEW) => Page::Messages(MessagesView::New),
            _ => return None,
        })
    }

    /// The app this page belongs to.
    pub fn app(self) -> &'static str {
        match self {
            Page::Phone(_) | Page::Call | Page::Settings(_) => app::PHONE,
            Page::Contacts(_) => app::CONTACTS,
            Page::Messages(_) => app::MESSAGES,
        }
    }

    /// Its number in its app (for OPEN from another app); the nearest page OPEN can reach.
    pub fn number(self) -> u16 {
        use number::{contacts, messages, phone};
        match self {
            Page::Phone(PhoneView::Recents | PhoneView::Detail) => phone::RECENTS,
            Page::Phone(PhoneView::Dial) => phone::DIAL,
            Page::Phone(PhoneView::Speed) => phone::SPEED,
            Page::Call => phone::CALL,
            Page::Settings(_) => phone::SETTINGS,
            Page::Contacts(ContactsView::List | ContactsView::Mark) => contacts::LIST,
            Page::Contacts(ContactsView::Card) => contacts::CARD,
            Page::Contacts(ContactsView::Edit) => contacts::NEW,
            Page::Messages(MessagesView::List | MessagesView::Blocked) => messages::LIST,
            Page::Messages(MessagesView::Thread) => messages::THREAD,
            Page::Messages(MessagesView::New) => messages::NEW,
        }
    }

    /// (`Nav.page`, `Nav.view`) for phone-ui.
    pub fn nav(self) -> (i32, &'static str) {
        match self {
            Page::Phone(v) => (
                0,
                match v {
                    PhoneView::Recents => "",
                    PhoneView::Dial => "dial",
                    PhoneView::Detail => "detail",
                    PhoneView::Speed => "speed",
                },
            ),
            Page::Contacts(v) => (
                1,
                match v {
                    ContactsView::List => "",
                    ContactsView::Card => "card",
                    ContactsView::Edit => "edit",
                    ContactsView::Mark => "mark",
                },
            ),
            Page::Call => (3, ""),
            Page::Settings(v) => (
                4,
                match v {
                    SettingsView::Main => "",
                    SettingsView::Blocked => "blocked",
                    SettingsView::Replies => "replies",
                },
            ),
            Page::Messages(v) => (
                6,
                match v {
                    MessagesView::List => "",
                    MessagesView::Thread => "thread",
                    MessagesView::New => "new",
                    MessagesView::Blocked => "blocked",
                },
            ),
        }
    }

    /// Where Back goes from here; `None` leaves the app (the UI server's Back: home).
    pub fn back(self) -> Option<Self> {
        match self {
            Page::Phone(PhoneView::Recents) | Page::Contacts(ContactsView::List) | Page::Call => {
                None
            }
            Page::Phone(_) => Some(Page::Phone(PhoneView::Recents)),
            Page::Contacts(ContactsView::Edit) => Some(Page::Contacts(ContactsView::Card)),
            Page::Contacts(_) => Some(Page::Contacts(ContactsView::List)),
            Page::Settings(SettingsView::Blocked | SettingsView::Replies) => {
                Some(Page::Settings(SettingsView::Main))
            }
            Page::Settings(SettingsView::Main) => Some(Page::Phone(PhoneView::Recents)),
            Page::Messages(MessagesView::List) => None,
            Page::Messages(_) => Some(Page::Messages(MessagesView::List)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_back() {
        assert_eq!(
            Page::from_number(app::CONTACTS, 0),
            Some(Page::Contacts(ContactsView::List))
        );
        assert_eq!(
            Page::from_number(app::PHONE, 4).unwrap().nav(),
            (0, "speed")
        );
        assert_eq!(Page::from_number(app::PHONE, 9), None);
        assert_eq!(Page::from_number(app::CONTACTS, 4), None);
        // every page opens in its own app at its own number
        for p in [
            Page::Phone(PhoneView::Dial),
            Page::Phone(PhoneView::Recents),
            Page::Phone(PhoneView::Speed),
            Page::Call,
            Page::Settings(SettingsView::Main),
            Page::Contacts(ContactsView::List),
            Page::Contacts(ContactsView::Card),
            Page::Contacts(ContactsView::Edit),
            Page::Messages(MessagesView::List),
            Page::Messages(MessagesView::Thread),
            Page::Messages(MessagesView::New),
        ] {
            assert_eq!(Page::from_number(p.app(), p.number()), Some(p), "{p:?}");
        }
        assert_eq!(
            Page::Phone(PhoneView::Detail).back(),
            Some(Page::Phone(PhoneView::Recents))
        );
        assert_eq!(
            Page::Phone(PhoneView::Dial).back(),
            Some(Page::Phone(PhoneView::Recents))
        );
        assert_eq!(Page::Phone(PhoneView::Recents).back(), None);
        assert_eq!(Page::from_number(app::PHONE, 0).unwrap().nav(), (0, ""));
        assert_eq!(
            Page::Settings(SettingsView::Main).back(),
            Some(Page::Phone(PhoneView::Recents))
        );
        assert_eq!(Page::from_number(app::MESSAGES, 0).unwrap().nav(), (6, ""));
        assert_eq!(
            Page::Messages(MessagesView::Thread).back(),
            Some(Page::Messages(MessagesView::List))
        );
    }
}
