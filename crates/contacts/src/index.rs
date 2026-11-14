// SPDX-License-Identifier: Apache-2.0
//! The contact list's sections and its side index, per script (mockup 16): Latin A–Z, Armenian Ա–Ֆ, Cyrillic А–Я,
//! the phone's language first, then "#" for every other name. Within a script, names sort by their folded title.
//! The index shows only letters in use, a dash between scripts, and folds runs into "•" when it would not fit.

use crate::{Book, NameOrder, fold};
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Script {
    #[default]
    Latin,
    Armenian,
    Cyrillic,
    /// digits, signs, other alphabets: the "#" section
    Other,
}

const SCRIPTS: [Script; 3] = [Script::Latin, Script::Armenian, Script::Cyrillic];

/// The base letters of U+00C0..=U+017F (Latin-1 Supplement and Latin Extended-A), `-` for signs.
const LATIN_BASE: &str = concat!(
    "AAAAAAACEEEEIIII", // C0
    "DNOOOOO-OUUUUYTS", // D0
    "AAAAAAACEEEEIIII", // E0
    "DNOOOOO-OUUUUYTY", // F0
    "AAAAAACCCCCCCCDD", // 100
    "DDEEEEEEEEEEGGGG", // 110
    "GGGGHHHHIIIIIIII", // 120
    "IIIIJJKKKLLLLLLL", // 130
    "LLLNNNNNNNNNOOOO", // 140
    "OOOORRRRRRSSSSSS", // 150
    "SSTTTTTTUUUUUUUU", // 160
    "UUUUWWYYYZZZZZZS", // 170
);

/// The section letter of a name starting with `c`, and its script.
pub fn letter(c: char) -> (Script, char) {
    let up = c.to_uppercase().next().unwrap_or(c);
    match up as u32 {
        0x41..=0x5a => (Script::Latin, up),
        u @ 0xc0..=0x17f => match LATIN_BASE.as_bytes()[(u - 0xc0) as usize] {
            b'-' => (Script::Other, '#'),
            b => (Script::Latin, char::from(b)),
        },
        0x531..=0x556 => (Script::Armenian, up),
        // the "ew" ligature has no capital: it starts like Ե
        0x587 => (Script::Armenian, 'Ե'),
        0x410..=0x42f => (Script::Cyrillic, up),
        0x401 => (Script::Cyrillic, 'Е'),
        // shortcut: other Cyrillic letters (Ukrainian, Serbian) keep their own sections, ordered by code point
        0x400..=0x4ff => (Script::Cyrillic, up),
        _ => (Script::Other, '#'),
    }
}

/// One section of the list: its letter and the contacts under it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub script: Script,
    pub letter: char,
    pub ids: Vec<u32>,
}

/// The script order: `first` (the phone's language), then the others, then "#".
fn rank(s: Script, first: Script) -> u8 {
    if s == Script::Other {
        3
    } else if s == first {
        0
    } else {
        1 + SCRIPTS
            .iter()
            .filter(|&&x| x != first)
            .position(|&x| x == s)
            .unwrap_or(0) as u8
    }
}

/// Every contact (favourites too) under its letter; sections in script order, then letter order.
pub fn sections(book: &Book, order: NameOrder, first: Script) -> Vec<Section> {
    let mut v: Vec<(u8, char, String, u32)> = book
        .contacts
        .iter()
        .map(|c| {
            let key = fold(&c.title(order));
            let (s, l) = letter(key.chars().next().unwrap_or('#'));
            (rank(s, first), l, key, c.id)
        })
        .collect();
    v.sort();
    let mut out: Vec<Section> = Vec::new();
    for (_, l, key, id) in v {
        match out.last_mut() {
            Some(s) if s.letter == l => s.ids.push(id),
            _ => out.push(Section {
                script: letter(key.chars().next().unwrap_or('#')).0,
                letter: l,
                ids: alloc::vec![id],
            }),
        }
    }
    out
}

/// One place on the side index: a letter, a "•" standing for several, or the dash between scripts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mark {
    pub text: char,
    /// the sections (indexes into [`sections`]) it stands for: one for a letter, several for "•", none for the dash
    pub sections: Vec<usize>,
}

pub const DOT: char = '•';
pub const DASH: char = '–';

/// The side index for `sections`, in at most `max` places (when it can: at least one per script, a dash between).
/// A script that does not fit keeps its first and last letters and folds the middle into one "•".
pub fn side_index(sections: &[Section], max: usize) -> Vec<Mark> {
    // runs of the same script ("#" is its own run)
    let mut runs: Vec<Vec<usize>> = Vec::new();
    for (i, s) in sections.iter().enumerate() {
        match runs.last_mut() {
            Some(r) if sections[r[0]].script == s.script => r.push(i),
            _ => runs.push(alloc::vec![i]),
        }
    }
    let lettered = |r: &Vec<usize>| sections[r[0]].script != Script::Other;
    let dashes = runs
        .iter()
        .filter(|r| lettered(r))
        .count()
        .saturating_sub(1);
    let room = max.saturating_sub(dashes);
    let mut budget: Vec<usize> = runs.iter().map(Vec::len).collect();
    // take one place at a time from the longest run until it fits
    while budget.iter().sum::<usize>() > room {
        let Some((i, _)) = budget
            .iter()
            .enumerate()
            .filter(|&(_, &b)| b > 1)
            .rev()
            .max_by_key(|&(_, &b)| b)
        else {
            break;
        };
        budget[i] -= 1;
    }
    let mut out = Vec::new();
    for (k, (r, &b)) in runs.iter().zip(&budget).enumerate() {
        if k > 0 && lettered(r) {
            out.push(Mark {
                text: DASH,
                sections: Vec::new(),
            });
        }
        let one = |i: usize| Mark {
            text: sections[i].letter,
            sections: alloc::vec![i],
        };
        if b >= r.len() {
            out.extend(r.iter().map(|&i| one(i)));
        } else {
            let head = b / 2;
            let tail = b - 1 - head;
            out.extend(r[..head].iter().map(|&i| one(i)));
            out.push(Mark {
                text: DOT,
                sections: r[head..r.len() - tail].to_vec(),
            });
            out.extend(r[r.len() - tail..].iter().map(|&i| one(i)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::person;

    fn book() -> Book {
        let mut b = Book::default();
        for (f, l) in [
            ("Ava", "Lindqvist"),
            ("Мария", "Ковалёва"),
            ("Ben", "Okafor"),
            ("Անի", "Պետրոսյան"),
            ("ava", "Adams"),
            ("Émile", "Zola"),
            ("Ёжик", ""),
            ("Արամ", "Հակոբյան"),
            ("911", ""),
            ("Zara", ""),
            ("Ելենա", ""),
            ("Анна", ""),
        ] {
            b.add(person(f, l, &["1"])).unwrap();
        }
        b
    }

    fn letters(s: &[Section]) -> String {
        s.iter().map(|s| s.letter).collect()
    }

    #[test]
    fn letters_per_script() {
        assert_eq!(letter('a'), (Script::Latin, 'A'));
        assert_eq!(letter('é'), (Script::Latin, 'E'));
        assert_eq!(letter('Ł'), (Script::Latin, 'L'));
        assert_eq!(letter('ś'), (Script::Latin, 'S'));
        assert_eq!(letter('ž'), (Script::Latin, 'Z'));
        assert_eq!(letter('ß'), (Script::Latin, 'S'));
        assert_eq!(letter('×'), (Script::Other, '#'));
        assert_eq!(letter('ա'), (Script::Armenian, 'Ա'));
        assert_eq!(letter('ֆ'), (Script::Armenian, 'Ֆ'));
        assert_eq!(letter('և'), (Script::Armenian, 'Ե'));
        assert_eq!(letter('я'), (Script::Cyrillic, 'Я'));
        assert_eq!(letter('ё'), (Script::Cyrillic, 'Е'));
        assert_eq!(letter('1'), (Script::Other, '#'));
        assert_eq!(letter('α'), (Script::Other, '#'));
        assert_eq!(LATIN_BASE.len(), 0x180 - 0xc0);
    }

    #[test]
    fn sections_follow_the_phones_language() {
        let b = book();
        let s = sections(&b, NameOrder::FirstLast, Script::Latin);
        assert_eq!(letters(&s), "ABEZԱԵАЕМ#");
        // "ava Adams" before "Ava Lindqvist": folded names
        let a: Vec<&str> = s[0]
            .ids
            .iter()
            .map(|&id| b.get(id).unwrap().last.as_str())
            .collect();
        assert_eq!(a, ["Adams", "Lindqvist"]);
        // Ёжик sits under Е
        assert_eq!(s[7].ids.len(), 1);
        assert_eq!(b.get(s[7].ids[0]).unwrap().first, "Ёжик");
        let s = sections(&b, NameOrder::FirstLast, Script::Cyrillic);
        assert_eq!(letters(&s), "АЕМABEZԱԵ#");
        let s = sections(&b, NameOrder::LastFirst, Script::Armenian);
        assert_eq!(letters(&s), "ԵՀՊALOZАЕК#");
        assert_eq!(
            sections(&Book::default(), NameOrder::FirstLast, Script::Latin),
            []
        );
    }

    fn text(m: &[Mark]) -> String {
        m.iter().map(|m| m.text).collect()
    }

    fn mixed(n: usize) -> Vec<Section> {
        let mut v = Vec::new();
        for (script, from, count) in [
            (Script::Latin, 'A', 26.min(n)),
            (Script::Armenian, 'Ա', 38.min(n)),
            (Script::Cyrillic, 'А', 32.min(n)),
        ] {
            for k in 0..count {
                v.push(Section {
                    script,
                    letter: char::from_u32(from as u32 + k as u32).unwrap(),
                    ids: alloc::vec![k as u32],
                });
            }
        }
        v.push(Section {
            script: Script::Other,
            letter: '#',
            ids: alloc::vec![0],
        });
        v
    }

    #[test]
    fn index_fits_or_folds() {
        let s = mixed(3);
        assert_eq!(text(&side_index(&s, 40)), "ABC–ԱԲԳ–АБВ#");
        let m = side_index(&s, 40);
        assert_eq!(m[3].sections, []);
        assert_eq!(m[4].sections, [3]);

        let s = mixed(40);
        let m = side_index(&s, 19);
        assert_eq!(m.len(), 19);
        // every section is reachable, once, in order
        let all: Vec<usize> = m.iter().flat_map(|m| m.sections.clone()).collect();
        assert_eq!(all, (0..s.len()).collect::<Vec<_>>());
        // each script keeps its first and last letter around one dot
        let t = text(&m);
        let parts: Vec<&str> = t.split(DASH).collect();
        assert_eq!(parts.len(), 3);
        for (p, (a, z)) in parts.iter().zip([('A', 'Z'), ('Ա', 'Ֆ'), ('А', 'Я')]) {
            assert!(p.starts_with(a), "{p}");
            assert_eq!(p.matches(DOT).count(), 1, "{p}");
            assert!(p.trim_end_matches('#').ends_with(z), "{p}");
        }
        // the dot remembers its letters
        let dot = m.iter().find(|m| m.text == DOT).unwrap();
        assert!(dot.sections.len() > 1);

        // too small: one dot per script
        let m = side_index(&s, 3);
        assert_eq!(text(&m), "•–•–•#");
        assert_eq!(side_index(&[], 10), []);
    }
}
