# NeoPhone

Phone, Contacts and Messages for [NeoSymbian](https://github.com/NeoSymbian): three apps (`NEOPHONE.EXE`,
`NEOCONT.EXE`, `NEOMSG.EXE`), each its own process and card in Open apps, that open each other's pages (neoui app
protocol 1.9, `OPEN`: a contact's Call opens Phone on the call), drawn with Slint and the neoui kit into the surfaces
neoui's UI server gives them. N8 Belle's structure, redrawn
for the Pixel 3a XL's screen; the screens follow the approved mockups (36 screens: the dialer with Recents above the
keypad, Recents, call details, speed dial, the call screens, the contact list with per-script sections, the card, the
editor, Phone & Contacts settings, the blocklist; the conversations, a thread, a new message, Reply with message,
the message settings, the blocked messages).

| Path | What | Licence |
|---|---|---|
| `crates/contacts` | The contacts model: the book (NCT1 file), caller-name lookup, smart dialling (T9: Latin, Armenian, Cyrillic), the per-script A-Z index, vCard 2.1 import/export | Apache-2.0 |
| `crates/phone-core` | Behaviour without UI or OS: pages, the dialer, Recents, the call screens from MODEMD's call list, speed dial, blocklist editing, the conversations from MODEMD's messages | Apache-2.0 |
| `ui-shared` | The Slint parts the three share (`@phone-shared`): the frame (kit header, the footer that switches between the apps), `Nav`, rows, sheets, the text field, icons | GPL-3.0 (Slint) |
| `crates/phone-ui`, `crates/contacts-ui`, `crates/messages-ui` | Each app's screens in Slint (no_std), with the neoui kit imported as `@neoui/...` (built by `neoui_kit::compile_with`); host snapshot tests write `snapshots/*.png` | GPL-3.0 (links Slint) |
| `symbian/common` | What the three share on the device: MODEMD's client (modem-ipc 2.3), the contacts and quick-replies files, the settings they read, blocking, opening each other | GPL-3.0 |
| `symbian/phone`, `symbian/contacts`, `symbian/messages` | `NEOPHONE.EXE` (Recents, the dialer, the call screens, Settings), `NEOCONT.EXE` (Contacts: the only writer of the book), `NEOMSG.EXE` (Messages): each one's event loop and its areas' glue | GPL-3.0 (links Slint) |

It talks to MODEMD ([telephony](https://github.com/NeoSymbian/telephony)) directly: calls, the call log, caller ID,
call waiting, the blocklist MODEMD applies before a call rings or a message alerts, and the messages MODEMD receives,
keeps and sends (SMS: Messages mirrors them by revision). neoui's UI server keeps Phone and Messages resident
(an incoming call shows at once; Messages posts the shade's entries), opens Phone's pages from the shade and Settings
(`AppEvent::Page`), and passes one app's OPEN to another (`AppEvent::Open` with an argument: a number). Contacts
writes the book and bumps the `contacts.rev` setting; Phone and Messages read it again.

## Build

```sh
cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test   # host; snapshots/*.png
cd symbian && cargo fmt --all --check && cargo arm   # target/armv5te-none-eabi/release/lib{neophone,neocont,neomsg}.a
```

For a ROM, copy `symbian/exes.txt`'s three lines and the three staticlibs into NeoSymbian/build's `rust/`. The pins (e32, Slint,
neoui, telephony) must match neoui's `symbian/Cargo.toml`: one e32 and one Slint per ROM.
