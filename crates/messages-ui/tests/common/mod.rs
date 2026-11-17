// SPDX-License-Identifier: GPL-3.0-only
//! The snapshot harness: one software-rendered `MessagesPreview` (the app and the shell's toast and shade) per test binary (Slint's platform is per process), drawn
//! at the Pixel's app area (1080 x 2037: 2160 minus the UI server's status bar and the Gestures band, dp = 3) into
//! `snapshots/<name>.png` at the repo root, for side-by-side review with the approved mockups.
#![allow(dead_code)]

use messages_ui::{MessagesPreview, Theme};
use slint::ComponentHandle;
use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType, TargetPixel,
};
use slint::platform::{Platform, WindowAdapter};
use std::rc::Rc;

pub const W: u32 = 1080;
pub const H: u32 = 2037;

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
    pub ui: MessagesPreview,
    h: std::cell::Cell<u32>,
}

/// The app at the Pixel's size; call once per test binary.
pub fn app() -> Shot {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Host(window.clone())))
        .expect("one platform per process");
    let ui = MessagesPreview::new().expect("MessagesPreview");
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
    /// The app area's height in px: 2037 in Gestures, 1917 in Buttons mode (639 dp).
    pub fn resize(&self, h: u32) {
        self.h.set(h);
        self.window.set_size(slint::PhysicalSize::new(W, h));
    }

    /// A finger from (x0, y) to (x1, y), in px, in steps.
    pub fn drag(&self, x0: f32, x1: f32, y: f32) {
        use slint::platform::{PointerEventButton, WindowEvent};
        let w = self.window.window();
        let at = |x: f32| slint::LogicalPosition::new(x, y);
        w.dispatch_event(WindowEvent::PointerPressed {
            position: at(x0),
            button: PointerEventButton::Left,
        });
        for i in 1..=10 {
            NOW.set(NOW.get() + core::time::Duration::from_millis(16));
            slint::platform::update_timers_and_animations();
            w.dispatch_event(WindowEvent::PointerMoved {
                position: at(x0 + (x1 - x0) * i as f32 / 10.0),
            });
        }
        w.dispatch_event(WindowEvent::PointerReleased {
            position: at(x1),
            button: PointerEventButton::Left,
        });
    }

    /// A finger down at (x, y) in px, held `ms`, then up: a tap, or (500 ms and more) a long-press.
    pub fn press(&self, x: f32, y: f32, ms: u64) {
        use slint::platform::{PointerEventButton, WindowEvent};
        let w = self.window.window();
        let at = slint::LogicalPosition::new(x, y);
        w.dispatch_event(WindowEvent::PointerPressed {
            position: at,
            button: PointerEventButton::Left,
        });
        for _ in 0..ms / 50 {
            NOW.set(NOW.get() + core::time::Duration::from_millis(50));
            slint::platform::update_timers_and_animations();
        }
        w.dispatch_event(WindowEvent::PointerReleased {
            position: at,
            button: PointerEventButton::Left,
        });
        w.dispatch_event(WindowEvent::PointerExited);
    }

    /// A second on: timers fire, then a frame is drawn (and thrown away), so views that came up are built.
    pub fn tick(&self) {
        NOW.set(NOW.get() + core::time::Duration::from_secs(1));
        slint::platform::update_timers_and_animations();
        self.window.request_redraw();
        let mut px = vec![Px(0); (W * self.h.get()) as usize];
        self.window.draw_if_needed(|r| {
            r.render(&mut px, W as usize);
        });
    }

    /// Draw and save `snapshots/<name>.png`; returns the pixels for checks.
    pub fn shoot(&self, name: &str) -> Vec<u32> {
        NOW.set(NOW.get() + core::time::Duration::from_secs(1));
        slint::platform::update_timers_and_animations();
        self.window.request_redraw();
        let h = self.h.get();
        let mut px = vec![Px(0); (W * h) as usize];
        self.window.draw_if_needed(|r| {
            r.render(&mut px, W as usize);
        });
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../snapshots");
        std::fs::create_dir_all(&root).unwrap();
        let file = std::fs::File::create(root.join(format!("{name}.png"))).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), W, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        let rgb: Vec<u8> = px
            .iter()
            .flat_map(|p| [(p.0 >> 16) as u8, (p.0 >> 8) as u8, p.0 as u8])
            .collect();
        w.write_image_data(&rgb).unwrap();
        px.into_iter().map(|p| p.0).collect()
    }
}
