// SPDX-License-Identifier: Apache-2.0
//! Messages (mockups 28-36): the app's copy of MODEMD's message store ([`Mirror`], kept up to date with
//! modem_ipc::op::SMS_READ), the conversations ([`threads`]: one per person or number, newest first), a thread's
//! bubbles with their day bands, the blocked messages, the composer's counter, the To field's recipients.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use contacts::{Book, NameOrder};
use modem_ipc::{SmsEntry, sms as st};
use phone_rules::block::why;
use phone_rules::number;

/// One message as the app keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Msg {
    pub entry: SmsEntry,
    pub text: String,
}

impl Msg {
    pub fn number(&self) -> &str {
        self.entry.number.as_str()
    }
}

/// The same person: the same text, or the same subscriber's number (MODEMD's rule too).
pub fn same(a: &str, b: &str) -> bool {
    a == b || number::matches(a, b)
}

/// MODEMD's messages as the app last read them: oldest first (by id), no tombstones.
#[derive(Clone, Debug, Default)]
pub struct Mirror {
    pub msgs: Vec<Msg>,
    /// the store revision read up to (SMS_READ's arg 1)
    pub rev: u32,
}

impl Mirror {
    /// Take one SMS_READ answer: the records changed since `rev`. The records applied (0: up to date).
    pub fn apply(&mut self, mut b: &[u8]) -> usize {
        let mut n = 0;
        while let Some((e, text, used)) = SmsEntry::read(b) {
            b = &b[used..];
            n += 1;
            self.rev = self.rev.max(e.rev);
            let at = self.msgs.binary_search_by_key(&e.id, |m| m.entry.id);
            match (at, e.state == st::DELETED) {
                (Ok(i), true) => {
                    self.msgs.remove(i);
                }
                (Ok(i), false) => {
                    self.msgs[i] = Msg {
                        entry: e,
                        text: String::from(text),
                    }
                }
                (Err(i), false) => self.msgs.insert(
                    i,
                    Msg {
                        entry: e,
                        text: String::from(text),
                    },
                ),
                (Err(_), true) => {}
            }
        }
        n
    }

    /// Start over (SMS_READ said the deletions since `rev` are forgotten).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn get(&self, id: u32) -> Option<&Msg> {
        self.msgs
            .binary_search_by_key(&id, |m| m.entry.id)
            .ok()
            .map(|i| &self.msgs[i])
    }
}

/// A conversation in the list (mockup 28).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thread {
    /// the number (or sender name) of its newest message: the thread's key
    pub number: String,
    /// the contact's name, else the number
    pub name: String,
    pub contact: Option<u32>,
    pub unread: u32,
    /// the newest message: its index in the mirror, its text with "You: " for ours
    pub last: usize,
    pub snippet: String,
    pub failed: bool,
    pub utc: i64,
}

/// Who `number` is: the contact's name and id, else the number itself.
pub fn who(book: &Book, order: NameOrder, number: &str) -> (String, Option<u32>) {
    match book.lookup(number) {
        Some(c) => (c.title(order), Some(c.id)),
        None => (String::from(number), None),
    }
}

/// The conversations, newest first. Blocked messages are not in them.
pub fn threads(m: &Mirror, book: &Book, order: NameOrder) -> Vec<Thread> {
    let mut out: Vec<Thread> = Vec::new();
    for (i, msg) in m.msgs.iter().enumerate().rev() {
        if msg.entry.state == st::BLOCKED {
            continue;
        }
        let unread = u32::from(msg.entry.state == st::UNREAD);
        if let Some(t) = out.iter_mut().find(|t| same(&t.number, msg.number())) {
            t.unread += unread;
            continue;
        }
        let (name, contact) = who(book, order, msg.number());
        out.push(Thread {
            number: String::from(msg.number()),
            name,
            contact,
            unread,
            last: i,
            snippet: snippet(msg),
            failed: msg.entry.state == st::FAILED,
            utc: msg.entry.utc,
        });
    }
    out
}

/// The list's second line: the text on one line, "You: " before ours.
fn snippet(m: &Msg) -> String {
    let line: String = m
        .text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(80)
        .collect();
    if m.entry.incoming {
        line
    } else {
        format!("You: {line}")
    }
}

/// The conversations whose name, number or any message matches `query` (case folded).
pub fn search(m: &Mirror, threads: &[Thread], query: &str) -> Vec<usize> {
    let q = contacts::fold(query.trim());
    if q.is_empty() {
        return (0..threads.len()).collect();
    }
    let digits = number::digits(query);
    threads
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            contacts::fold(&t.name).contains(&q)
                || (!digits.is_empty() && number::digits(&t.number).contains(&digits))
                || m.msgs.iter().any(|msg| {
                    msg.entry.state != st::BLOCKED
                        && same(msg.number(), &t.number)
                        && contacts::fold(&msg.text).contains(&q)
                })
        })
        .map(|(i, _)| i)
        .collect()
}

/// A thread's messages (indexes into the mirror), oldest first.
pub fn thread(m: &Mirror, number: &str) -> Vec<usize> {
    m.msgs
        .iter()
        .enumerate()
        .filter(|(_, msg)| msg.entry.state != st::BLOCKED && same(msg.number(), number))
        .map(|(i, _)| i)
        .collect()
}

/// The blocked messages, newest first.
pub fn blocked(m: &Mirror) -> Vec<usize> {
    let mut v: Vec<usize> = (0..m.msgs.len())
        .filter(|&i| m.msgs[i].entry.state == st::BLOCKED)
        .collect();
    v.reverse();
    v
}

/// The local day of UTC milliseconds.
fn day(utc_ms: i64, offset_s: i32) -> i64 {
    (utc_ms.div_euclid(1000) + i64::from(offset_s)).div_euclid(86_400)
}

/// The band before each bubble: "Today", "Yesterday", "12 Nov" when the day changes, else "".
pub fn bands(m: &Mirror, idx: &[usize], now_ms: i64, offset_s: i32) -> Vec<String> {
    let mut last = None;
    idx.iter()
        .map(|&i| {
            let utc = m.msgs[i].entry.utc;
            let d = day(utc, offset_s);
            if utc == 0 || last == Some(d) {
                return String::new();
            }
            last = Some(d);
            match day(now_ms, offset_s) - d {
                0 => String::from("Today"),
                1 => String::from("Yesterday"),
                _ => {
                    let when = contacts::recents::when_long(utc, now_ms, offset_s);
                    // "14 Nov 2026 21:03" → "14 Nov 2026"
                    String::from(when.rsplit_once(' ').map_or(when.as_str(), |(d, _)| d))
                }
            }
        })
        .collect()
}

/// The bubble's time ("09:53").
pub fn time(utc_ms: i64, offset_s: i32) -> String {
    if utc_ms == 0 {
        return String::new();
    }
    let min = (utc_ms.div_euclid(1000) + i64::from(offset_s)).rem_euclid(86_400) / 60;
    format!("{:02}:{:02}", min / 60, min % 60)
}

/// The composer's counter while typing (mockup 29): "138 left · 1 SMS", "Unicode · 57 left · 1 SMS"; "" for an
/// empty text, "Too long" beyond 10 parts.
pub fn counter(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let c = sms::count(text);
    if c.too_long() {
        return String::from("Too long");
    }
    let n = if c.parts == 1 {
        String::from("1 SMS")
    } else {
        format!("{} SMS", c.parts)
    };
    if c.unicode {
        format!("Unicode · {} left · {n}", c.left)
    } else {
        format!("{} left · {n}", c.left)
    }
}

/// Text that can be sent: not empty, not only spaces, at most 10 parts.
pub fn sendable(text: &str) -> bool {
    !text.trim().is_empty() && !sms::count(text).too_long()
}

/// Why a message was blocked (mockup 35): "Blocked contact", "Number", "Prefix".
pub fn why_text(why: u8, known: bool) -> &'static str {
    match why {
        _ if known => "Blocked contact",
        why::PREFIX => "Blocked prefix",
        _ => "Blocked number",
    }
}

/// The details of a message (mockup 31): (label, value) lines.
pub fn details(msg: &Msg, name: &str, now_ms: i64, offset_s: i32) -> Vec<(&'static str, String)> {
    let e = &msg.entry;
    let when = |utc| contacts::recents::when_long(utc, now_ms, offset_s);
    let party = if name == msg.number() {
        String::from(name)
    } else {
        format!("{name} · {}", msg.number())
    };
    let mut v = alloc::vec![(if e.incoming { "From" } else { "To" }, party)];
    if e.incoming {
        v.push(("Received", when(e.utc)));
    } else {
        v.push(("Written", when(e.utc)));
        v.push((
            "Status",
            String::from(match e.state {
                st::SENDING => "Sending",
                st::SENT => "Sent",
                st::DELIVERED => "Delivered",
                _ => "Not sent",
            }),
        ));
        if e.done_utc != 0 {
            v.push(("Delivered", when(e.done_utc)));
        }
    }
    let chars = msg.text.chars().count();
    v.push((
        "Size",
        format!(
            "{} SMS · {chars} characters · {}",
            e.parts.max(1),
            if e.unicode { "Unicode" } else { "GSM" }
        ),
    ));
    v
}

/// A recipient being typed in To: a number when it is one (digits, +, spaces, dashes), else a name to look up.
pub fn as_number(typed: &str) -> Option<String> {
    let t = typed.trim();
    let ok = !t.is_empty()
        && t.chars().enumerate().all(|(i, c)| {
            c.is_ascii_digit() || (c == '+' && i == 0) || matches!(c, ' ' | '-' | '(' | ')')
        })
        && number::digits(t).len() >= 3;
    ok.then(|| String::from(t))
}

/// The contacts' numbers matching what is typed in To: (contact id, name, label, number), at most `max`.
pub fn suggestions(
    book: &Book,
    order: NameOrder,
    typed: &str,
    max: usize,
) -> Vec<(u32, String, &'static str, String)> {
    let t = typed.trim();
    if t.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for id in book.search(t, order) {
        let Some(c) = book.get(id) else { continue };
        for (label, n) in &c.numbers {
            if out.len() >= max {
                return out;
            }
            out.push((id, c.title(order), label.text(), n.clone()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use contacts::{Contact, Label};
    use modem_ipc::Text;

    const T0: i64 = 1_794_722_700_000; // 2026-11-15 06:05 UTC

    fn rec(
        id: u32,
        rev: u32,
        number: &str,
        state: u8,
        incoming: bool,
        text: &str,
        out: &mut Vec<u8>,
    ) {
        let e = SmsEntry {
            id,
            rev,
            utc: T0 + i64::from(id) * 60_000,
            state,
            parts: 1,
            incoming,
            number: Text::new(number),
            ..SmsEntry::default()
        };
        let mut b = alloc::vec![0u8; SmsEntry::HEAD + 2 + text.len()];
        let n = e.write(text, &mut b).unwrap();
        out.extend_from_slice(&b[..n]);
    }

    fn book() -> Book {
        let mut b = Book::default();
        b.add(Contact {
            first: "Ava".into(),
            last: "Lindqvist".into(),
            numbers: alloc::vec![(Label::Mobile, "+37491234567".into())],
            ..Contact::default()
        })
        .unwrap();
        b
    }

    fn mirror() -> Mirror {
        let mut b = Vec::new();
        rec(
            1,
            1,
            "+37491234567",
            st::READ,
            true,
            "Hey! Dinner tonight?",
            &mut b,
        );
        rec(2, 2, "091234567", st::DELIVERED, false, "Yes. 7?", &mut b);
        rec(3, 3, "Ucom", st::UNREAD, true, "Your code is 4821", &mut b);
        rec(4, 4, "+19005550101", st::BLOCKED, true, "WIN", &mut b);
        rec(
            5,
            5,
            "+37491234567",
            st::UNREAD,
            true,
            "Running late",
            &mut b,
        );
        let mut m = Mirror::default();
        assert_eq!(m.apply(&b), 5);
        m
    }

    #[test]
    fn threads_group_numbers_and_skip_blocked() {
        let m = mirror();
        let t = threads(&m, &book(), NameOrder::FirstLast);
        assert_eq!(t.len(), 2);
        assert_eq!(
            (t[0].name.as_str(), t[0].unread, t[0].contact.is_some()),
            ("Ava Lindqvist", 1, true)
        );
        assert_eq!(t[0].snippet, "Running late");
        assert_eq!((t[1].name.as_str(), t[1].unread), ("Ucom", 1));
        assert_eq!(thread(&m, "+37491234567"), [0, 1, 4]);
        assert_eq!(blocked(&m), [3]);
        assert_eq!(search(&m, &t, "dinner"), [0]);
        assert_eq!(search(&m, &t, "4821"), [1]);
        assert_eq!(search(&m, &t, ""), [0, 1]);
    }

    #[test]
    fn updates_and_deletions_apply() {
        let mut m = mirror();
        let mut b = Vec::new();
        rec(5, 6, "+37491234567", st::READ, true, "Running late", &mut b);
        rec(3, 7, "Ucom", st::DELETED, true, "", &mut b);
        assert_eq!(m.apply(&b), 2);
        assert_eq!((m.rev, m.msgs.len()), (7, 4));
        assert_eq!(m.get(5).unwrap().entry.state, st::READ);
        assert!(m.get(3).is_none());
        assert_eq!(m.apply(&[]), 0);
    }

    #[test]
    fn counter_and_recipients() {
        assert_eq!(counter(""), "");
        assert_eq!(counter("Hi"), "158 left · 1 SMS");
        assert_eq!(counter("Вижу тебя в 7"), "Unicode · 57 left · 1 SMS");
        assert_eq!(counter(&"a".repeat(300)), "6 left · 2 SMS");
        assert_eq!(counter(&"a".repeat(2000)), "Too long");
        assert!(!sendable("  "));
        assert_eq!(as_number(" +374 91 23 "), Some(String::from("+374 91 23")));
        assert_eq!(as_number("Ava"), None);
        assert_eq!(as_number("12"), None);
        let s = suggestions(&book(), NameOrder::FirstLast, "av", 5);
        assert_eq!(s[0].1, "Ava Lindqvist");
        assert_eq!(s[0].3, "+37491234567");
    }

    #[test]
    fn bands_and_details() {
        let m = mirror();
        let idx = thread(&m, "+37491234567");
        let b = bands(&m, &idx, T0 + 3_600_000, 4 * 3600);
        assert_eq!(b, ["Today", "", ""]);
        let b = bands(&m, &idx, T0 + 86_400_000, 4 * 3600);
        assert_eq!(b[0], "Yesterday");
        assert_eq!(time(T0, 4 * 3600), "10:05");
        let d = details(&m.msgs[1], "Ava Lindqvist", T0, 4 * 3600);
        assert_eq!(d[0], ("To", String::from("Ava Lindqvist · 091234567")));
        assert_eq!(d[2], ("Status", String::from("Delivered")));
        assert!(
            d.last()
                .unwrap()
                .1
                .starts_with("1 SMS · 7 characters · GSM")
        );
        assert_eq!(why_text(why::PREFIX, false), "Blocked prefix");
    }
}
