// SPDX-License-Identifier: GPL-3.0-only
//! NEOPHONE.EXE: Phone and Contacts as one neoui app process (NeoSymbian/phone). The UI server starts it hidden at
//! boot and keeps it running (neoui `host::RESIDENT`), opens its pages from the Phone and Contacts icons, the call
//! notification and Settings (`AppEvent::Page`), and shows its surface between the status bar and the toolbar,
//! with the title and middle button this app sets (neoui ipc 1.4).
//!
//! - [`ctx`]: the state and the shared moves; [`modem`]: MODEMD's client; [`store`]: the contacts file and vCards
//! - areas: [`dial`] (Phone), [`people`] (Contacts), [`call`] (the call screens), [`prefs`] (Settings, blocking)
//! - this file: connecting, loading, and the event loop (input, pages, MODEMD's status, time)
#![no_std]

extern crate alloc;

mod call;
mod ctx;
mod dial;
mod modem;
mod people;
mod prefs;
mod store;

use alloc::rc::Rc;
use core::pin::pin;
use ctx::{Ctx, Model};
use e32::ipc::TIpcArgs;
use e32::rt::{self, Either};
use e32::sys::TPtr8;
use e32::{dprintln, time};
use neoapp::App;
use neoui_proto::InputEvent;
use neoui_proto::ipc::{self as proto, AppEvent};
use neoui_proto::settings::keys;
use phone_core::page::{Page, PhoneView};
use phone_ui::{Nav, PhoneApp, Theme};
use slint::ComponentHandle;

e32::main!(main);

fn main() -> i32 {
    match run() {
        Ok(()) => 0,
        Err(e) => {
            dprintln!("neophone: failed {}", e);
            e
        }
    }
}

fn run() -> Result<(), i32> {
    let app = App::connect()?;
    dprintln!("neophone: surface {}x{} mapped", app.width, app.height);
    let ui = PhoneApp::new().map_err(|_| -2)?;
    ui.global::<Theme>().set_dp(app.dp);
    ui.show().map_err(|_| -2)?;
    for prefix in ["phone.", "contacts."] {
        if let Err(e) = app.watch_settings(prefix) {
            dprintln!("neophone: cannot watch {} ({})", prefix, e);
        }
    }
    let migrate = !app.setting(keys::CONTACTS_MIGRATED).unwrap_or(false);
    let (book, migrated) = store::load(migrate);
    if migrate {
        let _ = app.set_setting(keys::CONTACTS_MIGRATED, true);
        if migrated {
            dprintln!("neophone: contacts copied from the shell's book");
        }
    }
    let mut modem = modem::Modem::default();
    let block = modem.block_list().unwrap_or_default();
    let model = Model {
        page: Page::Phone(PhoneView::Dial),
        before_call: Page::Phone(PhoneView::Dial),
        modem,
        status: None,
        book,
        order: contacts::NameOrder::FirstLast,
        log: alloc::vec::Vec::new(),
        log_revision: None,
        block,
        dial: Default::default(),
        people: Default::default(),
        call: Default::default(),
        prefs: Default::default(),
        note_until: 0,
        visible: false,
    };
    let ctx = Rc::new(Ctx::new(app, &ui, model));
    prefs::reload(&ctx);
    people::send_known(&ctx);
    let nav = ui.global::<Nav>();
    {
        let ctx = Rc::downgrade(&ctx);
        nav.on_act(move |action, value| {
            let Some(ctx) = ctx.upgrade() else { return };
            match action.as_str() {
                "switch" => ctx.go(if value == "1" {
                    Page::Contacts(phone_core::page::ContactsView::List)
                } else {
                    Page::Phone(PhoneView::Dial)
                }),
                "option" => ctx.option(&value),
                "dismiss-options" => ctx.ui().global::<Nav>().set_options_open(false),
                _ => {}
            }
        });
    }
    dial::bind(&ctx, &ui);
    people::bind(&ctx, &ui);
    call::bind(&ctx, &ui);
    prefs::bind(&ctx, &ui);
    ctx.go(Page::Phone(PhoneView::Dial));
    ctx.sync()?;

    rt::block_on(async {
        let mut watch = pin!(modem::Watch::default().next());
        loop {
            let mut ev = [0u8; proto::EVENT_LEN];
            let got = {
                let mut des = TPtr8::new(&mut ev);
                let mut args = TIpcArgs::new();
                args.des8(0, &mut des);
                // one NEXT_EVENT stays outstanding while MODEMD's status and the clock come in
                let mut next = pin!(ctx.app.server.send(proto::NEXT_EVENT, &args));
                loop {
                    let wait = ctx.next_wake_us();
                    match rt::select(next.as_mut(), rt::select(watch.as_mut(), time::delay(wait)))
                        .await
                    {
                        Either::Left(r) => break r,
                        Either::Right(Either::Left((w, status))) => {
                            watch.set(w.next());
                            ctx.on_status(status);
                            ctx.sync()?;
                        }
                        Either::Right(Either::Right(())) => {
                            ctx.tick();
                            ctx.sync()?;
                        }
                    }
                }
            };
            got?;
            match AppEvent::decode(&ev) {
                Some(AppEvent::Quit) => {
                    dprintln!("neophone: quit");
                    return Ok(());
                }
                Some(AppEvent::Visible(v)) => {
                    dprintln!("neophone: visible {}", v);
                    ctx.m.borrow_mut().visible = v;
                    if v {
                        ctx.show();
                    }
                }
                Some(AppEvent::Page { page }) => {
                    dprintln!("neophone: page {}", page);
                    if let Some(p) = Page::from_number(page) {
                        ctx.go(p);
                    }
                }
                Some(AppEvent::Settings { .. }) => {
                    prefs::reload(&ctx);
                    ctx.show();
                }
                Some(AppEvent::Back) => {
                    ctx.back();
                }
                Some(AppEvent::Options) => ctx.options(),
                Some(AppEvent::Action) => ctx.action(),
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
