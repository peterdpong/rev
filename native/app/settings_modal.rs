use super::controls::*;
use super::popover::*;
use super::*;
use crate::settings::{DiffLayout, Settings};
use crate::theme::*;
use gpui_kit::{
    assets::IconName,
    base::{Button as BaseButton, Dialog},
    component::switch::Switch,
};
use std::rc::Rc;

const SETTINGS_WIDTH: f32 = 560.;
const SETTINGS_NAV_WIDTH: f32 = 150.;
const SETTINGS_ROW_HEIGHT: f32 = 44.;
const SEGMENT_HEIGHT: f32 = HEADER_PILL_HEIGHT - 4.;

impl Rev {
    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings_open = true;
        self.settings_focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.settings_open {
            return;
        }
        self.settings_open = false;
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn update_settings(&mut self, update: impl FnOnce(&mut Settings), cx: &mut Context<Self>) {
        update(&mut self.settings);
        let settings = self.settings.clone();
        self.diff.update(cx, |diff, cx| {
            diff.set_split(settings.diff_layout == DiffLayout::Split, cx);
            diff.set_wrap(settings.wrap_lines, cx);
        });
        self.save_workspace();
        cx.notify();
    }

    pub(super) fn settings_button(&self, cx: &mut Context<Self>) -> BaseButton {
        icon_button("open-settings", "Settings", IconName::Settings)
            .on_click(cx.listener(|this, _, window, cx| this.open_settings(window, cx)))
    }

    pub(super) fn settings_modal(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let motion = PopoverMotion::new("settings-motion", self.settings_open, window, cx);
        if !self.settings_open {
            return div().into_any_element();
        }
        let this = cx.entity().downgrade();
        Dialog::new(cx)
            .focus_handle(self.settings_focus.clone())
            .open(true)
            .on_open_change(move |open, _, window, cx| {
                if !open {
                    _ = this.update(cx, |this, cx| this.close_settings(window, cx));
                }
            })
            .backdrop(div().absolute().inset_0().bg(rgba(0x00000080)))
            .popup(
                motion.apply(
                    popover_surface()
                        .id("settings")
                        .debug_selector(|| "settings-modal".into())
                        .w(px(SETTINGS_WIDTH))
                        .overflow_hidden()
                        .occlude()
                        .child(
                            row()
                                .h(px(HEADER_HEIGHT))
                                .flex_shrink_0()
                                .pl(px(POPOVER_PADDING))
                                .pr(px(UI_SPACING))
                                .border_b_1()
                                .border_color(rgb(BORDER))
                                .child(
                                    div()
                                        .flex_1()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("Settings"),
                                )
                                .child(
                                    icon_button("close-settings", "Close", IconName::Close)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.close_settings(window, cx)
                                        })),
                                ),
                        )
                        .child(
                            row()
                                .items_start()
                                .gap_0()
                                .child(
                                    column()
                                        .w(px(SETTINGS_NAV_WIDTH))
                                        .flex_shrink_0()
                                        .self_stretch()
                                        .p(px(UI_SPACING))
                                        .gap(px(2.))
                                        .bg(rgb(PANEL))
                                        .border_r_1()
                                        .border_color(rgb(BORDER))
                                        .child(
                                            choice_button("settings-general", true)
                                                .h(px(LIST_ROW_HEIGHT))
                                                .child("General"),
                                        ),
                                )
                                .child(self.general_settings(cx)),
                        ),
                ),
            )
            .into_any_element()
    }

    fn general_settings(&self, cx: &mut Context<Self>) -> Div {
        let settings = &self.settings;
        column()
            .flex_1()
            .min_w_0()
            .px(px(POPOVER_PADDING))
            .py(px(UI_SPACING))
            .text_size(px(12.))
            .child(setting_row(
                "Diff layout",
                segmented(
                    "diff-layout",
                    settings.diff_layout,
                    [
                        (DiffLayout::Unified, "Unified"),
                        (DiffLayout::Split, "Split"),
                    ],
                    {
                        let this = cx.entity().downgrade();
                        move |layout, _, cx| {
                            _ = this.update(cx, |this, cx| {
                                this.update_settings(|settings| settings.diff_layout = layout, cx)
                            });
                        }
                    },
                ),
            ))
            .child(
                setting_row(
                    "Wrap long lines",
                    Switch::new("wrap-lines")
                        .checked(settings.wrap_lines)
                        .accessibility_label("Wrap long lines")
                        .on_change(cx.listener(|this, wrap: &bool, _, cx| {
                            let wrap = *wrap;
                            this.update_settings(|settings| settings.wrap_lines = wrap, cx)
                        })),
                )
                .border_b_0(),
            )
    }
}

fn setting_row(label: &'static str, control: impl IntoElement) -> Div {
    row()
        .h(px(SETTINGS_ROW_HEIGHT))
        .justify_between()
        .border_b_1()
        .border_color(rgb(BORDER))
        .child(label)
        .child(control)
}

fn segmented<T: Copy + PartialEq + 'static, const N: usize>(
    id: &'static str,
    value: T,
    options: [(T, &'static str); N],
    on_select: impl Fn(T, &mut Window, &mut App) + 'static,
) -> Div {
    let on_select = Rc::new(on_select);
    row()
        .debug_selector(move || id.into())
        .p(px(1.))
        .gap(px(2.))
        .rounded(px(CONTROL_RADIUS))
        .border_1()
        .border_color(rgb(CONTROL_BORDER))
        .bg(rgb(PANEL))
        .children(
            options
                .into_iter()
                .enumerate()
                .map(|(index, (option, label))| {
                    let selected = option == value;
                    let on_select = on_select.clone();
                    BaseButton::new((id, index))
                        .h(px(SEGMENT_HEIGHT))
                        .px(px(10.))
                        .rounded(px(CONTROL_RADIUS - 2.))
                        .border_1()
                        .border_color(rgba(0x00000000))
                        .text_size(px(12.))
                        .text_color(rgb(MUTED))
                        .when(selected, selected_surface)
                        .when(!selected, |d| {
                            d.hover(|d| d.bg(rgb(HOVER)).text_color(rgb(TEXT)))
                        })
                        .focus_visible(|d| d.border_color(rgb(FOCUS)))
                        .aria_selected(selected)
                        .accessibility_label(label)
                        .child(label)
                        .on_click(move |_, window, cx| on_select(option, window, cx))
                }),
        )
}
