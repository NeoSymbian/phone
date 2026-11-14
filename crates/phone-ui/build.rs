// SPDX-License-Identifier: GPL-3.0-only
//! Compiles NeoPhone's screens. neoui's theme, fonts and components come from the neoui checkout cargo fetched for
//! neoui-proto (DEP_NEOUI_UI), imported as `@neoui/...`: one theme for the shell and this app.

fn main() {
    let ui = std::env::var("DEP_NEOUI_UI")
        .expect("neoui-proto exports DEP_NEOUI_UI (neoui 0802b52 or later)");
    let libs =
        std::collections::HashMap::from([(String::from("neoui"), std::path::PathBuf::from(ui))]);
    slint_build::compile_with_config(
        "ui/app.slint",
        slint_build::CompilerConfiguration::new()
            .embed_resources(slint_build::EmbedResourcesKind::EmbedForSoftwareRenderer)
            .with_sdf_fonts(true)
            .with_library_paths(libs),
    )
    .expect("compile ui/app.slint");
}
