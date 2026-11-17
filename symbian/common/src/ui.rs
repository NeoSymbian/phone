// SPDX-License-Identifier: GPL-3.0-only
//! The UI helpers every app uses: a shell dialog, a view's header ([`Chrome`], [`tool`]), Slint models.

use alloc::string::String;
use alloc::vec::Vec;
use slint::{ModelRc, SharedString, VecModel};

/// A dialog for the UI server: `text` (and `detail` on its own line, if it fits), a red `confirm` and Cancel.
pub fn ask(text: &str, detail: &str, confirm: &str) -> neoapp::Query {
    let mut t = String::from(text);
    if !detail.is_empty() && t.len() + 1 + detail.len() <= neoui_proto::page::TEXT_MAX {
        t.push('\n');
        t.push_str(detail);
    }
    while t.len() > neoui_proto::page::TEXT_MAX {
        t.pop();
    }
    neoapp::Query {
        text: t,
        buttons: alloc::vec![confirm.into(), "Cancel".into()],
        destructive: true,
    }
}

/// A view's action glyph and style, for [`Chrome`] (the app draws its own header with the kit, ui-shared/frame.slint).
pub mod tool {
    pub const NONE: i32 = 0;
    pub const CALL: i32 = 1;
    pub const ADD: i32 = 2;
    pub const PERSON_ADD: i32 = 3;
    pub const EDIT: i32 = 4;
    pub const CHECK: i32 = 5;
    pub const DELETE: i32 = 6;
    /// the action is a big pill in the content (the dialer's Call, D14)
    pub const PILL: i32 = 1;
    /// a form: [✕ title ✓]
    pub const CANCEL: i32 = 2;
    /// no header at all (a call screen)
    pub const EMPTY: i32 = 4;
}

/// A view's title and action, as each area describes it (made into the kit header and the shell's page by the app's
/// `Ctx::sync`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Chrome {
    pub title: String,
    /// [`tool`]'s glyphs
    pub icon: i32,
    /// [`tool`]'s style bits: `CANCEL` a form, `PILL` the action in the content, `EMPTY` no action
    pub style: i32,
    pub label: String,
}

impl Chrome {
    pub fn titled(title: &str) -> Self {
        Self {
            title: title.into(),
            ..Self::default()
        }
    }
    pub fn with(mut self, icon: i32, style: i32, label: &str) -> Self {
        self.icon = icon;
        self.style = style;
        self.label = label.into();
        self
    }
}

pub fn model<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    ModelRc::from(alloc::rc::Rc::new(VecModel::from(items)))
}

pub fn strings(items: &[&str]) -> ModelRc<SharedString> {
    model(items.iter().map(|s| SharedString::from(*s)).collect())
}

/// A toast text cut to what NOTE takes.
pub fn note_text(text: &str) -> String {
    let mut t = String::from(text);
    while t.len() > neoui_proto::page::NOTE_TEXT_MAX {
        t.pop();
    }
    t
}
