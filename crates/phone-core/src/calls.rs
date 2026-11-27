// SPDX-License-Identifier: Apache-2.0
//! Which call screen shows, from MODEMD's call list (`Status::calls`, the authoritative one), and the calls' times.
//!
//! [`Tracker::update`] runs on every status: it notes when each call connected (for the timers), which calls are
//! new (the call screens open for them) and, when the last call goes, that call for the summary. [`Tracker::screen`]
//! picks the screen: incoming, calling, active, waiting, two calls, ended.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use modem_ipc::{Call, Status, call as st};

/// A call the screens show: ringing, being set up, connected or held (not ending or gone).
pub fn shown(c: &Call) -> bool {
    !matches!(c.state, st::NONE | st::DISCONNECTING | st::END)
}

/// Ringing for us: a new call (incoming, waiting, or an incoming one being set up).
pub fn ringing(c: &Call) -> bool {
    matches!(c.state, st::INCOMING | st::WAITING) || (c.state == st::SETUP && c.incoming)
}

/// Ours, not yet answered: dialling, the network setting it up, the other side ringing.
pub fn outgoing(c: &Call) -> bool {
    matches!(c.state, st::ORIGINATION | st::CC_IN_PROGRESS | st::ALERTING)
        || (c.state == st::SETUP && !c.incoming)
}

/// Connected: talking or on hold.
pub fn connected(c: &Call) -> bool {
    matches!(c.state, st::CONVERSATION | st::HOLD)
}

/// The call that just ended (the summary).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ended {
    pub call: Call,
    /// how long it was connected; `None`: it never connected
    pub seconds: Option<u32>,
    /// when it ended (ms)
    pub at: u64,
}

impl Ended {
    /// Worth a summary (when the setting is on): a call that connected, or one of ours that wasn't answered (Call
    /// again). A missed or rejected incoming call just closes.
    pub fn summary(&self) -> bool {
        self.seconds.is_some() || !self.call.incoming
    }
}

/// The call screen and what it shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    None,
    /// screen 9: a call rings and no other call is up
    Incoming(Call),
    /// screen 10: our call, not yet answered (`state` ALERTING: the other side rings)
    Calling(Call),
    /// screen 11: one connected call (talking, or on hold)
    Active(Call),
    /// screen 12: a second caller while `active` (talking or held) is up
    Waiting {
        active: Call,
        waiting: Call,
    },
    /// screen 13: two connected calls; `held` is the slim bar on top (tap: swap)
    Two {
        active: Call,
        held: Call,
    },
    /// p2-call-conference: every connected call is part of one conference (after Merge); `first` connected first
    Conference {
        first: Call,
        count: u8,
    },
    /// screen 15: the summary after the last call
    Ended(Ended),
}

/// What changed with a status.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Update {
    /// a call appeared that wasn't there before (the call screens open)
    pub appeared: bool,
    /// the last call went (summary or close)
    pub all_ended: bool,
}

/// The calls between statuses.
#[derive(Clone, Debug, Default)]
pub struct Tracker {
    /// the calls shown at the last status
    last: Vec<Call>,
    /// when each call connected (id, ms)
    since: Vec<(u8, u64)>,
    /// the call that went most recently (the summary's, if it was the last)
    gone: Option<Ended>,
    /// the summary, after the last call ended (cleared by [`Self::close`] or a new call)
    pub ended: Option<Ended>,
}

impl Tracker {
    pub fn update(&mut self, s: &Status, now: u64) -> Update {
        let calls: Vec<Call> = s.calls.as_slice().iter().copied().filter(shown).collect();
        let had = !self.last.is_empty();
        let mut appeared = false;
        for c in &calls {
            let before = self.last.iter().find(|l| l.id == c.id);
            appeared |= before.is_none();
            // MODEMD's connect time (2.6) when it has one, else when we first saw it connected
            match self.since.iter_mut().find(|(id, _)| *id == c.id) {
                Some(t) if c.since != 0 => t.1 = c.since,
                Some(_) => {}
                None if connected(c) => self
                    .since
                    .push((c.id, if c.since != 0 { c.since } else { now })),
                None => {}
            }
        }
        for l in &self.last {
            if !calls.iter().any(|c| c.id == l.id) {
                self.gone = Some(Ended {
                    call: *l,
                    seconds: self.seconds(l.id, now),
                    at: now,
                });
            }
        }
        self.since
            .retain(|(id, _)| calls.iter().any(|c| c.id == *id));
        let all_ended = had && calls.is_empty();
        if all_ended {
            self.ended = self.gone.take();
        } else if !calls.is_empty() {
            self.ended = None;
        }
        self.last = calls;
        Update {
            appeared,
            all_ended,
        }
    }

    /// Drop the summary (it closed).
    pub fn close(&mut self) {
        self.ended = None;
    }

    /// Seconds call `id` has been connected.
    pub fn seconds(&self, id: u8, now: u64) -> Option<u32> {
        self.since
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, t)| (now.saturating_sub(*t) / 1000) as u32)
    }

    /// When the next timer turns a second (ms), while a call is connected.
    pub fn next_tick(&self, now: u64) -> Option<u64> {
        self.since
            .iter()
            .map(|(_, t)| now + 1000 - now.saturating_sub(*t) % 1000)
            .min()
    }

    /// The screen for status `s` (`None`: no MODEMD yet).
    pub fn screen(&self, s: Option<&Status>) -> Screen {
        let calls: Vec<Call> = s
            .map(|s| s.calls.as_slice().iter().copied().filter(shown).collect())
            .unwrap_or_default();
        pick(&calls).unwrap_or(match self.ended {
            Some(e) => Screen::Ended(e),
            None => Screen::None,
        })
    }
}

fn pick(calls: &[Call]) -> Option<Screen> {
    let talking = || calls.iter().find(|c| c.state == st::CONVERSATION);
    if let Some(w) = calls.iter().find(|c| ringing(c)) {
        let others = || calls.iter().filter(|c| c.id != w.id);
        let active = talking()
            .or_else(|| others().find(|c| connected(c)))
            .or_else(|| others().next());
        return Some(match active {
            Some(a) => Screen::Waiting {
                active: *a,
                waiting: *w,
            },
            None => Screen::Incoming(*w),
        });
    }
    if let Some(o) = calls.iter().find(|c| outgoing(c)) {
        return Some(Screen::Calling(*o));
    }
    let up: Vec<&Call> = calls.iter().filter(|c| connected(c)).collect();
    if up.len() > 1 && up.iter().all(|c| c.multiparty) {
        let first = up
            .iter()
            .min_by_key(|c| if c.since == 0 { u64::MAX } else { c.since })?;
        return Some(Screen::Conference {
            first: **first,
            count: up.len() as u8,
        });
    }
    match up.as_slice() {
        [] => None,
        [one] => Some(Screen::Active(**one)),
        [first, ..] => {
            let active = talking().unwrap_or(first);
            let held = up.iter().find(|c| c.id != active.id)?;
            Some(Screen::Two {
                active: *active,
                held: **held,
            })
        }
    }
}

/// A running call's timer: "04:12", "1:04:12" past an hour.
pub fn clock(seconds: u32) -> String {
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// A finished call's length (the summary): "4:31", "0:07", "1:04:31".
pub fn length(seconds: u32) -> String {
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// An avatar's initials: the first letters of the first two words ("" for none: the screens show an icon).
pub fn initials(name: &str) -> String {
    name.split_whitespace()
        .take(2)
        .filter_map(|w| w.chars().next())
        .filter(|c| c.is_alphabetic())
        .flat_map(char::to_uppercase)
        .collect()
}

/// The avatar palette of the v2 boards (phone-ui `Tints` has the same eight, in this order).
pub const TINTS: [u32; 8] = [
    0x15803d, 0xbe185d, 0xc2410c, 0x6d28d9, 0x0e7490, 0x2f80ed, 0x8b5cf6, 0x14a3a3,
];

/// The call the shade's ongoing card was posted for (top-panel TP4): MODEMD reuses an id once its call ends, so the
/// card's End acts only on a call that is still the same one (id, number, direction).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardCall {
    pub id: u8,
    pub number: String,
    pub incoming: bool,
}

impl CardCall {
    #[must_use]
    pub fn of(c: &Call) -> Self {
        Self {
            id: c.id,
            number: String::from(c.number.as_str()),
            incoming: c.incoming,
        }
    }

    /// The id to end now: the posted call if it is still in MODEMD's list and live; `None` for a stale action.
    #[must_use]
    pub fn now_in(&self, calls: &[Call]) -> Option<u8> {
        calls
            .iter()
            .find(|c| {
                c.id == self.id
                    && c.incoming == self.incoming
                    && c.number.as_str() == self.number
                    && shown(c)
            })
            .map(|c| c.id)
    }
}

/// An avatar's colour (0xRRGGBB) that stays with the contact.
pub fn tint(contact_id: u32) -> u32 {
    TINTS[contact_id as usize % TINTS.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use modem_ipc::Text;

    fn call(id: u8, state: u8, incoming: bool) -> Call {
        Call {
            id,
            state,
            incoming,
            number: Text::new("+15550142"),
            presentation: 0,
            ..Call::default()
        }
    }

    #[test]
    fn the_card_ends_only_the_call_it_was_posted_for() {
        let posted = CardCall::of(&call(1, st::CONVERSATION, false));
        assert_eq!(posted.now_in(&[call(1, st::CONVERSATION, false)]), Some(1));
        // the call ended and MODEMD reused its id for another caller: stale
        let mut other = call(1, st::INCOMING, true);
        other.number = Text::new("+15550199");
        assert_eq!(posted.now_in(&[other]), None);
        // the same id, now incoming: another call
        assert_eq!(posted.now_in(&[call(1, st::CONVERSATION, true)]), None);
        // gone
        assert_eq!(posted.now_in(&[]), None);
        // a second call alongside doesn't move it
        assert_eq!(
            posted.now_in(&[call(2, st::CONVERSATION, false), call(1, st::HOLD, false)]),
            Some(1)
        );
    }

    fn status(calls: &[Call]) -> Status {
        let mut s = Status::default();
        s.calls.len = calls.len() as u8;
        s.calls.entries[..calls.len()].copy_from_slice(calls);
        s
    }

    fn screen(calls: &[Call]) -> Screen {
        Tracker::default().screen(Some(&status(calls)))
    }

    #[test]
    fn screens() {
        let ring = call(1, st::INCOMING, true);
        assert_eq!(screen(&[]), Screen::None);
        assert_eq!(Tracker::default().screen(None), Screen::None);
        assert_eq!(screen(&[ring]), Screen::Incoming(ring));
        let setup = call(1, st::SETUP, true);
        assert_eq!(screen(&[setup]), Screen::Incoming(setup));
        for state in [st::ORIGINATION, st::CC_IN_PROGRESS, st::ALERTING] {
            let c = call(1, state, false);
            assert_eq!(screen(&[c]), Screen::Calling(c));
        }
        let talk = call(1, st::CONVERSATION, false);
        assert_eq!(screen(&[talk]), Screen::Active(talk));
        let held = call(1, st::HOLD, false);
        assert_eq!(screen(&[held]), Screen::Active(held));
        // ending calls drop out
        let ending = call(2, st::DISCONNECTING, false);
        assert_eq!(screen(&[talk, ending]), Screen::Active(talk));
        assert_eq!(screen(&[call(1, st::END, false)]), Screen::None);
    }

    #[test]
    fn waiting_and_two() {
        let talk = call(1, st::CONVERSATION, false);
        let held = call(1, st::HOLD, false);
        let wait = call(2, st::WAITING, true);
        assert_eq!(
            screen(&[talk, wait]),
            Screen::Waiting {
                active: talk,
                waiting: wait
            }
        );
        assert_eq!(
            screen(&[wait, held]),
            Screen::Waiting {
                active: held,
                waiting: wait
            }
        );
        // a lone waiting call (the other just ended) rings like any incoming call
        assert_eq!(screen(&[wait]), Screen::Incoming(wait));
        let other = call(2, st::CONVERSATION, true);
        assert_eq!(
            screen(&[held, other]),
            Screen::Two {
                active: other,
                held
            }
        );
        let held2 = call(2, st::HOLD, true);
        assert_eq!(
            screen(&[held, held2]),
            Screen::Two {
                active: held,
                held: held2
            }
        );
        // Add call: the first is held while ours rings out
        let out = call(2, st::ALERTING, false);
        assert_eq!(screen(&[held, out]), Screen::Calling(out));
    }

    #[test]
    fn conference() {
        let a = Call {
            multiparty: true,
            since: 9_000,
            ..call(1, st::CONVERSATION, false)
        };
        let b = Call {
            multiparty: true,
            since: 20_000,
            ..call(2, st::CONVERSATION, true)
        };
        assert_eq!(screen(&[b, a]), Screen::Conference { first: a, count: 2 });
        // a conference on hold with a new call talking: two calls, as before
        let held = Call {
            state: st::HOLD,
            ..a
        };
        let c = call(3, st::CONVERSATION, false);
        assert_eq!(screen(&[held, c]), Screen::Two { active: c, held });
    }

    #[test]
    fn modemd_connect_time() {
        // MODEMD's connect time wins over when this app first saw the call (opened mid-call)
        let mut t = Tracker::default();
        let talk = Call {
            since: 2_000,
            ..call(1, st::CONVERSATION, false)
        };
        t.update(&status(&[talk]), 250_000);
        assert_eq!(t.seconds(1, 250_000), Some(248));
    }

    #[test]
    fn timers_and_summary() {
        let mut t = Tracker::default();
        let dial = call(1, st::ORIGINATION, false);
        let u = t.update(&status(&[dial]), 1_000);
        assert!(u.appeared && !u.all_ended);
        assert_eq!(t.next_tick(1_500), None);
        let talk = call(1, st::CONVERSATION, false);
        assert_eq!(t.update(&status(&[talk]), 2_000), Update::default());
        assert_eq!(t.seconds(1, 6_500), Some(4));
        assert_eq!(t.next_tick(6_500), Some(7_000));
        assert_eq!(t.next_tick(7_000), Some(8_000));
        // a second caller appears
        let wait = call(2, st::WAITING, true);
        assert!(t.update(&status(&[talk, wait]), 3_000).appeared);
        // the waiting caller gives up: no summary while a call is up
        let u = t.update(&status(&[talk]), 4_000);
        assert!(!u.appeared && !u.all_ended);
        assert_eq!(t.ended, None);
        // the call ends: its length, the summary
        let u = t.update(&status(&[call(1, st::DISCONNECTING, false)]), 273_000);
        assert!(u.all_ended);
        let e = t.ended.unwrap();
        assert_eq!((e.call.id, e.seconds, e.at), (1, Some(271), 273_000));
        assert!(e.summary());
        assert_eq!(t.screen(Some(&status(&[]))), Screen::Ended(e));
        assert_eq!(t.next_tick(273_000), None);
        // gone for good: nothing new
        assert_eq!(t.update(&status(&[]), 274_000), Update::default());
        t.close();
        assert_eq!(t.screen(Some(&status(&[]))), Screen::None);
    }

    #[test]
    fn missed_and_reused_ids() {
        let mut t = Tracker::default();
        t.update(&status(&[call(1, st::INCOMING, true)]), 0);
        assert!(t.update(&status(&[]), 5_000).all_ended);
        let e = t.ended.unwrap();
        assert_eq!(e.seconds, None);
        assert!(!e.summary(), "a missed call just closes");
        // MODEMD reuses id 1 for the next call: new, and the summary goes
        let u = t.update(&status(&[call(1, st::ORIGINATION, false)]), 6_000);
        assert!(u.appeared);
        assert_eq!(t.ended, None);
        assert!(t.update(&status(&[]), 9_000).all_ended);
        assert!(
            t.ended.unwrap().summary(),
            "Call again after an unanswered call"
        );
    }

    #[test]
    fn text() {
        assert_eq!(clock(252), "04:12");
        assert_eq!(clock(3_872), "1:04:32");
        assert_eq!(length(271), "4:31");
        assert_eq!(length(7), "0:07");
        assert_eq!(length(3_871), "1:04:31");
        assert_eq!(initials("Ava Lindqvist"), "AL");
        assert_eq!(initials("Անի Պետրոսյան"), "ԱՊ");
        assert_eq!(initials("ivan"), "I");
        assert_eq!(initials("+1 555 0142"), "");
        assert_eq!(tint(0), 0x15803d);
        assert_eq!(tint(9), 0xbe185d);
    }
}
