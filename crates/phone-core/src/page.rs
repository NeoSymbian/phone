// SPDX-License-Identifier: Apache-2.0
//! Pages and views. The UI server opens a page by number (neoui `host::PAGES`, `AppEvent::Page`); inside the app,
//! Recents and Speed dial are views of the Phone page.

/// The UI server's page numbers.
pub mod number {
    pub const DIAL: u16 = 0;
    pub const CONTACTS: u16 = 1;
    pub const RECENTS: u16 = 2;
    pub const CALL: u16 = 3;
    pub const SETTINGS: u16 = 4;
    pub const SPEED: u16 = 5;
    pub const MESSAGES: u16 = 6;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhoneView {
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
    /// The page the UI server's number `n` opens (`None`: not ours).
    pub fn from_number(n: u16) -> Option<Self> {
        Some(match n {
            number::DIAL => Page::Phone(PhoneView::Dial),
            number::CONTACTS => Page::Contacts(ContactsView::List),
            number::RECENTS => Page::Phone(PhoneView::Recents),
            number::CALL => Page::Call,
            number::SETTINGS => Page::Settings(SettingsView::Main),
            number::SPEED => Page::Phone(PhoneView::Speed),
            number::MESSAGES => Page::Messages(MessagesView::List),
            _ => return None,
        })
    }

    /// (`Nav.page`, `Nav.view`) for phone-ui.
    pub fn nav(self) -> (i32, &'static str) {
        match self {
            Page::Phone(v) => (
                0,
                match v {
                    PhoneView::Dial => "",
                    PhoneView::Recents => "recents",
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
            Page::Phone(PhoneView::Dial) | Page::Contacts(ContactsView::List) | Page::Call => None,
            Page::Phone(PhoneView::Detail) => Some(Page::Phone(PhoneView::Recents)),
            Page::Phone(_) => Some(Page::Phone(PhoneView::Dial)),
            Page::Contacts(ContactsView::Edit) => Some(Page::Contacts(ContactsView::Card)),
            Page::Contacts(_) => Some(Page::Contacts(ContactsView::List)),
            Page::Settings(SettingsView::Blocked | SettingsView::Replies) => {
                Some(Page::Settings(SettingsView::Main))
            }
            Page::Settings(SettingsView::Main) => None,
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
            Page::from_number(1),
            Some(Page::Contacts(ContactsView::List))
        );
        assert_eq!(Page::from_number(5).unwrap().nav(), (0, "speed"));
        assert_eq!(Page::from_number(9), None);
        assert_eq!(
            Page::Phone(PhoneView::Detail).back(),
            Some(Page::Phone(PhoneView::Recents))
        );
        assert_eq!(Page::Phone(PhoneView::Dial).back(), None);
        assert_eq!(Page::from_number(6).unwrap().nav(), (6, ""));
        assert_eq!(
            Page::Messages(MessagesView::Thread).back(),
            Some(Page::Messages(MessagesView::List))
        );
    }
}
