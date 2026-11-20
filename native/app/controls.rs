use crate::theme::*;
use gpui_kit::prelude::*;
use gpui_kit::*;
use gpui_kit::{
    assets::IconName,
    base::{Button as BaseButton, motion},
    component::{Icon, tooltip::Tooltip},
};

pub(super) const LIST_ROW_HEIGHT: f32 = 26.;
pub(super) const HEADER_PILL_LINE_HEIGHT: f32 = 16.;
pub(super) const ICON_BUTTON_SIZE: f32 = 24.;
pub(super) const HEADER_PILL_HEIGHT: f32 = 26.;
// Include the shared outer inset above/below the pills and the bottom border.
pub(super) const HEADER_HEIGHT: f32 = HEADER_PILL_HEIGHT + UI_SPACING * 2. + 1.;
pub(super) const HEADER_PILL_PADDING: f32 = 10.;
const FILE_HEADER_HEIGHT: f32 = CONTROL_HEIGHT + UI_SPACING * 2. + 1.;

#[derive(IntoElement)]
pub(super) struct FolderIcon {
    pub(super) id: SharedString,
    pub(super) expanded: bool,
}

impl RenderOnce for FolderIcon {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let openness = motion::transition(
            (self.id, "folder-openness"),
            if self.expanded { 1.0_f32 } else { 0.0_f32 },
            motion::Transition::new(std::time::Duration::from_millis(160)),
            window,
            cx,
        );

        div()
            .relative()
            .size(px(14.))
            .flex_shrink_0()
            .text_color(rgb(MUTED))
            .child(
                Icon::new(IconName::FolderClosed)
                    .absolute()
                    .top_0()
                    .left_0()
                    .size(px(14.))
                    .opacity(1.0 - openness),
            )
            .child(
                Icon::new(IconName::FolderOpen)
                    .absolute()
                    .top_0()
                    .left_0()
                    .size(px(14.))
                    .opacity(openness),
            )
    }
}

pub(super) fn row() -> Div {
    div().flex().items_center().gap_2()
}

pub(super) fn column() -> Div {
    div().flex().flex_col()
}

pub(super) fn file_header_row() -> Div {
    row()
        .h(px(FILE_HEADER_HEIGHT))
        .flex_shrink_0()
        .px(px(UI_SPACING))
        .gap(px(UI_SPACING))
        .bg(rgb(SUBPANEL))
        .border_b_1()
        .border_color(rgb(BORDER))
        .text_size(px(12.))
        .line_height(px(HEADER_PILL_LINE_HEIGHT))
}

pub(super) fn selected_surface<T: Styled>(element: T) -> T {
    element
        .bg(rgb(SELECTED))
        .border_1()
        .border_color(rgb(SELECTED_BORDER))
        .text_color(rgb(TEXT))
        .shadow_sm()
}

pub(super) fn viewed_button(viewed: bool, disabled: bool) -> BaseButton {
    BaseButton::new("viewed")
        .h(px(CONTROL_HEIGHT))
        .px(px(CONTROL_PADDING))
        .gap(px(UI_SPACING))
        .flex_shrink_0()
        .rounded(px(CONTROL_RADIUS))
        .border_1()
        .border_color(rgb(CONTROL_BORDER))
        .bg(rgb(SURFACE))
        .text_color(rgb(TEXT))
        .text_size(px(13.))
        .font_weight(FontWeight::SEMIBOLD)
        .when(viewed, selected_surface)
        .when(!disabled, |d| {
            d.hover(move |d| d.bg(rgb(if viewed { SELECTED } else { HOVER })))
        })
        .focus_visible(|d| d.border_color(rgb(FOCUS)))
        .styles(|styles| styles.disabled(|d| d.opacity(0.5)))
        .disabled(disabled)
        .role(accesskit::Role::CheckBox)
        .aria_toggled(if viewed {
            accesskit::Toggled::True
        } else {
            accesskit::Toggled::False
        })
        .accessibility_label("Viewed")
        .child(
            div()
                .size(px(14.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(2.))
                .border_1()
                .border_color(rgb(MUTED))
                .when(viewed, |d| {
                    d.bg(rgb(0x737373)).border_color(rgb(FOCUS)).child(
                        Icon::new(IconName::Check)
                            .size(px(12.))
                            .text_color(rgb(0xffffff)),
                    )
                }),
        )
        .child("Viewed")
}

pub(super) fn icon_actions() -> Div {
    row().gap(px(UI_SPACING)).flex_shrink_0()
}

pub(super) fn header_pill() -> Div {
    row()
        .h(px(HEADER_PILL_HEIGHT))
        .min_w_0()
        .px(px(HEADER_PILL_PADDING))
        .gap(px(6.))
        .rounded(px(CONTROL_RADIUS))
        .bg(rgb(SURFACE))
        .border_1()
        .border_color(rgb(CONTROL_BORDER))
        .text_size(px(12.))
        .line_height(px(HEADER_PILL_LINE_HEIGHT))
}

fn button_base(id: impl Into<ElementId>, label: impl Into<SharedString>) -> BaseButton {
    let label = label.into();
    BaseButton::new(id)
        .h(px(CONTROL_HEIGHT))
        .px(px(CONTROL_PADDING))
        .gap(px(UI_SPACING))
        .flex_shrink_0()
        .rounded(px(CONTROL_RADIUS))
        .border_1()
        .border_color(rgb(CONTROL_BORDER))
        .bg(rgb(SURFACE))
        .text_color(rgb(TEXT))
        .text_size(px(12.))
        .focus_visible(|d| d.border_color(rgb(FOCUS)))
        .styles(|styles| styles.disabled(|d| d.opacity(0.5)))
        .accessibility_label(label.clone())
        .child(label)
}

pub(super) fn primary_button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> BaseButton {
    button_base(id, label)
        .bg(rgb(TEXT))
        .text_color(rgb(BG))
        .border_color(rgb(TEXT))
        .hover(|d| d.bg(rgb(0xffffff)).border_color(rgb(0xffffff)))
}

pub(super) fn choice_button(id: impl Into<ElementId>, selected: bool) -> BaseButton {
    BaseButton::new(id)
        .w_full()
        .h(px(CHOICE_ROW_HEIGHT))
        .flex_shrink_0()
        .min_w_0()
        .px(px(CONTROL_PADDING))
        .gap(px(UI_SPACING))
        .justify_start()
        .text_left()
        .text_size(px(12.))
        .rounded(px(CONTROL_RADIUS))
        .border_1()
        .border_color(rgba(0x00000000))
        .when(selected, selected_surface)
        .hover(move |d| d.bg(rgb(if selected { SELECTED } else { HOVER })))
        .focus_visible(|d| d.border_color(rgb(FOCUS)))
        .styles(|styles| styles.disabled(|d| d.opacity(0.5)))
        .aria_selected(selected)
}

pub(super) fn icon_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    icon: IconName,
) -> BaseButton {
    let label = label.into();
    BaseButton::new(id)
        .size(px(ICON_BUTTON_SIZE))
        .flex_shrink_0()
        .rounded(px(CONTROL_RADIUS))
        .border_1()
        .border_color(rgba(0x00000000))
        .text_color(rgb(MUTED))
        .hover(|d| d.bg(rgb(HOVER)).text_color(rgb(TEXT)))
        .focus_visible(|d| d.border_color(rgb(FOCUS)))
        .styles(|styles| styles.disabled(|d| d.opacity(0.5)))
        .child(Icon::new(icon).size(px(14.)))
        .accessibility_label(label.clone())
        .tooltip(move |window, cx| Tooltip::new(label.clone()).build(window, cx))
}

pub(super) fn button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> BaseButton {
    button_base(id, label).hover(|d| d.bg(rgb(HOVER)))
}
