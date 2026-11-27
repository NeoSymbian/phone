// SPDX-License-Identifier: Apache-2.0
//! Recents (mockups 3, 5, 6, 27): the call log's rows for a filter chip, in day sections (Today, Yesterday,
//! Earlier) with repeated calls from one number folded into one row; new missed calls; why a call was blocked; a
//! row's calls for the details; the avatar's initials and tint.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use contacts::recents::group;
use contacts::{Contact, Label};
use modem_ipc::{LogEntry, log};
use phone_rules::block::{self, List};
use phone_rules::number;

/// The filter chips.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    All,
    Missed,
    Received,
    Dialled,
    Blocked,
}

impl Filter {
    pub const ALL: [Filter; 5] = [
        Filter::All,
        Filter::Missed,
        Filter::Received,
        Filter::Dialled,
        Filter::Blocked,
    ];

    pub fn from_index(i: i32) -> Self {
        usize::try_from(i)
            .ok()
            .and_then(|i| Self::ALL.get(i))
            .copied()
            .unwrap_or_default()
    }

    pub fn index(self) -> i32 {
        self as i32
    }

    /// The log kinds the chip shows (and Clear deletes): blocked calls only under Blocked, rejected with missed.
    pub fn kinds(self) -> &'static [u8] {
        match self {
            Filter::All => &[log::DIALLED, log::RECEIVED, log::MISSED, log::REJECTED],
            Filter::Missed => &[log::MISSED, log::REJECTED],
            Filter::Received => &[log::RECEIVED],
            Filter::Dialled => &[log::DIALLED],
            Filter::Blocked => &[log::BLOCKED],
        }
    }
}

/// What Recents asks before it deletes or blocks (the shell's destructive QUERY): ⋮ ▸ Clear list for a chip, and
/// selection mode's Delete and Block for `n` rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    Clear(Filter),
    Delete(usize),
    Block(usize),
}

impl Ask {
    /// The question, the line under it, the red button.
    pub fn text(self) -> (String, &'static str, &'static str) {
        let s = |n: usize| if n == 1 { "" } else { "s" };
        match self {
            Ask::Clear(f) => (
                "Clear the call list?".into(),
                match f {
                    Filter::All => "Every call in Recents is deleted.",
                    Filter::Missed => "The missed calls are deleted.",
                    Filter::Received => "The received calls are deleted.",
                    Filter::Dialled => "The dialled calls are deleted.",
                    Filter::Blocked => "The blocked calls are deleted.",
                },
                "Clear",
            ),
            Ask::Delete(n) => (format!("Delete {n} call{}?", s(n)), "", "Delete"),
            Ask::Block(n) => (
                format!("Block {n} number{}?", s(n)),
                "Their calls and messages won't come through.",
                "Block",
            ),
        }
    }
}

/// A day band.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Section {
    Today,
    Yesterday,
    Earlier,
}

impl Section {
    pub fn of(utc_ms: i64, now_ms: i64, offset_s: i32) -> Self {
        let day = |ms: i64| (ms.div_euclid(1000) + i64::from(offset_s)).div_euclid(86_400);
        match day(now_ms) - day(utc_ms) {
            i64::MIN..=0 => Section::Today,
            1 => Section::Yesterday,
            _ => Section::Earlier,
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            Section::Today => "Today",
            Section::Yesterday => "Yesterday",
            Section::Earlier => "Earlier",
        }
    }
}

/// One row: `log[first]` is its newest call, `count` calls from that number in the section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub first: usize,
    pub count: u32,
    pub section: Section,
}

/// The rows of `log` (newest first) under `filter`, folded per number inside each day section.
pub fn rows(log: &[LogEntry], filter: Filter, now_ms: i64, offset_s: i32) -> Vec<Row> {
    let shown: Vec<usize> = (0..log.len())
        .filter(|&i| filter.kinds().contains(&log[i].kind))
        .collect();
    let mut out = Vec::new();
    for section in [Section::Today, Section::Yesterday, Section::Earlier] {
        let part: Vec<usize> = shown
            .iter()
            .copied()
            .filter(|&i| Section::of(log[i].utc, now_ms, offset_s) == section)
            .collect();
        for (i, count) in group(part.iter().map(|&i| log[i].number.as_str())) {
            out.push(Row {
                first: part[i],
                count,
                section,
            });
        }
    }
    out
}

/// The time on a Recents row: "21:03" under Today and Yesterday (the band says the day), else the date.
pub fn row_when(utc_ms: i64, now_ms: i64, offset_s: i32) -> String {
    if Section::of(utc_ms, now_ms, offset_s) == Section::Earlier || utc_ms == 0 {
        return contacts::recents::when(utc_ms, now_ms, offset_s);
    }
    contacts::recents::when(utc_ms, utc_ms, offset_s)
}

/// The dialer's short list (mockup 3): one row per number over the whole log, blocked calls left out.
pub fn recent(log: &[LogEntry]) -> Vec<(usize, u32)> {
    let shown: Vec<usize> = (0..log.len())
        .filter(|&i| log[i].kind != log::BLOCKED)
        .collect();
    group(shown.iter().map(|&i| log[i].number.as_str()))
        .into_iter()
        .map(|(i, c)| (shown[i], c))
        .collect()
}

/// Missed calls newer than `seen` (the Missed chip's count).
pub fn new_missed(log: &[LogEntry], seen: u32) -> u32 {
    log.iter()
        .filter(|e| e.kind == log::MISSED && e.id > seen)
        .count() as u32
}

/// The shade's missed-calls card (top-panel TP2b): "2 missed calls" over its callers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissedCard {
    pub title: String,
    /// the callers, newest first, a repeated one once with its count: "Ben Okafor (2), +1 555 0123"
    pub text: String,
    /// the newest caller's number, for Call back and Message ("" when hidden)
    pub number: String,
    /// the newest missed call's id (a newer one posts the card again) and time
    pub newest: u32,
    pub utc: i64,
}

/// A tap on the missed-calls card's action with the card as it was posted (never recomputed at tap time: a newer
/// missed call may have come since): action 1 Call back, 2 Message, to the number the card named. `None` for no card
/// posted, a hidden number or another action.
#[must_use]
pub fn missed_action(posted: Option<&MissedCard>, action: u16) -> Option<(bool, &str)> {
    let c = posted.filter(|c| !c.number.is_empty())?;
    match action {
        1 => Some((true, c.number.as_str())),
        2 => Some((false, c.number.as_str())),
        _ => None,
    }
}

/// The card for the missed calls newer than `seen` (None: no card). `title_of` names a call's caller as Recents does
/// (a contact, the number, "Private number"); the text is cut to `max` bytes.
pub fn missed_card(
    log: &[LogEntry],
    seen: u32,
    title_of: impl Fn(&LogEntry) -> String,
    max: usize,
) -> Option<MissedCard> {
    let new: Vec<&LogEntry> = log
        .iter()
        .filter(|e| e.kind == log::MISSED && e.id > seen)
        .collect();
    let last = *new.iter().max_by_key(|e| e.id)?;
    let mut who: Vec<(String, u32)> = Vec::new();
    for e in &new {
        let t = title_of(e);
        match who.iter_mut().find(|(w, _)| *w == t) {
            Some((_, n)) => *n += 1,
            None => who.push((t, 1)),
        }
    }
    let text: Vec<String> = who
        .iter()
        .map(|(w, n)| {
            if *n > 1 {
                format!("{w} ({n})")
            } else {
                w.clone()
            }
        })
        .collect();
    let hidden = last.presentation == phone_rules::caller::presentation::RESTRICTED;
    Some(MissedCard {
        title: format!(
            "{} missed call{}",
            new.len(),
            if new.len() == 1 { "" } else { "s" }
        ),
        text: crate::messages::cut(&text.join(", "), max).into(),
        number: if hidden {
            String::new()
        } else {
            last.number.as_str().into()
        },
        newest: last.id,
        utc: last.utc,
    })
}

/// The newest entry's id (seeing Recents marks the missed calls up to it as seen).
pub fn newest(log: &[LogEntry]) -> u32 {
    log.iter().map(|e| e.id).max().unwrap_or(0)
}

/// The number dialled last (the log is newest first): Call on an empty dialer puts it in the field (N8).
pub fn last_dialled(log: &[LogEntry]) -> Option<&str> {
    log.iter()
        .filter(|e| e.kind == log::DIALLED)
        .max_by_key(|e| e.id)
        .map(|e| e.number.as_str())
        .filter(|n| !n.is_empty())
}

/// Every call of the row `id` belongs to (the same number), newest first: the details' list.
pub fn calls_of(log: &[LogEntry], id: u32) -> Vec<usize> {
    let Some(e) = log.iter().find(|e| e.id == id) else {
        return Vec::new();
    };
    let digits = number::digits(e.number.as_str());
    if digits.is_empty() {
        return log.iter().position(|x| x.id == id).into_iter().collect();
    }
    (0..log.len())
        .filter(|&i| number::digits(log[i].number.as_str()) == digits)
        .collect()
}

/// The direction's word.
pub fn kind_text(kind: u8) -> &'static str {
    match kind {
        log::DIALLED => "Dialled",
        log::RECEIVED => "Received",
        log::REJECTED => "Rejected",
        log::BLOCKED => "Blocked",
        _ => "Missed",
    }
}

/// The label of `contact`'s number `n` ("Mobile", "Work"), if it has one that matches.
pub fn label_of(contact: &Contact, n: &str) -> Option<Label> {
    contact
        .numbers
        .iter()
        .find(|(_, x)| number::matches(x, n))
        .map(|(l, _)| *l)
}

/// The second line's word: a contact's received or dialled call its number's label (the glyph gives the
/// direction), anything else its kind.
pub fn second_line(e: &LogEntry, label: Option<Label>) -> String {
    match (e.kind, label) {
        (log::RECEIVED | log::DIALLED, Some(l)) => l.text().into(),
        (k, _) => kind_text(k).into(),
    }
}

/// The header's search on Recents: `query` in the row's name (any case) or in its number's digits.
pub fn found(title: &str, n: &str, query: &str) -> bool {
    let q = query.trim();
    if q.is_empty() {
        return true;
    }
    let digits = number::digits(q);
    title.to_lowercase().contains(&q.to_lowercase())
        || (!digits.is_empty() && number::digits(n).contains(digits.as_str()))
}

/// Why a blocked call was blocked (mockup 27): the rule that matched.
pub fn why_text(e: &LogEntry, list: &List, known: bool) -> String {
    match e.why {
        block::why::NUMBER if known => "Blocked contact".into(),
        block::why::NUMBER => "Blocked number".into(),
        block::why::PREFIX => {
            let digits = number::digits(e.number.as_str());
            list.rules
                .iter()
                .filter(|r| r.prefix)
                .find(|r| {
                    let p = number::digits(&r.number);
                    !p.is_empty() && digits.starts_with(&p)
                })
                .map_or_else(|| "Prefix".into(), |r| format!("Prefix {}", r.number))
        }
        block::why::PRIVATE => "Private numbers".into(),
        block::why::UNKNOWN => "Unknown numbers".into(),
        _ => "Blocked".into(),
    }
}

/// Up to two initials of a name ("Ava Lindqvist" → "AL", "Անի Պետրոսյան" → "ԱՊ"); "" for a number.
pub fn initials(name: &str) -> String {
    let mut out = String::new();
    for w in name.split_whitespace().take(2) {
        if let Some(c) = w.chars().next().filter(|c| c.is_alphabetic()) {
            out.extend(c.to_uppercase());
        }
    }
    out
}

/// A stable tint index (0..`n`) for a contact id or, without one, a number's digits.
pub fn tint(contact: Option<u32>, n: &str, count: u32) -> i32 {
    let h = match contact {
        Some(id) => id,
        None => number::digits(n)
            .bytes()
            .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b))),
    };
    (h % count.max(1)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use modem_ipc::Text;

    const NOW: i64 = 1_794_690_180_000; // 2026-11-14 21:03 UTC
    const H: i64 = 3_600_000;

    #[test]
    fn missed_actions_use_the_card_as_posted() {
        let card = MissedCard {
            title: "2 missed calls".into(),
            text: "Ben, Ava".into(),
            number: "+15550123".into(),
            newest: 7,
            utc: 0,
        };
        assert_eq!(missed_action(Some(&card), 1), Some((true, "+15550123")));
        assert_eq!(missed_action(Some(&card), 2), Some((false, "+15550123")));
        assert_eq!(missed_action(Some(&card), 3), None);
        assert_eq!(missed_action(None, 1), None);
        let hidden = MissedCard {
            number: String::new(),
            ..card
        };
        assert_eq!(missed_action(Some(&hidden), 1), None);
    }

    fn e(id: u32, kind: u8, n: &str, ago_h: i64) -> LogEntry {
        LogEntry {
            id,
            utc: NOW - ago_h * H,
            kind,
            number: Text::new(n),
            ..LogEntry::default()
        }
    }

    fn log() -> Vec<LogEntry> {
        alloc::vec![
            e(9, log::RECEIVED, "+15550142", 0),
            e(8, log::MISSED, "+15550171", 1),
            e(7, log::RECEIVED, "+15550142", 2),
            e(6, log::BLOCKED, "+19005550101", 3),
            e(5, log::DIALLED, "+15550123", 30),
            e(4, log::RECEIVED, "+15550142", 30),
            e(3, log::RECEIVED, "+15550188", 100),
        ]
    }

    #[test]
    fn questions() {
        assert_eq!(
            Ask::Clear(Filter::All).text(),
            (
                "Clear the call list?".into(),
                "Every call in Recents is deleted.",
                "Clear"
            )
        );
        assert_eq!(Ask::Delete(1).text().0, "Delete 1 call?");
        assert_eq!(Ask::Delete(3).text().0, "Delete 3 calls?");
        assert_eq!(Ask::Block(2).text().0, "Block 2 numbers?");
    }

    #[test]
    fn last_dialled_number() {
        assert_eq!(last_dialled(&log()), Some("+15550123"));
        assert_eq!(last_dialled(&log()[..4]), None);
    }

    #[test]
    fn folded_per_section() {
        let l = log();
        let r = rows(&l, Filter::All, NOW, 0);
        let shape: Vec<(u32, u32, Section)> = r
            .iter()
            .map(|r| (l[r.first].id, r.count, r.section))
            .collect();
        assert_eq!(
            shape,
            [
                (9, 2, Section::Today),
                (8, 1, Section::Today),
                (5, 1, Section::Yesterday),
                (4, 1, Section::Yesterday),
                (3, 1, Section::Earlier),
            ]
        );
        assert_eq!(rows(&l, Filter::Blocked, NOW, 0).len(), 1);
        assert_eq!(rows(&l, Filter::Missed, NOW, 0).len(), 1);
        assert_eq!(recent(&l)[0], (0, 3));
        assert_eq!(calls_of(&l, 7), [0, 2, 5]);
    }

    #[test]
    fn missed_and_words() {
        let l = log();
        assert_eq!(row_when(l[4].utc, NOW, 0), "15:03");
        assert_eq!(row_when(l[6].utc, NOW, 0), "10 Nov");
        assert_eq!(new_missed(&l, 0), 1);
        assert_eq!(new_missed(&l, 8), 0);
        assert_eq!(newest(&l), 9);
        assert_eq!(initials("Ava Lindqvist"), "AL");
        assert_eq!(initials("Անի Պետրոսյան"), "ԱՊ");
        assert_eq!(initials("+1 555"), "");
        assert_eq!(second_line(&l[0], Some(Label::Mobile)), "Mobile");
        assert_eq!(second_line(&l[1], Some(Label::Mobile)), "Missed");
        assert_eq!(
            second_line(&l[4], Some(Label::MobileWork)),
            Label::MobileWork.text()
        );
        assert_eq!(second_line(&l[4], None), "Dialled");
        assert!(found("Ava Lindqvist", "+15550142", "lind"));
        assert!(found("Ava Lindqvist", "+15550142", "0142"));
        assert!(!found("Ava Lindqvist", "+15550142", "boris"));
        assert!(found("Борис", "", "бор"));
        assert_eq!(tint(None, "+15550142", 8), tint(None, "15550142", 8));
    }

    #[test]
    fn the_missed_calls_card() {
        let name = |e: &LogEntry| match e.number.as_str() {
            "+15550171" => String::from("Ben Okafor"),
            "" => String::from("Private number"),
            n => String::from(n),
        };
        let l = log();
        // one caller
        let c = missed_card(&l, 0, name, 160).unwrap();
        assert_eq!(
            (
                c.title.as_str(),
                c.text.as_str(),
                c.number.as_str(),
                c.newest,
                c.utc
            ),
            ("1 missed call", "Ben Okafor", "+15550171", 8, l[1].utc)
        );
        // seen: no card
        assert_eq!(missed_card(&l, 8, name, 160), None);
        // two callers, newest first (top-panel.md's example)
        let mut l = log();
        l.insert(0, e(10, log::MISSED, "+1 555 0123", 0));
        let c = missed_card(&l, 0, name, 160).unwrap();
        assert_eq!(
            (c.title.as_str(), c.text.as_str(), c.number.as_str()),
            ("2 missed calls", "+1 555 0123, Ben Okafor", "+1 555 0123")
        );
        // many: a repeated caller once with its count, a hidden one without a number, the text cut to fit
        l.insert(0, e(11, log::MISSED, "+15550171", 0));
        let mut p = e(12, log::MISSED, "", 0);
        p.presentation = phone_rules::caller::presentation::RESTRICTED;
        l.insert(0, p);
        let c = missed_card(&l, 0, name, 160).unwrap();
        assert_eq!(
            (
                c.title.as_str(),
                c.text.as_str(),
                c.number.as_str(),
                c.newest
            ),
            (
                "4 missed calls",
                "Private number, Ben Okafor (2), +1 555 0123",
                "",
                12
            )
        );
        assert_eq!(
            missed_card(&l, 0, name, 20).unwrap().text,
            "Private number, Ben "
        );
        // rejected and other calls don't count
        assert_eq!(
            missed_card(&l, 0, name, 160).unwrap().title,
            "4 missed calls"
        );
        l.insert(0, e(13, log::REJECTED, "+15550188", 0));
        assert_eq!(missed_card(&l, 12, name, 160), None);
    }

    #[test]
    fn why_blocked() {
        let list = List {
            rules: alloc::vec![block::Rule {
                prefix: true,
                calls: true,
                messages: false,
                number: "+1 900".into(),
            }],
            ..List::default()
        };
        let mut b = e(1, log::BLOCKED, "+19005550101", 0);
        b.why = block::why::PREFIX;
        assert_eq!(why_text(&b, &list, false), "Prefix +1 900");
        b.why = block::why::PRIVATE;
        assert_eq!(why_text(&b, &list, false), "Private numbers");
        b.why = block::why::NUMBER;
        assert_eq!(why_text(&b, &list, true), "Blocked contact");
    }
}
