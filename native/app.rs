use gpui_kit::component::{
    Selectable, Sizable, TitleBar,
    button::{Button, ButtonVariants},
    diff::{Diff, DiffFile, DiffMode, DiffState},
};
use gpui_kit::prelude::*;
use gpui_kit::*;

gpui_kit::actions!(patch, [Quit]);

pub fn window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some("Patch".into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(16.), px(15.))),
        }),
        app_owns_titlebar_drag: true,
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(1100.), px(760.)),
            cx,
        ))),
        window_min_size: Some(size(px(760.), px(500.))),
        ..Default::default()
    }
}

pub struct Patch {
    focus: FocusHandle,
    pub(crate) diff: Entity<DiffState>,
    split: bool,
    wrap: bool,
    title: SharedString,
    pub(crate) error: Option<String>,
}
impl Patch {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::from_patch(
            include_str!("../fixtures/sample.diff"),
            "Sample diff".into(),
            window,
            cx,
        )
    }
    pub fn from_patch(
        patch: &str,
        title: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (files, error) = match DiffFile::parse(patch) {
            Ok(files) => (files, None),
            Err(error) => (vec![], Some(error.to_string())),
        };
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        Self {
            focus,
            diff: cx.new(|cx| DiffState::new(files, cx)),
            split: false,
            wrap: true,
            title,
            error,
        }
    }
}
impl Render for Patch {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("patch")
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x17191d))
            .text_color(rgb(0xe4e6eb))
            .text_size(px(13.))
            .font_family(".AppleSystemUIFont")
            .track_focus(&self.focus)
            .key_context("Patch")
            .child(
                TitleBar::new()
                    .h(px(44.))
                    .pl(px(80.))
                    .bg(rgb(0x131519))
                    .border_b_1()
                    .border_color(rgb(0x2b2e34))
                    .child("Patch"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(36.))
                    .px_3()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(self.title.clone()),
                    )
                    .child(
                        Button::new("view-mode")
                            .small()
                            .ghost()
                            .label(if self.split { "Split" } else { "Unified" })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.split = !this.split;
                                this.diff.update(cx, |diff, cx| {
                                    diff.set_mode(
                                        if this.split {
                                            DiffMode::Split
                                        } else {
                                            DiffMode::Unified
                                        },
                                        cx,
                                    )
                                });
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("wrap")
                            .small()
                            .ghost()
                            .label("Wrap")
                            .selected(self.wrap)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.wrap = !this.wrap;
                                cx.notify();
                            })),
                    ),
            )
            .when_some(self.error.clone(), |d, error| {
                d.child(div().p_3().text_color(rgb(0xee9292)).child(error))
            })
            .child(
                div().id("review-surface").flex_1().min_h_0().child(
                    Diff::new(&self.diff)
                        .size_full()
                        .soft_wrap(self.wrap)
                        .text_size(px(12.)),
                ),
            )
    }
}
