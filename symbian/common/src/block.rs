// SPDX-License-Identifier: GPL-3.0-only
//! MODEMD's blocklist, edited through [`phone_core::blocking`] from any of the three apps (a Recents row, a contact
//! card, a message's sender, Settings ▸ Blocked numbers). Each edit returns the note to show.

use crate::modem::Modem;
use alloc::format;
use alloc::string::String;
use phone_core::blocking::{self, Refusal};
use phone_rules::block::List;

/// Edit a copy of `list` and send it to MODEMD; `list` keeps what MODEMD has (re-read after a failure). Ok with
/// `done` on success, Err with the note when refused or not sent.
pub fn edit(
    modem: &mut Modem,
    list: &mut List,
    f: impl FnOnce(&mut List) -> Result<(), Refusal>,
    done: &str,
) -> Result<String, String> {
    // the list as MODEMD has it now: another of the three apps may have changed it since this one read it
    if let Ok(now) = modem.block_list() {
        *list = now;
    }
    let mut copy = list.clone();
    f(&mut copy).map_err(|r| String::from(r.text()))?;
    let r = modem.set_block_list(&copy);
    if r != 0 {
        if let Ok(l) = modem.block_list() {
            *list = l;
        }
        return Err(format!("Not saved ({r})"));
    }
    *list = copy;
    Ok(String::from(done))
}

/// Block `number` (calls and/or messages).
pub fn block_number(
    modem: &mut Modem,
    list: &mut List,
    number: &str,
    calls: bool,
    messages: bool,
) -> Result<String, String> {
    edit(
        modem,
        list,
        |l| blocking::block(l, number, calls, messages),
        &format!("Blocked {number}"),
    )
}

/// Remove every rule matching `number` exactly (Unblock on a card, a Recents row, a message). Ok("") when nothing
/// blocked it.
pub fn unblock_number(modem: &mut Modem, list: &mut List, number: &str) -> Result<String, String> {
    if blocked(list, number).is_none() {
        return Ok(String::new());
    }
    edit(
        modem,
        list,
        |l| {
            blocking::remove_number(l, number);
            Ok(())
        },
        "Unblocked",
    )
}

/// What blocks `number` (calls, messages), if a number rule does.
pub fn blocked(list: &List, number: &str) -> Option<(bool, bool)> {
    blocking::lookup(list, number)
}
