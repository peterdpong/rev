use super::controls::*;
use super::popover::popover_surface;
use super::*;
use crate::theme::*;
use crate::workspace_state::TabKind;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    Disableable as _, Icon,
    command::{Command, CommandGroup, CommandItem, CommandState},
    kbd::Kbd,
};

const PALETTE_WIDTH: f32 = 560.;
const PALETTE_TOP: f32 = 72.;
const PALETTE_MAX_HEIGHT: f32 = 420.;
const PALETTE_ROW_CONTENT_HEIGHT: f32 = LIST_ROW_HEIGHT - 12.;

fn palette_item(
    label: impl Into<SharedString>,
    icon: IconName,
    action: Option<Box<dyn Action>>,
    checked: bool,
) -> CommandItem {
    let label = label.into();
    let content_label = label.clone();
    let item = CommandItem::new().label(label);
    let item = match &action {
        Some(action) => item.action(action.boxed_clone()),
        None => item,
    };
    item.child(move |window, _| {
        let shortcut = action
            .as_ref()
            .and_then(|action| Kbd::binding_for_action(action.as_ref(), Some("Rev"), window));
        row()
            .flex_1()
            .min_w_0()
            .h(px(PALETTE_ROW_CONTENT_HEIGHT))
            .gap(px(UI_SPACING))
            .text_size(px(12.))
            .line_height(px(PALETTE_ROW_CONTENT_HEIGHT))
            .child(
                Icon::new(icon)
                    .size(px(14.))
                    .flex_shrink_0()
                    .text_color(rgb(MUTED)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(content_label.clone()),
            )
            .when_some(shortcut, |d, shortcut| {
                d.child(
                    div()
                        .flex_shrink_0()
                        .text_color(rgb(MUTED))
                        .child(shortcut.appearance(false)),
                )
            })
            .when(checked, |d| {
                d.child(
                    Icon::new(IconName::Check)
                        .size(px(12.))
                        .flex_shrink_0()
                        .text_color(rgb(MUTED)),
                )
            })
    })
}

impl Rev {
    pub(super) fn toggle_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.command_palette_open {
            self.close_command_palette(window, cx);
            return;
        }
        if self.branch_popover.is_open() {
            self.close_branch_popover(window, cx);
        }
        let state = self
            .command_palette
            .get_or_insert_with(|| cx.new(|cx| CommandState::new(window, cx)))
            .clone();
        state.update(cx, |state, cx| {
            state.set_query("", window, cx);
            state.focus(window, cx);
        });
        self.command_palette_open = true;
        cx.notify();
    }

    pub(super) fn close_command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.command_palette_open {
            return;
        }
        self.command_palette_open = false;
        let focused = self
            .command_palette
            .as_ref()
            .is_some_and(|state| state.focus_handle(cx).contains_focused(window, cx));
        if focused {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    pub(super) fn command_palette(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = self
            .command_palette
            .as_ref()
            .filter(|_| self.command_palette_open)?;

        let mut tabs = Vec::new();
        let mut command = Command::new(state);
        for project in &self.workspace.projects {
            if project.tabs.is_empty() {
                continue;
            }
            let mut group = CommandGroup::new().label(project.name.clone());
            for tab in &project.tabs {
                let active = self.workspace.active_tab_id.as_ref() == Some(&tab.id);
                let icon = if tab.kind == TabKind::Current {
                    IconName::MonitorDot
                } else {
                    IconName::GitBranch
                };
                group = group.item(
                    palette_item(tab.label.clone(), icon, None, active)
                        .keywords([project.name.clone()]),
                );
                tabs.push((tab.request.clone(), tab.file.clone()));
            }
            command = command.group(group);
        }
        let tab_groups = self
            .workspace
            .projects
            .iter()
            .filter(|project| !project.tabs.is_empty())
            .map(|project| project.tabs.len())
            .collect::<Vec<_>>();
        command = command.group(self.palette_actions(cx));

        let this = cx.entity().downgrade();
        let confirm = this.clone();
        let command = command
            .placeholder("Switch tabs or run a command…")
            .max_h(px(PALETTE_MAX_HEIGHT))
            .on_confirm(move |index, window, cx| {
                let tab = tab_groups
                    .get(index.section)
                    .filter(|count| index.row < **count)
                    .map(|_| {
                        let before = tab_groups[..index.section].iter().sum::<usize>();
                        tabs[before + index.row].clone()
                    });
                _ = confirm.update(cx, |this, cx| {
                    this.close_command_palette(window, cx);
                    if let Some((request, file)) = tab {
                        this.load_review(request, file, window, cx);
                    }
                });
            })
            .on_cancel(move |window, cx| {
                let this = this.clone();
                window.defer(cx, move |window, cx| {
                    _ = this.update(cx, |this, cx| this.close_command_palette(window, cx));
                });
            });

        Some(
            div()
                .id("command-palette-backdrop")
                .absolute()
                .inset_0()
                .occlude()
                .flex()
                .justify_center()
                .items_start()
                .pt(px(PALETTE_TOP))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| this.close_command_palette(window, cx)),
                )
                .child(
                    popover_surface()
                        .id("command-palette")
                        .debug_selector(|| "command-palette".into())
                        .w(px(PALETTE_WIDTH))
                        .overflow_hidden()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(command.bordered(false)),
                )
                .into_any_element(),
        )
    }

    fn palette_actions(&self, cx: &App) -> CommandGroup {
        let reviewing = self.review.is_some();
        let diff = self.diff.read(cx);
        let item = |label: &str, icon: IconName, action: Box<dyn Action>| {
            palette_item(label.to_owned(), icon, Some(action), false)
        };
        CommandGroup::new()
            .label("Actions")
            .item(item(
                "Open folder…",
                IconName::FolderOpen,
                Box::new(OpenProject),
            ))
            .item(item("Refresh", IconName::RefreshCw, Box::new(Refresh)).disabled(!reviewing))
            .item(item(
                if self.sidebar_collapsed {
                    "Show sidebar"
                } else {
                    "Hide sidebar"
                },
                IconName::PanelLeft,
                Box::new(ToggleSidebar),
            ))
            .item(item("Next file", IconName::ArrowDown, Box::new(NextFile)).disabled(!reviewing))
            .item(
                item("Previous file", IconName::ArrowUp, Box::new(PreviousFile))
                    .disabled(!reviewing),
            )
            .item(
                item("Mark file as viewed", IconName::Check, Box::new(MarkViewed))
                    .disabled(!reviewing),
            )
            .item(
                item(
                    if diff.is_split(cx) {
                        "Switch to unified diff"
                    } else {
                        "Switch to split diff"
                    },
                    if diff.is_split(cx) {
                        IconName::Rows2
                    } else {
                        IconName::Columns2
                    },
                    Box::new(ToggleDiffMode),
                )
                .disabled(!reviewing),
            )
            .item(
                item(
                    if diff.wraps_lines() {
                        "Disable line wrapping"
                    } else {
                        "Enable line wrapping"
                    },
                    IconName::TextWrap,
                    Box::new(ToggleWrap),
                )
                .disabled(!reviewing),
            )
            .item(
                item(
                    if diff.is_single_file() {
                        "Show all files"
                    } else {
                        "Show one file at a time"
                    },
                    if diff.is_single_file() {
                        IconName::Files
                    } else {
                        IconName::File
                    },
                    Box::new(ToggleSingleFile),
                )
                .disabled(!reviewing),
            )
    }
}
