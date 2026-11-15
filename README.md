# NeoPhone

Phone, Contacts and Messages for [NeoSymbian](https://github.com/NeoSymbian): one app process (`NEOPHONE.EXE`)
behind three home icons, drawn with Slint into the surface neoui's UI server gives it. N8 Belle's structure, redrawn
for the Pixel 3a XL's screen; the screens follow the approved mockups (36 screens: the dialer with Recents above the
keypad, Recents, call details, speed dial, the call screens, the contact list with per-script sections, the card, the
editor, Phone & Contacts settings, the blocklist; the conversations, a thread, a new message, Reply with message,
the message settings, the blocked messages).

| Path | What | Licence |
|---|---|---|
| `crates/contacts` | The contacts model: the book (NCT1 file), caller-name lookup, smart dialling (T9: Latin, Armenian, Cyrillic), the per-script A-Z index, vCard 2.1 import/export | Apache-2.0 |
| `crates/phone-core` | Behaviour without UI or OS: pages, the dialer, Recents, the call screens from MODEMD's call list, speed dial, blocklist editing, the conversations from MODEMD's messages | Apache-2.0 |
| `crates/phone-ui` | The screens in Slint (no_std), with neoui's theme, fonts and components imported as `@neoui/...` from the pinned neoui; host snapshot tests write `snapshots/*.png` | GPL-3.0 (links Slint) |
| `symbian/app` | `NEOPHONE.EXE`: the event loop, MODEMD's client (modem-ipc 2.3), the contacts and quick-replies files, each area's glue | GPL-3.0 (links Slint) |

It talks to MODEMD ([telephony](https://github.com/NeoSymbian/telephony)) directly: calls, the call log, caller ID,
call waiting, the blocklist MODEMD applies before a call rings or a message alerts, and the messages MODEMD receives,
keeps and sends (SMS: Messages mirrors them by revision). neoui's UI server keeps it resident and opens its
pages (`AppEvent::Page`); it sets the status bar's title and the toolbar's middle button (neoui app protocol 1.4).

## Build

```sh
cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test   # host; snapshots/*.png
cd symbian && cargo fmt --all --check && cargo arm   # target/armv5te-none-eabi/release/libneophone.a
```

For a ROM, copy `symbian/exes.txt`'s line and `libneophone.a` into NeoSymbian/build's `rust/`. The pins (e32, Slint,
neoui, telephony) must match neoui's `symbian/Cargo.toml`: one e32 and one Slint per ROM.
