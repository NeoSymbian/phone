// SPDX-License-Identifier: Apache-2.0
//! The Contacts page without UI (mockups 16-22, 24, 26): avatars, the list's section offsets, search hits, the card's
//! speed-dial key and block line, the editor's checks, vCard merging and the known-number hashes MODEMD gets.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use contacts::index::Section;
use contacts::{Book, Contact, Label, NameOrder};
use phone_rules::number;

/// A section band's and a row's height on the list (dp), for scrolling to a section.
pub const BAND_H: f32 = 30.0;
pub const ROW_H: f32 = 56.0;
/// The side index's pitch (dp per place).
pub const INDEX_PITCH: f32 = 17.0;

/// The initials an avatar shows: the first letters of the first two words ("#" for none).
pub fn initials(name: &str) -> String {
    let i: String = name
        .split_whitespace()
        .take(2)
        .filter_map(|w| w.chars().next())
        .flat_map(char::to_uppercase)
        .collect();
    if i.is_empty() { "#".into() } else { i }
}

/// An avatar colour (0xRRGGBB) that stays with the contact.
pub fn tint(id: u32) -> u32 {
    const TINTS: [u32; 8] = [
        0x22b46a, 0xd6409f, 0xe8833a, 0x8b5cf6, 0x14a3a3, 0x2f80ed, 0xe5484d, 0x14a37f,
    ];
    TINTS[id as usize % TINTS.len()]
}

/// A blocked contact's avatar: the colour at 45 % saturation.
pub fn desaturate(rgb: u32) -> u32 {
    let ch = |s: u32| ((rgb >> s) & 0xff) as f32;
    let (r, g, b) = (ch(16), ch(8), ch(0));
    let grey = 0.299 * r + 0.587 * g + 0.114 * b;
    let mix = |c: f32| (grey + (c - grey) * 0.45) as u32 & 0xff;
    mix(r) << 16 | mix(g) << 8 | mix(b)
}

/// Where each section's band starts below the first one (dp).
pub fn tops(sections: &[Section]) -> Vec<f32> {
    let mut y = 0.0;
    sections
        .iter()
        .map(|s| {
            let top = y;
            y += BAND_H + ROW_H * s.ids.len() as f32;
            top
        })
        .collect()
}

/// How many places the side index has in a list `height` dp high.
pub fn index_room(height: f32) -> usize {
    (height / INDEX_PITCH).max(3.0) as usize
}

fn low(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// `text` cut around the first word that starts with `query` (case folded): (before, match, after).
pub fn split_word<'a>(text: &'a str, query: &str) -> Option<(&'a str, &'a str, &'a str)> {
    let q: Vec<char> = query.chars().map(low).collect();
    let cs: Vec<(usize, char)> = text.char_indices().collect();
    if q.is_empty() || q.len() > cs.len() {
        return None;
    }
    (0..=cs.len() - q.len())
        .filter(|&i| i == 0 || cs[i - 1].1.is_whitespace())
        .find(|&i| (0..q.len()).all(|k| low(cs[i + k].1) == q[k]))
        .map(|i| {
            let (a, b) = (cs[i].0, cs.get(i + q.len()).map_or(text.len(), |c| c.0));
            (&text[..a], &text[a..b], &text[b..])
        })
}

/// A search result's two lines, each as (before, match, after): the name, then the first number or what matched.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hit {
    pub name: [String; 3],
    pub detail: [String; 3],
}

fn parts(p: (&str, &str, &str)) -> [String; 3] {
    [p.0.into(), p.1.into(), p.2.into()]
}

/// How `c` matched the search `query` ([`Book::search`]).
pub fn hit(c: &Contact, query: &str, order: NameOrder) -> Hit {
    let title = c.title(order);
    let words: Vec<&str> = query.split_whitespace().collect();
    let first_number =
        |(l, n): &(Label, String)| [format!("{} · ", l.text()), String::new(), n.clone()];
    let name = words.iter().find_map(|w| split_word(&title, w));
    let detail = if name.is_some() {
        None
    } else if let Some(p) = words.iter().find_map(|w| split_word(&c.company, w)) {
        Some([format!("Company: {}", p.0), p.1.into(), p.2.into()])
    } else {
        let d = number::digits(query);
        c.numbers.iter().find_map(|(l, n)| {
            let at = n.find(d.as_str()).filter(|_| !d.is_empty())?;
            Some([
                format!("{} · {}", l.text(), &n[..at]),
                d.clone(),
                n[at + d.len()..].into(),
            ])
        })
    };
    Hit {
        name: name.map_or_else(|| [title.clone(), String::new(), String::new()], parts),
        detail: detail
            .or_else(|| c.numbers.first().map(first_number))
            .unwrap_or_default(),
    }
}

/// The speed-dial key (2..=9) one of `c`'s numbers is on; `speed` holds keys 2..=9.
pub fn speed_key(speed: &[String], c: &Contact) -> Option<u8> {
    speed
        .iter()
        .position(|s| c.numbers.iter().any(|(_, n)| number::matches(s, n)))
        .map(|i| i as u8 + 2)
}

/// Clear every speed-dial key that calls one of `c`'s numbers; true if one was.
pub fn clear_speed(speed: &mut [String], c: &Contact) -> bool {
    let mut cleared = false;
    for s in speed.iter_mut() {
        if c.numbers.iter().any(|(_, n)| number::matches(s, n)) {
            s.clear();
            cleared = true;
        }
    }
    cleared
}

/// The blocked card's line: what the rule blocks.
pub fn block_text(calls: bool, messages: bool) -> &'static str {
    match (calls, messages) {
        (true, true) => "Calls and messages",
        (true, false) => "Calls",
        _ => "Messages",
    }
}

/// The delete query's detail.
pub fn delete_detail(key: Option<u8>) -> String {
    key.map(|k| format!("Its speed dial key {k} is cleared too."))
        .unwrap_or_default()
}

/// The editor saves a contact with a name (or company, nickname) or a number.
pub fn worth_saving(c: &Contact) -> bool {
    let mut c = c.clone();
    c.tidy();
    !(c.first.is_empty()
        && c.last.is_empty()
        && c.company.is_empty()
        && c.nickname.is_empty()
        && c.numbers.is_empty())
}

/// Add imported contacts, skipping ones the book has (same name and first number); (added, skipped).
pub fn merge(book: &mut Book, found: Vec<Contact>) -> (usize, usize) {
    let (mut added, mut skipped) = (0, 0);
    for c in found {
        let dup = book.contacts.iter().any(|x| {
            x.title(NameOrder::FirstLast) == c.title(NameOrder::FirstLast)
                && x.numbers.first() == c.numbers.first()
        });
        if dup || book.add(c).is_err() {
            skipped += 1;
        } else {
            added += 1;
        }
    }
    (added, skipped)
}

/// Every contact number as [`phone_rules::block::known_key`], sorted, once each (my card is not a caller).
pub fn known_keys(book: &Book) -> Vec<u32> {
    let mut v: Vec<u32> = book
        .contacts
        .iter()
        .flat_map(|c| &c.numbers)
        .filter_map(|(_, n)| phone_rules::block::known_key(n))
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn person(first: &str, last: &str, company: &str, numbers: &[&str]) -> Contact {
        Contact {
            first: first.into(),
            last: last.into(),
            company: company.into(),
            numbers: numbers
                .iter()
                .map(|n| (Label::Mobile, String::from(*n)))
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn avatars() {
        assert_eq!(initials("Ava Lindqvist"), "AL");
        assert_eq!(initials("անի պետրոսյան"), "ԱՊ");
        assert_eq!(initials("Мария"), "М");
        assert_eq!(initials("  "), "#");
        assert_eq!(tint(1), tint(9));
        assert_ne!(tint(1), tint(2));
        assert_eq!(desaturate(0x808080), 0x808080);
        let d = desaturate(0xd6409f);
        assert!((d >> 16) < 0xd6 && (d >> 16) > 0x80, "{d:06x}");
    }

    #[test]
    fn hits() {
        assert_eq!(
            split_word("Ani Petrosyan", "an"),
            Some(("", "An", "i Petrosyan"))
        );
        assert_eq!(
            split_word("Ani Petrosyan", "PE"),
            Some(("Ani ", "Pe", "trosyan"))
        );
        assert_eq!(split_word("Vahan & Co", "an"), None);
        assert_eq!(
            split_word("Мария Ковалёва", "ков"),
            Some(("Мария ", "Ков", "алёва"))
        );
        assert_eq!(split_word("Ab", "abc"), None);
        let o = NameOrder::FirstLast;
        let ani = person("Ani", "Petrosyan", "", &["+37428123456"]);
        assert_eq!(
            hit(&ani, "an", o),
            Hit {
                name: ["".into(), "An".into(), "i Petrosyan".into()],
                detail: ["Mobile · ".into(), "".into(), "+37428123456".into()],
            }
        );
        let aram = person("Aram", "Hakobyan", "Anahit & Co", &["+37491000111"]);
        assert_eq!(hit(&aram, "an", o).detail, ["Company: ", "An", "ahit & Co"]);
        assert_eq!(hit(&aram, "an", o).name[0], "Aram Hakobyan");
        assert_eq!(
            hit(&aram, "000", o).detail,
            ["Mobile · +37491", "000", "111"]
        );
        assert_eq!(
            hit(&Contact::default(), "x", o),
            Hit {
                name: ["Unnamed".into(), "".into(), "".into()],
                ..Default::default()
            }
        );
    }

    #[test]
    fn speed_and_delete() {
        let mut speed: Vec<String> = vec!["".into(), "".into(), "091234567".into(), "".into()];
        let c = person("Ani", "", "", &["+37491234567"]);
        assert_eq!(speed_key(&speed, &c), Some(4));
        assert_eq!(speed_key(&speed, &person("X", "", "", &[])), None);
        assert_eq!(
            delete_detail(Some(4)),
            "Its speed dial key 4 is cleared too."
        );
        assert_eq!(delete_detail(None), "");
        assert!(clear_speed(&mut speed, &c));
        assert_eq!(speed_key(&speed, &c), None);
        assert!(!clear_speed(&mut speed, &c));
    }

    #[test]
    fn editor_checks_and_merge() {
        assert!(!worth_saving(&Contact {
            email: "a@b".into(),
            note: "x".into(),
            ..Default::default()
        }));
        assert!(worth_saving(&person(" ", "", "", &["+1 555"])));
        assert!(worth_saving(&person("Ani", "", "", &[])));
        let mut b = Book::default();
        b.add(person("Ani", "P", "", &["1"])).unwrap();
        let (a, s) = merge(
            &mut b,
            vec![
                person("Ani", "P", "", &["1"]),
                person("Ani", "P", "", &["2"]),
                Contact::default(),
            ],
        );
        assert_eq!((a, s), (1, 2));
        assert_eq!(b.contacts.len(), 2);
    }

    #[test]
    fn offsets_and_keys() {
        let s = vec![
            Section {
                script: contacts::index::Script::Latin,
                letter: 'A',
                ids: vec![1, 2],
            },
            Section {
                script: contacts::index::Script::Latin,
                letter: 'B',
                ids: vec![3],
            },
            Section {
                script: contacts::index::Script::Other,
                letter: '#',
                ids: vec![4],
            },
        ];
        assert_eq!(tops(&s), [0.0, 142.0, 228.0]);
        assert_eq!(index_room(518.0), 30);
        assert_eq!(index_room(10.0), 3);
        let mut b = Book::default();
        b.add(person("A", "", "", &["+37491234567", "091234567"]))
            .unwrap();
        b.add(person("B", "", "", &["+15550142"])).unwrap();
        b.set_my_card(Some(person("Me", "", "", &["+15550199"])));
        let k = known_keys(&b);
        assert_eq!(k.len(), 2);
        assert!(k.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(block_text(true, true), "Calls and messages");
        assert_eq!(block_text(true, false), "Calls");
    }
}
