use super::*;
use super::controls::*;
use crate::theme::*;
use gpui_kit::{
    assets::IconName,
    component::{ActiveTheme as _, Icon},
};

pub(super) const BRANCH_PILL_HEIGHT: f32 = 20.;

fn branch_pill() -> Div {
    row()
        .h(px(BRANCH_PILL_HEIGHT))
        .min_w_0()
        .gap_0()
        .rounded(px(CONTROL_RADIUS))
        .border_1()
        .border_color(rgb(CONTROL_BORDER))
        .bg(rgb(SURFACE))
        .overflow_hidden()
}

fn branch_name(name: String, cx: &App) -> Div {
    div()
        .min_w_0()
        .px(px(HEADER_PILL_PADDING))
        .truncate()
        .font_family(cx.theme().mono_font_family.clone())
        .text_size(px(12.))
        .text_color(rgb(TEXT))
        .child(name)
}

pub(super) fn branch_comparison(base: String, head: String, cx: &App) -> Stateful<Div> {
    row()
        .id("branch-comparison")
        .min_w_0()
        .gap(px(UI_SPACING))
        .child(
            branch_pill()
                .debug_selector(|| "branch-base".into())
                .flex_shrink_1()
                .child(branch_name(base, cx)),
        )
        .child(
            Icon::new(IconName::ArrowLeft)
                .size(px(12.))
                .flex_shrink_0()
                .text_color(rgb(MUTED)),
        )
        .child(
            branch_pill()
                .debug_selector(|| "branch-head".into())
                .flex_shrink_1()
                .child(branch_name(head.clone(), cx))
                .child(
                    CopyButton::new("copy-branch", head, "Copy branch name")
                        .h_full()
                        .px(px(5.))
                        .border_l_1()
                        .border_color(rgb(CONTROL_BORDER)),
                ),
        )
}
