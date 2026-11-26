// SPDX-License-Identifier: GPL-3.0-only
//! NEOPHONE.EXE: Phone (NeoSymbian/phone), one of three apps with Contacts (NEOCONT) and Messages (NEOMSG). The UI
//! server starts it hidden at boot and keeps it running (neoui `host::RESIDENT`: an incoming call shows at once),
//! opens its pages from the Phone icon, the shade and Settings (`AppEvent::Page`), and the other two apps open it on a
//! page with a number (`AppEvent::Open`: Call, a speed-dial key, Settings).
//!
//! - [`ctx`]: the state and the shared moves; `phone_common`: MODEMD's client, the contacts file, settings, blocking
//! - areas: [`dial`] (Recents, the keypad sheet, details, speed dial), [`call`] (the call screens), [`prefs`]
//!   (Settings, blocking, quick replies)
//! - this file: connecting, loading, and the event loop (input, pages, MODEMD's status, time)
#![no_std]

extern crate alloc;

mod call;
mod ctx;
mod dial;
mod prefs;

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
use neoui_proto::settings::Value;
use phone_common::{CONTACTS_REV, modem, store};
use phone_core::page::{Page, PhoneView, app as key};
use phone_ui::{Kit, Nav, PhoneApp, Theme};
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
    dprintln!("neophone: surface {}x{} mapped", app.width, app.height());
    let ui = PhoneApp::new().map_err(|_| -2)?;
    ui.global::<Theme>().set_dp(app.dp);
    ui.show().map_err(|_| -2)?;
    for prefix in ["phone.", "contacts."] {
        if let Err(e) = app.watch_settings(prefix) {
            dprintln!("neophone: cannot watch {} ({})", prefix, e);
        }
    }
    // Contacts (NEOCONT) writes the book and moves the shell's old one over; this app only reads it
    let (book, _) = store::load(false);
    let mut book_rev = contacts_rev(&app);
    let mut modem = modem::Modem::default();
    let block = modem.block_list().unwrap_or_default();
    let model = Model {
        page: Page::Phone(PhoneView::Recents),
        before_call: Page::Phone(PhoneView::Recents),
        modem,
        status: None,
        book,
        order: contacts::NameOrder::FirstLast,
        log: alloc::vec::Vec::new(),
        log_revision: None,
        block,
        dial: Default::default(),
        call: Default::default(),
        prefs: Default::default(),
        msgs: Default::default(),
        visible: false,
        call_bar: Default::default(),
        call_opened: false,
        speed_opened: false,
        settings_opened: false,
        waiting_until: None,
    };
    let ctx = Rc::new(Ctx::new(app, &ui, model));
    prefs::reload(&ctx);
    ctx::load_sms(&ctx);
    let nav = ui.global::<Nav>();
    {
        // the frame's taps: ← or a form's ✕ (Back), the action (an icon, ✓ or the content's pill), ⋮'s items, the
        // footer's Contacts and Messages
        let weak = Rc::downgrade(&ctx);
        let ctx = weak.clone();
        nav.on_searched(move |q| {
            if let Some(ctx) = ctx.upgrade() {
                ctx.searched(&q);
            }
        });
        let ctx = weak;
        nav.on_header(move |kind, i| {
            let Some(ctx) = ctx.upgrade() else { return };
            match kind.as_str() {
                "back" => {
                    ctx.back();
                }
                // a form's ✓ or the content's pill
                "save" => ctx.action(),
                // Return to call
                "call-bar" => ctx.go(Page::Call),
                "action" => ctx.header_action(usize::try_from(i).unwrap_or(usize::MAX)),
                "selection" => ctx.selection_action(usize::try_from(i).unwrap_or(usize::MAX)),
                "selection-option" => {
                    ctx.selection_option(usize::try_from(i).unwrap_or(usize::MAX))
                }
                // ✕ in selection mode: Back clears it
                "selection-closed" => {
                    ctx.back();
                }
                "tab" => match i {
                    1 => ctx.go(Page::Contacts(phone_core::page::ContactsView::List)),
                    2 => ctx.go(Page::Messages(phone_core::page::MessagesView::List)),
                    _ => {}
                },
                _ => ctx.option_at(usize::try_from(i).unwrap_or(usize::MAX)),
            }
        });
    }
    dial::bind(&ctx, &ui);
    call::bind(&ctx, &ui);
    prefs::bind(&ctx, &ui);
    ctx.go(Page::Phone(PhoneView::Recents));
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
                Some(e @ AppEvent::Visible(v)) => {
                    dprintln!("neophone: visible {}", v);
                    ctx.app.handle(&e);
                    ctx.m.borrow_mut().visible = v;
                    if v {
                        ctx.show();
                    }
                }
                Some(AppEvent::Page { page }) => {
                    dprintln!("neophone: page {}", page);
                    ctx.m.borrow_mut().call_opened = false;
                    ctx.m.borrow_mut().settings_opened = false;
                    match Page::from_number(key::PHONE, page) {
                        // the system Settings app's row: Back returns there (DONE)
                        Some(Page::Settings(_)) => {
                            ctx.m.borrow_mut().settings_opened = true;
                            prefs::open_at(&ctx, "");
                        }
                        Some(p) => ctx.go(p),
                        None => {}
                    }
                }
                // Contacts or Messages opened a page: Call a number, put one on a speed-dial key, Settings
                Some(AppEvent::Open { page }) => {
                    let arg = ctx.app.arg().unwrap_or_default();
                    dprintln!("neophone: opened on page {}", page);
                    ctx.m.borrow_mut().call_opened = false;
                    ctx.m.borrow_mut().settings_opened = false;
                    match Page::from_number(key::PHONE, page) {
                        Some(Page::Call) if !arg.is_empty() => {
                            ctx.m.borrow_mut().call_opened = true;
                            dial::place_call(&ctx, &arg);
                        }
                        Some(Page::Phone(PhoneView::Speed)) if !arg.is_empty() => {
                            ctx.m.borrow_mut().speed_opened = true;
                            dial::start_assign(&ctx, &arg);
                        }
                        // Settings at a section ("messages", "contacts"); Back returns to the app that opened it
                        Some(Page::Settings(_)) => {
                            ctx.m.borrow_mut().settings_opened = true;
                            prefs::open_at(&ctx, &arg);
                        }
                        // the footer's Phone tab in Contacts and Messages opens DIAL: Recents, the root
                        Some(Page::Phone(PhoneView::Dial)) => {
                            ctx.go(Page::Phone(PhoneView::Recents))
                        }
                        Some(p) => ctx.go(p),
                        None => {}
                    }
                }
                Some(AppEvent::Settings { .. }) => {
                    prefs::reload(&ctx);
                    // Contacts saved the book
                    let rev = contacts_rev(&ctx.app);
                    if rev != book_rev {
                        book_rev = rev;
                        ctx.m.borrow_mut().book = store::load(false).0;
                    }
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
                    let kf = ui.global::<phone_ui::KeyboardFocus>();
                    kf.set_dismissed(h == 0 && kf.get_open());
                    kf.set_text_h(h as f32);
                }
                Some(AppEvent::Answer { i }) => ctx.answer(i == 0),
                // a tap on a card's action in the shade (Call back, Message, End)
                Some(AppEvent::Notified { id, action }) => ctx.notified(id, action),
                // the navigation mode changed the app's height (Buttons 639 dp, Gestures 679 dp)
                Some(e @ AppEvent::Area { .. }) => ctx.app.handle(&e),
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

/// The book's save counter Contacts keeps (`contacts.rev`; None before its first save).
fn contacts_rev(app: &App) -> Option<i32> {
    match app.setting_raw(CONTACTS_REV) {
        Ok(Value::Int(n)) => Some(n),
        _ => None,
    }
}
