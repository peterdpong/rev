use super::*;
use super::controls::*;
use crate::theme::*;
use gpui_kit::{
    assets::IconName,
    base::Button as BaseButton,
    component::{ActiveTheme as _, Icon, tooltip::Tooltip},
};

pub(super) const BRANCH_PILL_HEIGHT: f32 = 20.;
const BRANCH_COPIED_DURATION: std::time::Duration = std::time::Duration::from_millis(1500);

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
                .child(CopyBranch { name: head }),
        )
}

#[derive(IntoElement)]
struct CopyBranch {
    name: String,
}

#[derive(Default)]
struct CopyBranchState {
    copied: bool,
    reset: Option<Task<()>>,
}

impl RenderOnce for CopyBranch {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state("copy-branch", cx, |_, _| CopyBranchState::default());
        let copied = state.read(cx).copied;
        let name = self.name;
        let label = if copied { "Copied" } else { "Copy branch name" };
        BaseButton::new("copy-branch")
            .debug_selector(|| "copy-branch".into())
            .h_full()
            .flex_shrink_0()
            .px(px(5.))
            .border_l_1()
            .border_color(rgb(CONTROL_BORDER))
            .text_color(rgb(MUTED))
            .hover(|d| d.bg(rgb(HOVER)).text_color(rgb(TEXT)))
            .accessibility_label(label)
            .tooltip(move |window, cx| Tooltip::new(label).build(window, cx))
            .child(
                Icon::new(if copied {
                    IconName::Check
                } else {
                    IconName::Copy
                })
                .size(px(12.)),
            )
            .on_click(window.listener_for(&state, move |state, _, window, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(name.clone()));
                state.copied = true;
                state.reset = Some(cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(BRANCH_COPIED_DURATION).await;
                    let _ = this.update(cx, |state, cx| {
                        state.copied = false;
                        cx.notify();
                    });
                }));
                cx.notify();
            }))
    }
}
