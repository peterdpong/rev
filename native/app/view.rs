use super::*;
use crate::theme::*;
use gpui_kit::{
    assets::IconName,
    base::{Button as BaseButton, Popup, motion},
    component::{Icon, TitleBar, tooltip::Tooltip},
};

const SIDEBAR_WIDTH: f32 = 225.;
const HEADER_HEIGHT: f32 = 38.;
const LIST_ROW_HEIGHT: f32 = 26.;
const HEADER_PILL_HEIGHT: f32 = 26.;
// Shared inset matches the centered header pills, excluding the bottom border.
const UI_SPACING: f32 = (HEADER_HEIGHT - 1. - HEADER_PILL_HEIGHT) / 2.;
const HEADER_PILL_PADDING: f32 = 10.;
const HEADER_PILL_LINE_HEIGHT: f32 = 16.;
const PR_DESCRIPTION_GAP: f32 = 2.;

fn row() -> Div {
    div().flex().items_center().gap_2()
}

fn column() -> Div {
    div().flex().flex_col()
}

fn selected_surface<T: Styled>(element: T) -> T {
    element
        .bg(rgb(SELECTED))
        .border_1()
        .border_color(rgb(SELECTED_BORDER))
        .text_color(rgb(TEXT))
        .shadow_sm()
}

fn button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id).label(label).small().ghost()
}

fn icon_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    icon: IconName,
) -> BaseButton {
    let label = label.into();
    BaseButton::new(id)
        .size(px(24.))
        .flex_shrink_0()
        .rounded_md()
        .border_1()
        .border_color(rgba(0x00000000))
        .text_color(rgb(MUTED))
        .hover(|d| d.text_color(rgb(TEXT)))
        .focus_visible(|d| d.border_color(rgb(FOCUS)))
        .styles(|styles| styles.disabled(|d| d.opacity(0.5)))
        .child(Icon::new(icon).size(px(14.)))
        .accessibility_label(label.clone())
        .tooltip(move |window, cx| Tooltip::new(label.clone()).build(window, cx))
}

impl Rev {
    fn review_header(&self, _: &mut Context<Self>) -> TitleBar {
        TitleBar::new()
            .h(px(HEADER_HEIGHT))
            .pl_0()
            .bg(rgb(BG))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(div().w(px(SIDEBAR_WIDTH)).h_full().bg(rgb(PANEL)))
    }

    fn sidebar(&mut self, _: &mut Context<Self>) -> AnyElement {
        div()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .bg(rgb(PANEL))
            .into_any_element()
    }

    fn diff_view_header(&self, _: &mut Context<Self>) -> AnyElement {
        div().into_any_element()
    }

    fn file_list(&mut self, _: &mut Context<Self>) -> AnyElement {
        div().into_any_element()
    }

    fn diff_view(&mut self, _: &mut Context<Self>) -> AnyElement {
        Diff::new(&self.diff)
            .size_full()
            .header_visible(false)
            .soft_wrap(self.wrap)
            .into_any_element()
    }

    fn dialog(&self, _: &mut Window, _: &mut Context<Self>) -> AnyElement {
        div().into_any_element()
    }
}

impl Render for Rev {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content =
            if self.review.is_some() {
                column()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(self.diff_view_header(cx))
                    .child(
                        row()
                            .items_start()
                            .gap_0()
                            .flex_1()
                            .min_h_0()
                            .child(self.file_list(cx))
                            .child(self.diff_view(cx)),
                    )
                    .into_any_element()
            } else {
                column()
                    .flex_1()
                    .h_full()
                    .items_center()
                    .justify_center()
                    .gap_4()
                    .child(
                        div()
                            .text_size(px(24.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Open a review"),
                    )
                    .child(row().child(
                        Button::new("welcome-open").label("Open folder").on_click(
                            cx.listener(|this, _, window, cx| this.pick_folder(window, cx)),
                        ),
                    ))
                    .into_any_element()
            };
        let status = if self.modal.is_none() {
            self.error.clone()
        } else {
            None
        };
        column()
            .id("rev")
            .relative()
            .size_full()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .text_size(px(13.))
            .font_family(".AppleSystemUIFont")
            .track_focus(&self.focus)
            .key_context("Rev")
            .on_action(
                cx.listener(|this, _: &OpenProject, window, cx| this.pick_folder(window, cx)),
            )
            .on_action(cx.listener(|this, _: &Refresh, window, cx| this.refresh(window, cx)))
            .on_action(
                cx.listener(|this, _: &NextFile, window, cx| this.navigate_file(1, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &PreviousFile, window, cx| {
                    this.navigate_file(-1, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &MarkViewed, window, cx| {
                if this.modal.is_none() {
                    this.mark_viewed(window, cx);
                }
            }))
            .child(self.review_header(cx))
            .child(
                row()
                    .items_start()
                    .gap_0()
                    .flex_1()
                    .min_h_0()
                    .child(self.sidebar(cx))
                    .child(content),
            )
            .when_some(status, |d, error| {
                d.child(
                    row()
                        .px_4()
                        .py_2()
                        .border_t_1()
                        .border_color(rgb(BORDER))
                        .child(div().flex_1().text_color(rgb(DANGER)).child(error))
                        .child(button("dismiss-error", "×").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.error = None;
                                cx.notify();
                            },
                        ))),
                )
            })
            .when(self.modal.is_some(), |d| d.child(self.dialog(window, cx)))
    }
}
