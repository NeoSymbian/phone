// SPDX-License-Identifier: GPL-3.0-only
//! Compiles the screens with the neoui kit (`@neoui`) and the parts Phone, Contacts and Messages share
//! (`@phone-shared`, the repo's `ui-shared/`).

fn main() {
    let shared = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ui-shared");
    neoui_kit::compile_with("ui/app.slint", &[("phone-shared", shared)]);
}
