// SPDX-License-Identifier: GPL-3.0-only
//! NEOCONT.EXE: Contacts (NeoSymbian/phone), one of three apps with Phone (NEOPHONE) and Messages (NEOMSG). The only app that writes the contacts book (the others
//! read it again when `contacts.rev` changes). Phone and Messages open it on a number's card or a new contact
//! (`AppEvent::Open`); its card's Call and Message open them.
//!
//! - [`ctx`]: the state and the shared moves; `phone_common`: MODEMD's client, the contacts file, settings, blocking
//! - [`people`]: the screens' behaviour
//! - this file: connecting, loading, and the event loop (input, pages, MODEMD's status)
#![no_std]

extern crate alloc;

mod ctx;
mod people;

use alloc::rc::Rc;
use contacts_ui::{ContactsApp, Kit, Nav, Theme};
use core::pin::pin;
use ctx::{Ctx, Model};
use e32::ipc::TIpcArgs;
use e32::rt::{self, Either};
use e32::sys::TPtr8;
use e32::{dprintln, time};
use neoapp::App;
use neoui_proto::InputEvent;
use neoui_proto::ipc::{self as proto, AppEvent};
use phone_common::{modem, store};
use phone_core::page::{ContactsView, MessagesView, Page, PhoneView, app as key};
use slint::ComponentHandle;

e32::main!(main);

fn main() -> i32 {
    match run() {
        Ok(()) => 0,
        Err(e) => {
            dprintln!("neocont: failed {}", e);
            e
        }
    }
}

fn run() -> Result<(), i32> {
    let app = App::connect()?;
    dprintln!("neocont: connected, {}x{}", app.width, app.height());
    let ui = ContactsApp::new().map_err(|_| -2)?;
    ui.global::<Theme>().set_dp(app.dp);
    ui.show().map_err(|_| -2)?;
    for prefix in ["phone.", "contacts."] {
        if let Err(e) = app.watch_settings(prefix) {
            dprintln!("neocont: cannot watch {} ({})", prefix, e);
        }
    }
    // the first start moves the shell's old book over (before Phone was an app)
    let migrate = !app
        .setting(neoui_proto::settings::keys::CONTACTS_MIGRATED)
        .unwrap_or(false);
    let (book, migrated) = store::load(migrate);
    if migrate {
        let _ = app.set_setting(neoui_proto::settings::keys::CONTACTS_MIGRATED, true);
        if migrated {
            dprintln!("neocont: contacts copied from the shell's book");
        }
    }
    let mut modem = modem::Modem::default();
    let block = modem.block_list().unwrap_or_default();
    let model = Model {
        page: Page::Contacts(ContactsView::List),
        modem,
        status: None,
        book,
        order: contacts::NameOrder::FirstLast,
        log: alloc::vec::Vec::new(),
        log_revision: None,
        block,
        prefs: Default::default(),
        people: Default::default(),
        visible: false,
        call_bar: Default::default(),
    };
    let ctx = Rc::new(Ctx::new(app, &ui, model));
    ctx.reload_prefs();
    people::send_known(&ctx);
    {
        // the frame's taps: ← or a form's ✕ (Back), the action (an icon or ✓), ⋮'s items, the footer's other apps
        let weak = Rc::downgrade(&ctx);
        let ctx = weak.clone();
        ui.global::<Nav>().on_searched(move |q| {
            if let Some(ctx) = ctx.upgrade() {
                ctx.searched(&q);
            }
        });
        let ctx = weak;
        ui.global::<Nav>().on_header(move |kind, i| {
            let Some(ctx) = ctx.upgrade() else { return };
            match kind.as_str() {
                "back" => {
                    ctx.back();
                }
                // a form's ✓ or the content's pill
                "save" => ctx.action(),
                // Return to call
                "call-bar" => {
                    let _ = phone_common::open_number(
                        &ctx.app,
                        key::PHONE,
                        phone_core::page::number::phone::CALL,
                        "",
                    );
                }
                "action" => ctx.header_action(usize::try_from(i).unwrap_or(usize::MAX)),
                "selection" => ctx.selection_action(usize::try_from(i).unwrap_or(usize::MAX)),
                // ✕ in selection mode: Back clears it
                "selection-closed" => {
                    ctx.back();
                }
                "tab" => match i {
                    0 => ctx.go(Page::Phone(PhoneView::Recents)),
                    1 => ctx.go(Page::Contacts(ContactsView::List)),
                    2 => ctx.go(Page::Messages(MessagesView::List)),
                    _ => {}
                },
                _ => ctx.option_at(usize::try_from(i).unwrap_or(usize::MAX)),
            }
        });
    }
    people::bind(&ctx, &ui);
    ctx.go(Page::Contacts(ContactsView::List));
    ctx.sync()?;

    rt::block_on(async {
        let mut watch = pin!(modem::Watch::default().next());
        loop {
            let mut ev = [0u8; proto::EVENT_LEN];
            let got = {
                let mut des = TPtr8::new(&mut ev);
                let mut args = TIpcArgs::new();
                args.des8(0, &mut des);
                // one NEXT_EVENT stays outstanding while MODEMD's status comes in
                let mut next = pin!(ctx.app.server.send(proto::NEXT_EVENT, &args));
                loop {
                    match rt::select(
                        next.as_mut(),
                        rt::select(
                            watch.as_mut(),
                            time::delay(phone_common::wait_us(&ctx.app, own_wait(&ctx))),
                        ),
                    )
                    .await
                    {
                        Either::Left(r) => break r,
                        Either::Right(Either::Left((w, status))) => {
                            watch.set(w.next());
                            ctx.on_status(status);
                            ctx.sync()?;
                        }
                        // Slint's timers and animations (a long-press, the letter bubble)
                        Either::Right(Either::Right(())) => ctx.sync()?,
                    }
                }
            };
            got?;
            match AppEvent::decode(&ev) {
                Some(AppEvent::Quit) => {
                    dprintln!("neocont: quit");
                    return Ok(());
                }
                Some(e @ AppEvent::Visible(v)) => {
                    ctx.app.handle(&e);
                    ctx.m.borrow_mut().visible = v;
                    if v {
                        ctx.show();
                    }
                }
                Some(AppEvent::Open { page }) => {
                    let arg = ctx.app.arg().unwrap_or_default();
                    dprintln!("neocont: opened on page {}", page);
                    if let Some(p) = Page::from_number(key::CONTACTS, page) {
                        opened(&ctx, page, p, &arg);
                    }
                }

                Some(AppEvent::Settings { .. }) => {
                    ctx.reload_prefs();
                    // the book is ours: nothing to read again
                    ctx.show();
                }
                // the kit's layers first (the header's ⋮ menu)
                Some(AppEvent::Back) if neoapp::kit_back!(ctx.ui()) => {}
                Some(AppEvent::Back) => {
                    ctx.back();
                }
                Some(AppEvent::Keyboard { h }) => {
                    // 0 while a field still has the focus: the user put the keyboard away (its hide key, Back);
                    // a tap on the field brings it back (it asks again through ctx.sync)
                    let ui = ctx.ui();
                    let kf = ui.global::<contacts_ui::KeyboardFocus>();
                    kf.set_dismissed(h == 0 && kf.get_open());
                    kf.set_text_h(h as f32);
                }
                Some(AppEvent::Answer { i }) => ctx.answer(i == 0),
                // the navigation mode changed the app's height (Buttons 639 dp, Gestures 679 dp)
                Some(e @ AppEvent::Area { .. }) => {
                    ctx.app.handle(&e);
                    ctx.show();
                }
                Some(AppEvent::Input(input)) => match input {
                    InputEvent::Key { code, down: true } => ctx.app.dispatch_key(code),
                    InputEvent::Key { .. } => {}
                    touch => ctx.app.dispatch_touch(touch),
                },
                _ => {}
            }
            ctx.sync()?;
        }
    })
}

/// Phone or Messages opened Contacts on a page with a number: its card (or a new contact), a new contact; else the
/// page.
fn opened(ctx: &Ctx, n: u16, p: Page, arg: &str) {
    use phone_core::page::number::contacts;
    match n {
        contacts::CARD | contacts::NEW if !arg.is_empty() => {
            people::opened_on(ctx, arg, n == contacts::NEW);
        }
        _ => ctx.go(p),
    }
}

/// The loop's own wake (µs): every second while the Return to call bar's clock runs.
fn own_wait(ctx: &Ctx) -> i32 {
    if ctx.m.borrow().call_bar.live() {
        1_000_000
    } else {
        60_000_000
    }
}
