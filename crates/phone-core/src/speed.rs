// SPDX-License-Identifier: Apache-2.0
//! Speed dial (mockup 7): keys 2..=9 hold numbers (`phone.speed`: 8 lines, "" = unassigned), key 1 is the voice
//! mailbox. The grid's tiles show who each number belongs to.

use alloc::string::String;
use contacts::{Book, NameOrder};

/// The keys' numbers, key 2 first.
pub type Cells = [String; 8];

/// The number on key `k` (2..=9), if assigned.
pub fn number(cells: &Cells, k: usize) -> Option<&str> {
    k.checked_sub(2)
        .and_then(|i| cells.get(i))
        .map(String::as_str)
        .filter(|n| !n.is_empty())
}

/// `cells` with key `k` (2..=9) set to `number` ("" removes it); `None` for another key.
pub fn with(cells: &Cells, k: usize, number: &str) -> Option<Cells> {
    let i = k.checked_sub(2).filter(|i| *i < 8)?;
    let mut out = cells.clone();
    out[i] = number.into();
    Some(out)
}

/// The tile's name: "Ava L." for a contact, else the number.
pub fn short_name(book: &Book, order: NameOrder, n: &str) -> String {
    let Some(c) = book.lookup(n) else {
        return n.into();
    };
    let title = c.title(order);
    let mut words = title.split_whitespace();
    match (words.next(), words.next()) {
        (Some(a), Some(b)) => {
            let mut s = String::from(a);
            s.push(' ');
            s.extend(b.chars().next());
            s.push('.');
            s
        }
        _ => title,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use contacts::{Contact, Label};

    #[test]
    fn keys() {
        let cells: Cells = Default::default();
        assert_eq!(number(&cells, 2), None);
        let cells = with(&cells, 5, "+15550142").unwrap();
        assert_eq!(number(&cells, 5), Some("+15550142"));
        assert!(with(&cells, 1, "x").is_none());
        assert!(with(&cells, 10, "x").is_none());
        assert_eq!(number(&with(&cells, 5, "").unwrap(), 5), None);
    }

    #[test]
    fn names() {
        let mut b = Book::default();
        b.add(Contact {
            first: "Ава".into(),
            last: "Линдквист".into(),
            numbers: alloc::vec![(Label::Mobile, "+15550142".into())],
            ..Contact::default()
        })
        .unwrap();
        assert_eq!(short_name(&b, NameOrder::FirstLast, "+15550142"), "Ава Л.");
        assert_eq!(
            short_name(&b, NameOrder::FirstLast, "+15550199"),
            "+15550199"
        );
    }
}
