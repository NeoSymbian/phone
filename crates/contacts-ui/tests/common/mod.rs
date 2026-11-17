// SPDX-License-Identifier: GPL-3.0-only
//! The snapshot harness: one software-rendered `ContactsApp` per test binary (Slint's platform is per process), drawn
//! at the Pixel's app area (1080 x 2037: 2160 minus the UI server's status bar and the Gestures band, dp = 3) into
//! `snapshots/<name>.png` at the repo root, for side-by-side review with the approved mockups.
#![allow(dead_code)]

use contacts_ui::{ContactsApp, Theme};
use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType, TargetPixel,
};
use slint::platform::{Platform, WindowAdapter};
use slint::platform::{PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition};
use std::rc::Rc;

pub const W: u32 = 1080;
pub const H: u32 = 2037;
pub const DP: f32 = 3.0;

struct Host(Rc<MinimalSoftwareWindow>);

thread_local! {
    /// The test clock: each shot moves it a second on, so animations (a sheet opening or closing) have finished.
    static NOW: std::cell::Cell<core::time::Duration> = const { std::cell::Cell::new(core::time::Duration::ZERO) };
}

impl Platform for Host {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
    fn duration_since_start(&self) -> core::time::Duration {
        NOW.get()
    }
}

#[derive(Clone, Copy, Default)]
struct Px(u32);

impl TargetPixel for Px {
    fn blend(&mut self, c: PremultipliedRgbaColor) {
        let a = 255 - u32::from(c.alpha);
        let ch = |s: u32, v: u8| (u32::from(v) + ((self.0 >> s) & 0xff) * a / 255).min(255);
        self.0 = ch(16, c.red) << 16 | ch(8, c.green) << 8 | ch(0, c.blue);
    }
    fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Px(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
    }
}

pub struct Shot {
    window: Rc<MinimalSoftwareWindow>,
    pub ui: ContactsApp,
    /// the app's height now: 2037 in Gestures (679 dp), 1917 in Buttons mode (639 dp)
    h: std::cell::Cell<u32>,
}

/// The app at the Pixel's size; call once per test binary.
pub fn app() -> Shot {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Host(window.clone())))
        .expect("one platform per process");
    let ui = ContactsApp::new().expect("ContactsApp");
    ui.global::<Theme>().set_dp(3.0);
    window.set_size(slint::PhysicalSize::new(W, H));
    ui.show().expect("show");
    Shot {
        window,
        ui,
        h: std::cell::Cell::new(H),
    }
}

impl Shot {
    /// The app's height in px (the navigation mode changes it).
    pub fn resize(&self, h: u32) {
        self.h.set(h);
        self.window.set_size(slint::PhysicalSize::new(W, h));
    }

    /// A finger down at (x, y) dp, and up again when `release`.
    pub fn press(&self, x: f32, y: f32, release: bool) {
        let w = self.ui.window();
        let position = LogicalPosition::new(x * DP, y * DP);
        w.dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        if release {
            w.dispatch_event(WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Left,
            });
        }
    }

    /// The finger up at (x, y) dp.
    pub fn release(&self, x: f32, y: f32) {
        self.ui
            .window()
            .dispatch_event(WindowEvent::PointerReleased {
                position: LogicalPosition::new(x * DP, y * DP),
                button: PointerEventButton::Left,
            });
    }

    /// Let `ms` pass (Slint's timers: a long press).
    pub fn wait(&self, ms: u64) {
        NOW.set(NOW.get() + core::time::Duration::from_millis(ms));
        slint::platform::update_timers_and_animations();
    }

    /// Draw and save `snapshots/<name>.png`; returns the pixels for checks.
    pub fn shoot(&self, name: &str) -> Vec<u32> {
        let px = self.pixels();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../snapshots");
        std::fs::create_dir_all(&root).unwrap();
        let file = std::fs::File::create(root.join(format!("{name}.png"))).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), W, self.h.get());
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        let rgb: Vec<u8> = px
            .iter()
            .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8])
            .collect();
        w.write_image_data(&rgb).unwrap();
        px
    }

    /// Draw (timers and animations first) and return the pixels, 0xRRGGBB by rows of [`W`].
    pub fn pixels(&self) -> Vec<u32> {
        // change handlers run first (they may start an animation), then a second passes
        slint::platform::update_timers_and_animations();
        NOW.set(NOW.get() + core::time::Duration::from_secs(1));
        slint::platform::update_timers_and_animations();
        self.window.request_redraw();
        let h = self.h.get();
        let mut px = vec![Px(0); (W * h) as usize];
        self.window.draw_if_needed(|r| {
            r.render(&mut px, W as usize);
        });
        px.into_iter().map(|p| p.0).collect()
    }
}
