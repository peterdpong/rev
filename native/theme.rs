use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, rgb, rgba};
use std::sync::atomic::{AtomicBool, Ordering};

/// Shared outer inset and gap for app controls.
pub const UI_SPACING: f32 = 5.5;
pub const CONTROL_HEIGHT: f32 = 28.;
pub const CONTROL_RADIUS: f32 = 6.;
pub const CONTROL_PADDING: f32 = 10.;
pub const CHOICE_ROW_HEIGHT: f32 = 40.;
pub const POPOVER_PADDING: f32 = UI_SPACING * 2.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub bg: u32,
    pub subpanel: u32,
    pub panel: u32,
    pub surface: u32,
    pub hover: u32,
    pub selected: u32,
    pub border: u32,
    pub control_border: u32,
    pub selected_border: u32,
    pub text: u32,
    pub muted: u32,
    pub focus: u32,
    pub strong: u32,
    pub success: u32,
    pub danger: u32,
    pub warning: u32,
    pub blue: u32,
    pub purple: u32,
    pub diff_added_bg: u32,
    pub diff_deleted_bg: u32,
    pub diff_added_emphasis: u32,
    pub diff_deleted_emphasis: u32,
    pub selection: u32,
}

pub const DARK: Palette = Palette {
    bg: 0x191919,
    subpanel: 0x1d1d1d,
    panel: 0x212121,
    surface: 0x262626,
    hover: 0x2d2d2d,
    selected: 0x363636,
    border: 0x303030,
    control_border: 0x414141,
    selected_border: 0x505050,
    text: 0xececec,
    muted: 0xa1a1a1,
    focus: 0x9b9b9b,
    strong: 0xffffff,
    success: 0x28c76f,
    danger: 0xf04b50,
    warning: 0xe3aa2e,
    blue: 0x58a6ff,
    purple: 0xbc8cff,
    diff_added_bg: 0x102d1e,
    diff_deleted_bg: 0x3a141a,
    diff_added_emphasis: 0x185c35,
    diff_deleted_emphasis: 0x752734,
    selection: 0x1f6feb40,
};

pub const LIGHT: Palette = Palette {
    bg: 0xffffff,
    subpanel: 0xfafafa,
    panel: 0xf5f5f5,
    surface: 0xffffff,
    hover: 0xf0f0f0,
    selected: 0xe8e8e8,
    border: 0xe5e5e5,
    control_border: 0xd4d4d4,
    selected_border: 0xbdbdbd,
    text: 0x1f1f1f,
    muted: 0x6b6b6b,
    focus: 0x8a8a8a,
    strong: 0x000000,
    success: 0x1a7f37,
    danger: 0xcf222e,
    warning: 0x9a6700,
    blue: 0x0969da,
    purple: 0x8250df,
    diff_added_bg: 0xe6ffec,
    diff_deleted_bg: 0xffebe9,
    diff_added_emphasis: 0xabf2bc,
    diff_deleted_emphasis: 0xffc1c0,
    selection: 0x0969da33,
};

static LIGHT_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn palette() -> &'static Palette {
    if LIGHT_ACTIVE.load(Ordering::Relaxed) {
        &LIGHT
    } else {
        &DARK
    }
}

pub fn init(cx: &mut App) {
    apply(false, cx);
}

pub fn apply(light: bool, cx: &mut App) {
    LIGHT_ACTIVE.store(light, Ordering::Relaxed);
    let p = palette();
    Theme::change(
        if light {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        },
        None,
        cx,
    );
    // Update the shared component and Base tokens together with the app palette.
    Theme::update(cx, |theme| {
        theme.background = rgb(p.bg).into();
        theme.foreground = rgb(p.text).into();
        theme.border = rgb(p.border).into();
        theme.muted = rgb(p.hover).into();
        theme.muted_foreground = rgb(p.muted).into();
        theme.accent = rgb(p.hover).into();
        theme.accent_foreground = rgb(p.text).into();
        theme.popover = rgb(p.surface).into();
        theme.popover_foreground = rgb(p.text).into();
        theme.input = rgb(p.control_border).into();
        theme.ring = rgb(p.focus).into();
        theme.caret = rgb(p.text).into();
        theme.selection = rgba(p.selection).into();

        theme.primary = rgb(p.text).into();
        theme.primary_foreground = rgb(p.bg).into();
        theme.primary_hover = rgb(p.strong).into();
        theme.primary_active = rgb(p.muted).into();
        theme.button_primary = theme.primary;
        theme.button_primary_foreground = theme.primary_foreground;
        theme.button_primary_hover = theme.primary_hover;
        theme.button_primary_active = theme.primary_active;

        theme.button = rgb(p.surface).into();
        theme.button_foreground = rgb(p.text).into();
        theme.button_hover = rgb(p.hover).into();
        theme.button_active = rgb(p.selected).into();
        theme.secondary = theme.button;
        theme.secondary_foreground = theme.button_foreground;
        theme.secondary_hover = theme.button_hover;
        theme.secondary_active = theme.button_active;
        theme.button_secondary = theme.secondary;
        theme.button_secondary_foreground = theme.secondary_foreground;
        theme.button_secondary_hover = theme.secondary_hover;
        theme.button_secondary_active = theme.secondary_active;

        theme.colors.list = rgb(p.bg).into();
        theme.list_head = rgb(p.subpanel).into();
        theme.list_even = rgb(p.subpanel).into();
        theme.list_hover = rgb(p.hover).into();
        theme.list_active = rgb(p.selected).into();
        theme.list_active_border = rgb(p.selected_border).into();
        theme.sidebar = rgb(p.panel).into();
        theme.sidebar_foreground = rgb(p.text).into();
        theme.sidebar_border = rgb(p.border).into();
        theme.sidebar_accent = rgb(p.selected).into();
        theme.sidebar_accent_foreground = rgb(p.text).into();
        theme.sidebar_primary = theme.primary;
        theme.sidebar_primary_foreground = theme.primary_foreground;
        theme.scrollbar = rgba(0x00000000).into();
        theme.scrollbar_thumb = rgba(0x77777788).into();
        theme.scrollbar_thumb_hover = rgb(0x777777).into();

        theme.success = rgb(p.success).into();
        theme.danger = rgb(p.danger).into();
        theme.warning = rgb(p.warning).into();
        theme.info = rgb(p.blue).into();
        theme.link = rgb(p.text).into();
        theme.link_hover = rgb(p.strong).into();
        theme.link_active = rgb(p.muted).into();
    });
    cx.refresh_windows();
}
