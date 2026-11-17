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

/// A view's action glyphs and style, for [`Chrome`] (the app draws its own header with the kit, ui-shared/frame.slint;
/// the numbers are ui-shared/icons.slint's).
pub mod tool {
    pub const NONE: i32 = 0;
    pub const CALL: i32 = 1;
    pub const ADD: i32 = 2;
    pub const PERSON_ADD: i32 = 3;
    pub const EDIT: i32 = 4;
    pub const CHECK: i32 = 5;
    pub const DELETE: i32 = 6;
    pub const STAR: i32 = 7;
    pub const STAR_FULL: i32 = 8;
    pub const MESSAGE: i32 = 9;
    pub const BLOCK: i32 = 10;
    pub const SELECT_ALL: i32 = 11;
    pub const ARCHIVE: i32 = 12;
    pub const SHARE: i32 = 13;
    pub const SEARCH: i32 = 14;
    pub const KEYPAD: i32 = 15;
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
    /// more header icons after the action (a card's ★ and ✎): a [`tool`] glyph and the name the area's `option`
    /// runs when it is tapped (as if picked from ⋮)
    pub more: Vec<(i32, &'static str)>,
    /// the header's search: its placeholder (None: no 🔍); the typed text goes to the area's `searched`
    pub search: Option<&'static str>,
    /// selection mode: how many are selected (0: off; the header shows [✕ N selected · `selection`]), and its
    /// actions as [`tool`] glyphs and `option` names; ✕ is Back
    pub selected: i32,
    pub selection: Vec<(i32, &'static str)>,
    /// selection mode with nothing ticked yet (⋮ ▸ Select): the selection header shows 0
    pub selecting: bool,
    /// ⋮ in selection mode (Select all, Export): the `option` names
    pub selection_menu: Vec<&'static str>,
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

    /// The action is a header icon (not a form's ✓, not the content's pill, not none).
    pub fn icon_action(&self) -> bool {
        self.icon != tool::NONE && self.style & (tool::PILL | tool::CANCEL | tool::EMPTY) == 0
    }

    /// What the header's icon `i` does: `None` the action (the area's `action`), `Some(name)` an `option`.
    pub fn header_action(&self, i: usize) -> Option<Option<&'static str>> {
        if self.icon_action() {
            if i == 0 {
                return Some(None);
            }
            return self.more.get(i - 1).map(|(_, n)| Some(*n));
        }
        self.more.get(i).map(|(_, n)| Some(*n))
    }
}

pub fn model<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    ModelRc::from(alloc::rc::Rc::new(VecModel::from(items)))
}

pub fn strings(items: &[&str]) -> ModelRc<SharedString> {
    model(items.iter().map(|s| SharedString::from(*s)).collect())
}

/// The header title's font: DejaVu Sans when it has a script Inter lacks (Armenian; Slint has no fallback), else the
/// window's ("").
pub fn title_font(title: &str) -> &'static str {
    if title.chars().any(|c| ('\u{531}'..='\u{58f}').contains(&c)) {
        "DejaVu Sans"
    } else {
        ""
    }
}

/// A toast text cut to what NOTE takes.
pub fn note_text(text: &str) -> String {
    let mut t = String::from(text);
    while t.len() > neoui_proto::page::NOTE_TEXT_MAX {
        t.pop();
    }
    t
}
