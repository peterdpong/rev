//! Rev's independent renderer. The only UI dependency used here is GPUI's
//! element/text/input API; document and projection logic live in `rev-diff`.
mod state;
mod view;

pub use rev_diff::{
    File as DiffFile, Mode as DiffMode, Position as DiffLinePosition, Selection as DiffLineRange,
    Side as DiffSide,
};
pub use state::{DiffEvent, DiffScrollPosition, DiffState};
pub use view::Diff;

use gpui::{App, KeyBinding};

gpui::actions!(
    rev_diff_prototype,
    [
        Copy, SelectAll, Up, Down, PageUp, PageDown, First, Last, ExtendUp, ExtendDown
    ]
);

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-c", Copy, Some("RevDiffPrototype")),
        KeyBinding::new("secondary-a", SelectAll, Some("RevDiffPrototype")),
        KeyBinding::new("up", Up, Some("RevDiffPrototype")),
        KeyBinding::new("down", Down, Some("RevDiffPrototype")),
        KeyBinding::new("pageup", PageUp, Some("RevDiffPrototype")),
        KeyBinding::new("pagedown", PageDown, Some("RevDiffPrototype")),
        KeyBinding::new("home", First, Some("RevDiffPrototype")),
        KeyBinding::new("end", Last, Some("RevDiffPrototype")),
        KeyBinding::new("shift-up", ExtendUp, Some("RevDiffPrototype")),
        KeyBinding::new("shift-down", ExtendDown, Some("RevDiffPrototype")),
    ]);
}
