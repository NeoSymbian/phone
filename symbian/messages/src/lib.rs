// SPDX-License-Identifier: GPL-3.0-only
//! NEOMSG.EXE: Messages (NeoSymbian/phone), one of three apps with Phone (NEOPHONE) and Contacts (NEOCONT). The UI server starts it hidden at boot and keeps it
//! running (neoui `host::RESIDENT`): it posts a shade entry per conversation as texts arrive (Reply, Call). Phone and Contacts open
//! it on a conversation (`AppEvent::Open`); a thread's Call and Contact open them.
//!
//! - [`ctx`]: the state and the shared moves; `phone_common`: MODEMD's client, the contacts file, settings, blocking
//! - [`msgs`]: the screens' behaviour
//! - this file: connecting, loading, and the event loop (input, pages, MODEMD's status)
#![no_std]

extern crate alloc;

mod ctx;
mod msgs;

use alloc::rc::Rc;
use core::pin::pin;
use ctx::{Ctx, Model};
use e32::ipc::TIpcArgs;
use e32::rt::{self, Either};
use e32::sys::TPtr8;
use e32::{dprintln, time};
use messages_ui::{Kit, MessagesApp, Nav, Theme};
use neoapp::App;
use neoui_proto::InputEvent;
use neoui_proto::ipc::{self as proto, AppEvent};
use neoui_proto::settings::Value;
use phone_common::{CONTACTS_REV, modem, store};
use phone_core::page::{MessagesView, Page, PhoneView, app as key};
use slint::ComponentHandle;

e32::main!(main);

fn main() -> i32 {
    match run() {
        Ok(()) => 0,
        Err(e) => {
            dprintln!("neomsg: failed {}", e);
            e
        }
    }
}

fn run() -> Result<(), i32> {
    let app = App::connect()?;
    dprintln!("neomsg: connected, {}x{}", app.width, app.height());
    let ui = MessagesApp::new().map_err(|_| -2)?;
    ui.global::<Theme>().set_dp(app.dp);
    ui.show().map_err(|_| -2)?;
    for prefix in ["phone.", "contacts."] {
        if let Err(e) = app.watch_settings(prefix) {
            dprintln!("neomsg: cannot watch {} ({})", prefix, e);
        }
    }
    // Contacts (NEOCONT) writes the book; this app reads it again when contacts.rev changes
    let (book, _) = store::load(false);
    let mut book_rev = contacts_rev(&app);
    let mut modem = modem::Modem::default();
    let block = modem.block_list().unwrap_or_default();
    let model = Model {
        page: Page::Messages(MessagesView::List),
        modem,
        status: None,
        book,
        order: contacts::NameOrder::FirstLast,
        log: alloc::vec::Vec::new(),
        log_revision: None,
        block,
        prefs: Default::default(),
        msgs: Default::default(),
        visible: false,
        opened: false,
    };
    let ctx = Rc::new(Ctx::new(app, &ui, model));
    ctx.reload_prefs();
    msgs::load(&ctx);
    msgs::sync(&ctx);
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
                "action" => ctx.header_action(usize::try_from(i).unwrap_or(usize::MAX)),
                "selection" => ctx.selection_action(usize::try_from(i).unwrap_or(usize::MAX)),
                // ✕ in selection mode: Back clears it
                "selection-closed" => {
                    ctx.back();
                }
                "tab" => match i {
                    0 => ctx.go(Page::Phone(PhoneView::Recents)),
                    1 => ctx.go(Page::Contacts(phone_core::page::ContactsView::List)),
                    2 => ctx.go(Page::Messages(MessagesView::List)),
                    _ => {}
                },
                _ => ctx.option_at(usize::try_from(i).unwrap_or(usize::MAX)),
            }
        });
    }
    msgs::bind(&ctx, &ui);
    ctx.go(Page::Messages(MessagesView::List));
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
                            time::delay(phone_common::wait_us(&ctx.app, msgs::wait_us(&ctx))),
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
                        // a swiped delete whose toast is gone
                        Either::Right(Either::Right(())) => {
                            msgs::tick(&ctx);
                            ctx.sync()?;
                        }
                    }
                }
            };
            got?;
            match AppEvent::decode(&ev) {
                Some(AppEvent::Quit) => {
                    dprintln!("neomsg: quit");
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
                    dprintln!("neomsg: opened on page {}", page);
                    if let Some(p) = Page::from_number(key::MESSAGES, page) {
                        opened(&ctx, page, p, &arg);
                    }
                }
                // a tap on our entry in the shade, or on its Reply / Call
                Some(AppEvent::Notified { id, action }) => msgs::notified(&ctx, id, action),
                // the toast's Undo (a swiped conversation)
                Some(AppEvent::NoteAction) => msgs::undo(&ctx),
                Some(AppEvent::Settings { .. }) => {
                    ctx.reload_prefs();
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
                    let kf = ui.global::<messages_ui::KeyboardFocus>();
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

/// The book's save counter Contacts keeps (`contacts.rev`; None before its first save).
fn contacts_rev(app: &App) -> Option<i32> {
    match app.setting_raw(CONTACTS_REV) {
        Ok(Value::Int(n)) => Some(n),
        _ => None,
    }
}

/// Phone or Contacts opened Messages on a page with a number: the conversation (Reply: the composer takes the focus);
/// else the page.
fn opened(ctx: &Ctx, n: u16, p: Page, arg: &str) {
    use phone_core::page::number::messages;
    match n {
        messages::THREAD | messages::WRITE if !arg.is_empty() => {
            msgs::open_thread(ctx, arg, n == messages::WRITE);
            ctx.m.borrow_mut().opened = true;
        }
        _ => {
            ctx.m.borrow_mut().opened = false;
            ctx.go(p);
        }
    }
}
