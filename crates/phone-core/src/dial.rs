// SPDX-License-Identifier: Apache-2.0
//! The dialer (mockups 3, 4): the typed number, the `* + p w` cycle, long presses, and the contacts the digits
//! match (smart dialling: names through [`contacts::t9`], numbers by their digits) with the matched part split out
//! so the screen can light it.

use alloc::string::String;
use alloc::vec::Vec;
use contacts::{Book, Label, NameOrder, t9};

/// A second `*` within this cycles the last character `* + p w` (Symbian^3).
pub const STAR_CYCLE_MS: u64 = 1000;
/// The longest number the dialer takes.
pub const MAX_LEN: usize = 80;
/// The most smart-dial rows.
pub const MAX_MATCHES: usize = 20;

/// The number being typed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dialer {
    pub number: String,
    /// when `*` was last tapped (0: the last key was another)
    star_at: u64,
}

/// What a held key does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Long {
    /// typed as a tap (the number isn't empty, or the key has no long press)
    Typed,
    /// `0`: a `+` was typed
    Plus,
    /// `1`: call the voice mailbox
    Voicemail,
    /// `2..=9`: call that speed-dial key
    Speed(usize),
}

impl Dialer {
    /// A tap on a key: `*` again within [`STAR_CYCLE_MS`] turns the last `*` into `+`, then `p`, `w`, `*`.
    pub fn key(&mut self, key: &str, now_ms: u64) {
        let last = self.number.chars().last();
        if key == "*"
            && self.star_at != 0
            && now_ms.saturating_sub(self.star_at) < STAR_CYCLE_MS
            && matches!(last, Some('*' | '+' | 'p' | 'w'))
        {
            self.number.pop();
            self.number.push(match last {
                Some('*') => '+',
                Some('+') => 'p',
                Some('p') => 'w',
                _ => '*',
            });
        } else if self.number.len() + key.len() <= MAX_LEN {
            self.number.push_str(key);
        }
        self.star_at = if key == "*" { now_ms.max(1) } else { 0 };
    }

    /// A key held 0.6 s: on an empty number `0` types `+`, `1` is the voice mailbox, `2..=9` speed dial; otherwise
    /// it types like a tap.
    pub fn long(&mut self, key: &str, now_ms: u64) -> Long {
        if self.number.is_empty() {
            match key {
                "0" => {
                    self.number.push('+');
                    self.star_at = 0;
                    return Long::Plus;
                }
                "1" => return Long::Voicemail,
                k => {
                    if let Some(k @ 2..=9) = k.parse::<usize>().ok() {
                        return Long::Speed(k);
                    }
                }
            }
        }
        self.key(key, now_ms);
        Long::Typed
    }

    pub fn backspace(&mut self) {
        self.number.pop();
        self.star_at = 0;
    }

    pub fn clear(&mut self) {
        self.number.clear();
        self.star_at = 0;
    }

    /// Call on a number that is one key `2..=9` speed-dials it (Symbian^3).
    pub fn single_key(&self) -> Option<usize> {
        match self.number.as_bytes() {
            [d @ b'2'..=b'9'] => Some(usize::from(d - b'0')),
            _ => None,
        }
    }
}

/// Text cut around a highlighted part.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Split {
    pub pre: String,
    pub hit: String,
    pub post: String,
}

impl Split {
    fn plain(s: &str) -> Self {
        Self {
            pre: s.into(),
            ..Self::default()
        }
    }
    fn at(s: &str, from: usize, to: usize) -> Self {
        Self {
            pre: s[..from].into(),
            hit: s[from..to].into(),
            post: s[to..].into(),
        }
    }
}

/// One smart-dial row: a contact and the number Call on it dials.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub id: u32,
    pub name: Split,
    pub label: Label,
    pub number: Split,
}

/// The contacts `digits` match, best first: names it spells on the keypad ([`t9::smart_matches`]), then numbers
/// that contain it. Only for plain digits; at most [`MAX_MATCHES`].
pub fn matches(book: &Book, digits: &str, order: NameOrder) -> Vec<Match> {
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Vec::new();
    }
    let mut out: Vec<Match> = Vec::new();
    for id in t9::smart_matches(book, digits, order) {
        let Some(c) = book.get(id) else { continue };
        let Some((label, n)) = c.numbers.first() else {
            continue;
        };
        let title = c.title(order);
        out.push(Match {
            id,
            name: name_split(&title, digits),
            label: *label,
            number: Split::plain(n),
        });
    }
    for id in book.sorted(order) {
        if out.iter().any(|m| m.id == id) {
            continue;
        }
        let Some(c) = book.get(id) else { continue };
        if let Some((label, n, at)) = c
            .numbers
            .iter()
            .find_map(|(l, n)| n.find(digits).map(|at| (*l, n, at)))
        {
            out.push(Match {
                id,
                name: Split::plain(&c.title(order)),
                label,
                number: Split::at(n, at, at + digits.len()),
            });
        }
    }
    out.truncate(MAX_MATCHES);
    out
}

/// The part of `title` the digits spell: the start of the first word whose keys begin with them (or with the
/// digits before a `0`, which also works as a space).
fn name_split(title: &str, digits: &str) -> Split {
    let typed = digits.trim_start_matches('0');
    let first = typed.split('0').find(|w| !w.is_empty()).unwrap_or("");
    for q in [digits, first] {
        if q.is_empty() {
            continue;
        }
        let mut start = 0;
        for word in title.split(' ') {
            let mut chars = word.char_indices();
            let mut end = None;
            let mut keys = q.chars();
            loop {
                let Some(want) = keys.next() else {
                    end = Some(chars.next().map_or(word.len(), |(i, _)| i));
                    break;
                };
                match chars.next() {
                    Some((_, c)) if t9::key(c.to_lowercase().next().unwrap_or(c)) == want => {}
                    _ => break,
                }
            }
            if let Some(end) = end {
                return Split::at(title, start, start + end);
            }
            start += word.len() + 1;
        }
    }
    Split::plain(title)
}

#[cfg(test)]
mod tests {
    use super::*;
    use contacts::Contact;

    fn person(first: &str, last: &str, n: &str) -> Contact {
        Contact {
            first: first.into(),
            last: last.into(),
            numbers: alloc::vec![(Label::Mobile, n.into())],
            ..Contact::default()
        }
    }

    #[test]
    fn star_cycles() {
        let mut d = Dialer::default();
        d.key("1", 10);
        d.key("*", 100);
        d.key("*", 200);
        assert_eq!(d.number, "1+");
        d.key("*", 300);
        d.key("*", 400);
        assert_eq!(d.number, "1w");
        d.key("*", 2000);
        assert_eq!(d.number, "1w*");
        d.key("2", 2100);
        d.key("*", 2200);
        assert_eq!(d.number, "1w*2*");
        d.backspace();
        assert_eq!(d.number, "1w*2");
    }

    #[test]
    fn long_presses() {
        let mut d = Dialer::default();
        assert_eq!(d.long("1", 0), Long::Voicemail);
        assert_eq!(d.long("5", 0), Long::Speed(5));
        assert_eq!(d.number, "");
        assert_eq!(d.long("0", 0), Long::Plus);
        assert_eq!(d.long("5", 0), Long::Typed);
        assert_eq!(d.number, "+5");
        d.clear();
        d.key("7", 0);
        assert_eq!(d.single_key(), Some(7));
        d.key("7", 0);
        assert_eq!(d.single_key(), None);
    }

    #[test]
    fn smart_rows() {
        let mut b = Book::default();
        b.add(person("Ava", "Lindqvist", "+15550142")).unwrap();
        b.add(person("Ani", "Petrosyan", "+37428123456")).unwrap();
        b.add(person("Иван", "Петров", "+15550177")).unwrap();
        let m = matches(&b, "28", NameOrder::FirstLast);
        assert_eq!(m.len(), 2);
        assert_eq!(
            m[0].name,
            Split {
                pre: "".into(),
                hit: "Av".into(),
                post: "a Lindqvist".into()
            }
        );
        assert_eq!(m[1].number.hit, "28");
        assert_eq!(m[1].number.pre, "+374");
        // Cyrillic: П = 5 (мноп), е = 3
        let m = matches(&b, "53", NameOrder::FirstLast);
        assert_eq!(m[0].name.hit, "Пе");
        assert_eq!(m[0].name.pre, "Иван ");
        assert!(matches(&b, "2*", NameOrder::FirstLast).is_empty());
    }
}
