// SPDX-License-Identifier: GPL-3.0-only
//! Compiles NeoPhone's screens with the neoui kit (`@neoui/kit.slint`, from the neoui checkout cargo fetched for
//! neoui-kit): one theme and one set of components for the shell and this app.

fn main() {
    neoui_kit::compile("ui/app.slint");
}
