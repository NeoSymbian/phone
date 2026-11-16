// SPDX-License-Identifier: GPL-3.0-only
//! The snapshot harness: one software-rendered `PhoneApp` per test binary (Slint's platform is per process), drawn
//! at the Pixel's app area (1080 x 1917: 2160 minus the UI server's status bar and strip, dp = 3) into
//! `snapshots/<name>.png` at the repo root, for side-by-side review with the approved mockups.
#![allow(dead_code)]

use phone_ui::{PhoneApp, Theme};
use slint::ComponentHandle;
use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType, TargetPixel,
};
use slint::platform::{Platform, WindowAdapter};
use std::rc::Rc;

pub const W: u32 = 1080;
pub const H: u32 = 1917;

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
    pub ui: PhoneApp,
}

/// The app at the Pixel's size; call once per test binary.
pub fn app() -> Shot {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Host(window.clone())))
        .expect("one platform per process");
    let ui = PhoneApp::new().expect("PhoneApp");
    ui.global::<Theme>().set_dp(3.0);
    window.set_size(slint::PhysicalSize::new(W, H));
    ui.show().expect("show");
    Shot { window, ui }
}

impl Shot {
    /// Draw and save `snapshots/<name>.png`; returns the pixels for checks.
    pub fn shoot(&self, name: &str) -> Vec<u32> {
        NOW.set(NOW.get() + core::time::Duration::from_secs(1));
        slint::platform::update_timers_and_animations();
        self.window.request_redraw();
        let mut px = vec![Px(0); (W * H) as usize];
        self.window.draw_if_needed(|r| {
            r.render(&mut px, W as usize);
        });
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../snapshots");
        std::fs::create_dir_all(&root).unwrap();
        let file = std::fs::File::create(root.join(format!("{name}.png"))).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), W, H);
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
