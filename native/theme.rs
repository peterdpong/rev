use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, rgb, rgba};

// Neutral graphite surfaces, ordered from canvas to raised controls.
pub const BG: u32 = 0x191919;
pub const SUBPANEL: u32 = 0x1d1d1d;
pub const PANEL: u32 = 0x212121;
pub const SURFACE: u32 = 0x262626;
pub const HOVER: u32 = 0x2d2d2d;
pub const SELECTED: u32 = 0x363636;
pub const BORDER: u32 = 0x303030;
pub const CONTROL_BORDER: u32 = 0x414141;
pub const SELECTED_BORDER: u32 = 0x505050;
pub const TEXT: u32 = 0xececec;
pub const MUTED: u32 = 0xa1a1a1;
pub const FOCUS: u32 = 0x9b9b9b;

// Color is reserved for file status and diff meaning.
pub const SUCCESS: u32 = 0x4eaa78;
pub const DANGER: u32 = 0xd46b73;
pub const WARNING: u32 = 0xd3bb8b;
pub const BLUE: u32 = 0x9cabc4;
pub const PURPLE: u32 = 0xbca7cf;

pub fn init(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
    // Update the shared component and Base tokens together with the app palette.
    Theme::update(cx, |theme| {
        theme.background = rgb(BG).into();
        theme.foreground = rgb(TEXT).into();
        theme.border = rgb(BORDER).into();
        theme.muted = rgb(HOVER).into();
        theme.muted_foreground = rgb(MUTED).into();
        theme.accent = rgb(HOVER).into();
        theme.accent_foreground = rgb(TEXT).into();
        theme.popover = rgb(SURFACE).into();
        theme.popover_foreground = rgb(TEXT).into();
        theme.input = rgb(CONTROL_BORDER).into();
        theme.ring = rgb(FOCUS).into();
        theme.caret = rgb(TEXT).into();
        theme.selection = rgba(0x8b8b8b55).into();

        theme.primary = rgb(TEXT).into();
        theme.primary_foreground = rgb(BG).into();
        theme.primary_hover = rgb(0xffffff).into();
        theme.primary_active = rgb(0xd0d0d0).into();
        theme.button_primary = theme.primary;
        theme.button_primary_foreground = theme.primary_foreground;
        theme.button_primary_hover = theme.primary_hover;
        theme.button_primary_active = theme.primary_active;

        theme.button = rgb(SURFACE).into();
        theme.button_foreground = rgb(TEXT).into();
        theme.button_hover = rgb(HOVER).into();
        theme.button_active = rgb(SELECTED).into();
        theme.secondary = theme.button;
        theme.secondary_foreground = theme.button_foreground;
        theme.secondary_hover = theme.button_hover;
        theme.secondary_active = theme.button_active;
        theme.button_secondary = theme.secondary;
        theme.button_secondary_foreground = theme.secondary_foreground;
        theme.button_secondary_hover = theme.secondary_hover;
        theme.button_secondary_active = theme.secondary_active;

        theme.colors.list = rgb(BG).into();
        theme.list_head = rgb(SUBPANEL).into();
        theme.list_even = rgb(SUBPANEL).into();
        theme.list_hover = rgb(HOVER).into();
        theme.list_active = rgb(SELECTED).into();
        theme.list_active_border = rgb(SELECTED_BORDER).into();
        theme.sidebar = rgb(PANEL).into();
        theme.sidebar_foreground = rgb(TEXT).into();
        theme.sidebar_border = rgb(BORDER).into();
        theme.sidebar_accent = rgb(SELECTED).into();
        theme.sidebar_accent_foreground = rgb(TEXT).into();
        theme.sidebar_primary = theme.primary;
        theme.sidebar_primary_foreground = theme.primary_foreground;
        theme.scrollbar = rgba(0x00000000).into();
        theme.scrollbar_thumb = rgba(0x77777788).into();
        theme.scrollbar_thumb_hover = rgb(0x777777).into();

        theme.success = rgb(SUCCESS).into();
        theme.danger = rgb(DANGER).into();
        theme.warning = rgb(WARNING).into();
        theme.info = rgb(BLUE).into();
        theme.link = rgb(TEXT).into();
        theme.link_hover = rgb(0xffffff).into();
        theme.link_active = rgb(MUTED).into();
    });
}
