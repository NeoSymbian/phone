// SPDX-License-Identifier: GPL-3.0-only
//! MODEMD's client (NeoModem, modem-ipc 2.2): [`Watch`] keeps a NOTIFY outstanding for status changes (owned, so it
//! can sit in the event loop's select), [`Modem`] sends commands on its own session. Moved from neoui's shell
//! (symbian/src/phone.rs) when Phone became this app.

use alloc::vec;
use alloc::vec::Vec;
use e32::ipc::{Session, TIpcArgs, TVersion};
use e32::sys::{TPtr8, TPtrC8};
use modem_ipc::{LogEntry, MAX_LOG, Status, manage, op, waiting};

/// KErrNotReady: no MODEMD (QEMU/CI) or no session.
pub const NOT_READY: i32 = -18;

fn connect() -> Result<Session, i32> {
    let (major, minor, build) = modem_ipc::VERSION;
    Session::connect(
        modem_ipc::NAME,
        TVersion {
            major: major as u8,
            minor: minor as u8,
            build: build as u16,
        },
    )
}

/// Status updates: one NOTIFY outstanding, the future owns its session.
pub struct Watch {
    session: Option<Session>,
    revision: u32,
    /// the wait before the next connect after a failure: 0.5 s doubling to 10 s, back to 0.5 s after a status
    backoff_ms: u32,
}

const BACKOFF_MS: u32 = 500;
const BACKOFF_MAX_MS: u32 = 10_000;

/// Cancels the held request before its future and descriptor storage are dropped, unless it completed.
struct Cancel<'a>(&'a Session, bool);
impl Drop for Cancel<'_> {
    fn drop(&mut self) {
        if !self.1 {
            let _ = self.0.send_receive(op::CANCEL_NOTIFY, &TIpcArgs::new());
        }
    }
}

impl Default for Watch {
    fn default() -> Self {
        Self {
            session: None,
            revision: u32::MAX,
            backoff_ms: BACKOFF_MS,
        }
    }
}

impl Watch {
    async fn back_off(&mut self) {
        e32::time::delay(i32::try_from(self.backoff_ms * 1000).unwrap_or(i32::MAX)).await;
        self.backoff_ms = self.backoff_ms.saturating_mul(2).min(BACKOFF_MAX_MS);
    }

    /// The next status (`None` after a failure, after backing off).
    pub async fn next(mut self) -> (Self, Option<Status>) {
        if self.session.is_none() {
            self.session = connect().ok();
            self.revision = u32::MAX;
        }
        let Some(session) = self.session.as_ref() else {
            self.back_off().await;
            return (self, None);
        };
        let mut bytes = [0u8; Status::SIZE];
        let result = {
            let mut descriptor = TPtr8::new(&mut bytes);
            let mut args = TIpcArgs::new();
            args.des8(0, &mut descriptor);
            args.int(1, self.revision as i32);
            let future = core::pin::pin!(session.send(op::NOTIFY, &args));
            // declared after the pinned future: cancellation runs first on every exit path where it is still held
            let mut cancel = Cancel(session, false);
            let r = future.await;
            cancel.1 = true;
            r
        };
        let status = result.ok().and_then(|_| Status::from_bytes(&bytes));
        if let Some(s) = status {
            self.revision = s.revision;
            self.backoff_ms = BACKOFF_MS;
        } else {
            self.session = None;
            self.back_off().await;
        }
        (self, status)
    }
}

/// Commands. Each returns KErrNone (0) or a Symbian error (KErrNotReady without MODEMD or a call to act on).
#[derive(Default)]
pub struct Modem {
    session: Option<Session>,
}

impl Modem {
    fn send(&mut self, function: i32, args: &TIpcArgs) -> i32 {
        if self.session.is_none() {
            self.session = connect().ok();
        }
        match self.session.as_ref() {
            Some(s) => s.send_receive(function, args).unwrap_or_else(|e| e),
            None => NOT_READY,
        }
    }

    fn ints(&mut self, function: i32, a: i32, b: i32) -> i32 {
        let mut args = TIpcArgs::new();
        args.int(0, a);
        args.int(1, b);
        self.send(function, &args)
    }

    fn bytes(&mut self, function: i32, b: &[u8]) -> i32 {
        let d = TPtrC8::new(b);
        let mut args = TIpcArgs::new();
        args.desc8(0, &d);
        self.send(function, &args)
    }

    /// Read into a buffer of `room` bytes; the bytes written.
    fn read(&mut self, function: i32, room: usize) -> Result<Vec<u8>, i32> {
        let mut b = vec![0u8; room];
        let n = {
            let mut d = TPtr8::new(&mut b);
            let mut args = TIpcArgs::new();
            args.des8(0, &mut d);
            match self.send(function, &args) {
                r if r < 0 => return Err(r),
                _ => d.len(),
            }
        };
        b.truncate(n);
        Ok(b)
    }

    pub fn dial(&mut self, number: &str) -> i32 {
        self.bytes(op::DIAL, number.as_bytes())
    }
    /// Answer call `id` (0 = the selected call); a waiting call is answered and the active one held.
    pub fn answer(&mut self, id: u8) -> i32 {
        self.ints(op::ANSWER, i32::from(id), 0)
    }
    /// End call `id` (0 = the selected call); a ringing or waiting call is rejected as busy.
    pub fn hangup(&mut self, id: u8) -> i32 {
        self.ints(op::HANGUP, i32::from(id), 0)
    }
    pub fn hold(&mut self, id: u8) -> i32 {
        self.ints(op::MANAGE, manage::HOLD, i32::from(id))
    }
    pub fn resume(&mut self, id: u8) -> i32 {
        self.ints(op::MANAGE, manage::RESUME, i32::from(id))
    }
    pub fn swap(&mut self) -> i32 {
        self.ints(op::MANAGE, manage::SWAP, 0)
    }
    pub fn merge(&mut self) -> i32 {
        self.ints(op::MANAGE, manage::MERGE, 0)
    }
    /// End the active call(s) and answer the waiting one.
    pub fn end_and_answer(&mut self) -> i32 {
        self.ints(op::MANAGE, manage::END_ANSWER, 0)
    }
    pub fn mute(&mut self, on: bool) -> i32 {
        self.ints(op::MUTE, i32::from(on), 0)
    }
    pub fn speaker(&mut self, on: bool) -> i32 {
        self.ints(op::SPEAKER, i32::from(on), 0)
    }
    /// A DTMF tone for `key` (`0-9*#`; MODEMD stops it after 200 ms).
    pub fn dtmf(&mut self, key: u8) -> i32 {
        self.ints(op::DTMF, i32::from(key), 0)
    }
    /// Ring settings (volume 0..=10, silent, vibrate): Silence on an incoming call sends silent + no vibration.
    pub fn ring(&mut self, volume: u8, silent: bool, vibrate: bool) -> i32 {
        let mut args = TIpcArgs::new();
        args.int(0, i32::from(volume));
        args.int(1, i32::from(silent));
        args.int(2, i32::from(vibrate));
        self.send(op::RING_SETTINGS, &args)
    }
    /// Caller ID (`modem_ipc::clir`) and the days the log keeps (0, 1, 10, 30).
    pub fn call_settings(&mut self, clir: i32, days: i32) -> i32 {
        self.ints(op::CALL_SETTINGS, clir, days)
    }
    /// Call waiting: `None` asks the network, `Some(on)` sets it; the answer comes as `Status::waiting`.
    pub fn waiting(&mut self, set: Option<bool>) -> i32 {
        let a = match set {
            None => waiting::QUERY,
            Some(true) => waiting::ACTIVATE,
            Some(false) => waiting::DEACTIVATE,
        };
        self.ints(op::CALL_WAITING, a, 0)
    }

    /// The call log, newest first.
    pub fn log(&mut self) -> Result<Vec<LogEntry>, i32> {
        let b = self.read(op::CALL_LOG, MAX_LOG * LogEntry::SIZE)?;
        Ok(b.as_chunks::<{ LogEntry::SIZE }>()
            .0
            .iter()
            .filter_map(|e| LogEntry::from_bytes(e))
            .collect())
    }
    /// Delete entry `id` (0 = all) of `kind` (0 = any).
    pub fn clear_log(&mut self, id: u32, kind: u8) -> i32 {
        self.ints(op::CLEAR_LOG, id as i32, i32::from(kind))
    }

    pub fn block_list(&mut self) -> Result<phone_rules::block::List, i32> {
        let b = self.read(op::BLOCK_GET, modem_ipc::MAX_BLOCK_BYTES)?;
        phone_rules::block::List::decode(&b).ok_or(-6)
    }
    pub fn set_block_list(&mut self, list: &phone_rules::block::List) -> i32 {
        self.bytes(op::BLOCK_SET, &list.encode(false))
    }
    /// The contacts' numbers as [`phone_rules::block::known_key`] hashes (for "block unknown numbers").
    pub fn set_known(&mut self, keys: &[u32]) -> i32 {
        let b: Vec<u8> = keys
            .iter()
            .take(modem_ipc::MAX_KNOWN_BYTES / 4)
            .flat_map(|k| k.to_le_bytes())
            .collect();
        self.bytes(op::KNOWN_SET, &b)
    }
}
