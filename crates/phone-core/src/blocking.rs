// SPDX-License-Identifier: Apache-2.0
//! Editing the blocklist MODEMD applies before a call rings ([`phone_rules::block`]): adding a number or a prefix
//! (Settings ▸ Blocked numbers ▸ Add), blocking and unblocking a number from a card or a Recents row, and the rows'
//! text. The app sends the edited list with BLOCK_SET; MODEMD keeps its known-number hashes.

use alloc::format;
use alloc::string::String;
use phone_rules::block::{List, Rule, MAX_RULES, MAX_TEXT};
use phone_rules::{emergency, number};

/// Why a number can't be blocked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// not digits with an optional leading `+`, 3 to 32 characters (spaces and dashes allowed)
    Invalid,
    /// already in the list
    Duplicate,
    /// an emergency number (never blocked)
    Emergency,
    /// [`MAX_RULES`] rules already
    Full,
}

impl Refusal {
    /// The note the app shows.
    pub fn text(self) -> &'static str {
        match self {
            Refusal::Invalid => "Type a number (3 to 32 digits)",
            Refusal::Duplicate => "Already blocked",
            Refusal::Emergency => "Emergency numbers can't be blocked",
            Refusal::Full => "The list is full",
        }
    }
}

/// `text` trimmed, if it is a number a rule can hold.
fn valid(text: &str) -> Result<String, Refusal> {
    let t = text.trim();
    let body = t.strip_prefix('+').unwrap_or(t);
    let ok = body
        .chars()
        .all(|c| c.is_ascii_digit() || c == ' ' || c == '-')
        && number::digits(body).len() >= 3
        && t.len() <= MAX_TEXT;
    if !ok {
        return Err(Refusal::Invalid);
    }
    if emergency::check(&number::digits(t), None).is_some() {
        return Err(Refusal::Emergency);
    }
    Ok(String::from(t))
}

/// The index of the number rule `number` matches (as caller names do), or of the prefix rule with the same digits.
fn find(list: &List, text: &str, prefix: bool) -> Option<usize> {
    let d = number::digits(text);
    list.rules.iter().position(|r| {
        r.prefix == prefix
            && if prefix {
                number::digits(&r.number) == d
            } else {
                number::matches(&r.number, text)
            }
    })
}

/// Add a rule (Settings ▸ Blocked numbers ▸ Add): refused when invalid, already there, an emergency number or full.
pub fn add(
    list: &mut List,
    text: &str,
    prefix: bool,
    calls: bool,
    messages: bool,
) -> Result<(), Refusal> {
    let number = valid(text)?;
    if !calls && !messages {
        return Err(Refusal::Invalid);
    }
    if find(list, &number, prefix).is_some() {
        return Err(Refusal::Duplicate);
    }
    if list.rules.len() >= MAX_RULES {
        return Err(Refusal::Full);
    }
    list.rules.push(Rule {
        prefix,
        calls,
        messages,
        number,
    });
    Ok(())
}

/// Block `number` for calls and/or messages (a card's or a Recents row's Block): adds a number rule, or widens the
/// one that already matches.
pub fn block(list: &mut List, number: &str, calls: bool, messages: bool) -> Result<(), Refusal> {
    match find(list, number, false) {
        Some(i) => {
            let r = &mut list.rules[i];
            r.calls |= calls;
            r.messages |= messages;
            Ok(())
        }
        None => add(list, number, false, calls, messages),
    }
}

/// Remove every number rule `number` matches (Unblock); false when none did. Prefix rules stay.
pub fn remove_number(list: &mut List, number: &str) -> bool {
    let before = list.rules.len();
    list.rules
        .retain(|r| r.prefix || !number::matches(&r.number, number));
    list.rules.len() != before
}

/// What the number rules matching `number` block: (calls, messages); `None` when none matches.
pub fn lookup(list: &List, number: &str) -> Option<(bool, bool)> {
    list.rules
        .iter()
        .filter(|r| !r.prefix && number::matches(&r.number, number))
        .fold(None, |acc, r| {
            let (c, m) = acc.unwrap_or((false, false));
            Some((c || r.calls, m || r.messages))
        })
}

/// A rule's title when no contact owns it: the number, or "Starts with +1 900".
pub fn title(rule: &Rule) -> String {
    if rule.prefix {
        format!("Starts with {}", rule.number)
    } else {
        rule.number.clone()
    }
}

/// What a rule blocks: "Calls, messages", "Calls" or "Messages", then " · prefix" for a prefix rule.
pub fn summary(rule: &Rule) -> String {
    let what = match (rule.calls, rule.messages) {
        (true, true) => "Calls, messages",
        (true, false) => "Calls",
        (false, true) => "Messages",
        (false, false) => "Nothing",
    };
    if rule.prefix {
        format!("{what} · prefix")
    } else {
        String::from(what)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_validates() {
        let mut l = List::default();
        assert_eq!(add(&mut l, " +374 28 123456 ", false, true, true), Ok(()));
        assert_eq!(l.rules[0].number, "+374 28 123456");
        assert_eq!(
            add(&mut l, "028123456", false, true, false),
            Err(Refusal::Duplicate)
        );
        assert_eq!(add(&mut l, "12", false, true, true), Err(Refusal::Invalid));
        assert_eq!(
            add(&mut l, "12a45", false, true, true),
            Err(Refusal::Invalid)
        );
        assert_eq!(
            add(&mut l, "++12345", false, true, true),
            Err(Refusal::Invalid)
        );
        assert_eq!(
            add(&mut l, &"1".repeat(33), false, true, true),
            Err(Refusal::Invalid)
        );
        assert_eq!(
            add(&mut l, "112", false, true, true),
            Err(Refusal::Emergency)
        );
        assert_eq!(
            add(&mut l, "+911", true, true, true),
            Err(Refusal::Emergency)
        );
        assert_eq!(add(&mut l, "+1 900", true, true, true), Ok(()));
        assert_eq!(
            add(&mut l, "1900", true, true, true),
            Err(Refusal::Duplicate)
        );
        // a prefix and a number with the same digits are different rules
        assert_eq!(add(&mut l, "+1 900", false, true, true), Ok(()));
        while l.rules.len() < MAX_RULES {
            let n = format!("555{:04}", l.rules.len());
            add(&mut l, &n, true, true, true).unwrap();
        }
        assert_eq!(
            add(&mut l, "5551234", false, true, true),
            Err(Refusal::Full)
        );
    }

    #[test]
    fn block_merges_and_unblock_removes() {
        let mut l = List::default();
        block(&mut l, "+37491234567", true, false).unwrap();
        block(&mut l, "091234567", false, true).unwrap();
        assert_eq!(l.rules.len(), 1);
        assert_eq!(lookup(&l, "91234567"), Some((true, true)));
        add(&mut l, "+374", true, true, true).unwrap();
        assert_eq!(
            lookup(&l, "+37455000000"),
            None,
            "prefixes aren't number rules"
        );
        assert!(remove_number(&mut l, "+374 91 234567"));
        assert!(!remove_number(&mut l, "+374 91 234567"));
        assert_eq!(l.rules.len(), 1);
        assert!(l.rules[0].prefix);
    }

    #[test]
    fn row_text() {
        let mut l = List::default();
        add(&mut l, "+1 555 0199", false, true, false).unwrap();
        add(&mut l, "+1 900", true, true, true).unwrap();
        assert_eq!(title(&l.rules[0]), "+1 555 0199");
        assert_eq!(summary(&l.rules[0]), "Calls");
        assert_eq!(title(&l.rules[1]), "Starts with +1 900");
        assert_eq!(summary(&l.rules[1]), "Calls, messages · prefix");
    }
}
