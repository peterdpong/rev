use std::rc::Rc;

use gpui::{prelude::*, *};
use rev_diff::{LineKind, Row, TokenKind};

use super::{
    state::{GUTTER, LINE_HEIGHT},
    *,
};

const BG: u32 = 0x17181b;
const BORDER: u32 = 0x2c2e33;
const TEXT: u32 = 0xd7dbe4;
const MUTED: u32 = 0x77808e;
const SELECTED: u32 = 0x2a405e;

type Header = Rc<dyn Fn(&DiffFile, &mut Window, &mut App) -> AnyElement>;
type Add = Rc<dyn Fn(&DiffLineRange, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Diff {
    state: Entity<DiffState>,
    style: StyleRefinement,
    wrap: bool,
    header: Option<Header>,
    add: Option<Add>,
}

impl Diff {
    pub fn new(state: &Entity<DiffState>) -> Self {
        Self {
            state: state.clone(),
            style: StyleRefinement::default(),
            wrap: false,
            header: None,
            add: None,
        }
    }
    pub fn soft_wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }
    pub fn render_header(
        mut self,
        header: impl Fn(&DiffFile, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.header = Some(Rc::new(header));
        self
    }
    pub fn on_add_annotation(
        mut self,
        add: impl Fn(&DiffLineRange, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.add = Some(Rc::new(add));
        self
    }
}

impl Styled for Diff {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Diff {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        self.state
            .clone()
            .update(cx, |state, cx| render(self, state, window, cx))
    }
}

fn render(
    props: Diff,
    state: &mut DiffState,
    _window: &mut Window,
    cx: &mut Context<DiffState>,
) -> AnyElement {
    state.set_wrap(props.wrap, cx);
    state.geometry.borrow_mut().clear();
    let owner = cx.entity();
    let list_owner = owner.clone();
    let header = props.header;
    let add = props.add;
    let list = list(state.list.clone(), move |index, window, cx| {
        render_row(
            &list_owner,
            index,
            header.as_ref(),
            add.as_ref(),
            window,
            cx,
        )
    })
    .size_full();

    let height = f32::from(state.viewport.size.height).max(1.);
    let max = f32::from(state.list.max_offset_for_scrollbar().y).max(0.);
    let thumb = (height * height / (max + height)).clamp(28_f32.min(height), height);
    let offset = -f32::from(state.list.scroll_px_offset_for_scrollbar().y);
    let thumb_top = if max > 0. {
        (height - thumb) * (offset / max).clamp(0., 1.)
    } else {
        0.
    };

    let mut root = div();
    *root.style() = props.style;
    root.id("rev-independent-diff")
        .relative()
        .size_full()
        .overflow_hidden()
        .bg(rgb(BG))
        .text_color(rgb(TEXT))
        .track_focus(&state.focus)
        .key_context("RevDiffPrototype")
        .child(list)
        .child(
            canvas(
                move |bounds, _, cx| {
                    owner.update(cx, |state, cx| state.measure_viewport(bounds, cx));
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
        .when(max > 0., |root| {
            root.child(
                div()
                    .id("review-scroll-track")
                    .absolute()
                    .right_0()
                    .top_0()
                    .bottom_0()
                    .w(px(10.))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|state, event: &MouseDownEvent, _, cx| {
                            state.scrollbar_drag = true;
                            state.list.scrollbar_drag_started();
                            scrub(state, event.position.y);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .absolute()
                            .right(px(2.))
                            .top(px(thumb_top))
                            .w(px(5.))
                            .h(px(thumb))
                            .rounded(px(3.))
                            .bg(rgb(0x555c69)),
                    ),
            )
        })
        .on_mouse_move(cx.listener(|state, event: &MouseMoveEvent, window, cx| {
            if event.pressed_button == Some(MouseButton::Left) {
                if state.scrollbar_drag {
                    scrub(state, event.position.y);
                    cx.notify();
                } else {
                    state.drag_at(event.position, window, cx);
                }
            }
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|state, _, _, cx| release(state, cx)),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(|state, _, _, cx| release(state, cx)),
        )
        .on_action(cx.listener(|state, _: &Copy, _, cx| state.copy(cx)))
        .on_action(cx.listener(|state, _: &SelectAll, _, cx| state.select_all(cx)))
        .on_action(cx.listener(|state, _: &ExtendUp, _, cx| state.extend(-1, cx)))
        .on_action(cx.listener(|state, _: &ExtendDown, _, cx| state.extend(1, cx)))
        .on_action(cx.listener(|state, _: &Up, _, cx| {
            state.list.scroll_by(px(-LINE_HEIGHT));
            cx.notify();
        }))
        .on_action(cx.listener(|state, _: &Down, _, cx| {
            state.list.scroll_by(px(LINE_HEIGHT));
            cx.notify();
        }))
        .on_action(cx.listener(|state, _: &PageUp, _, cx| {
            state.list.scroll_by(-state.viewport.size.height * 0.9);
            cx.notify();
        }))
        .on_action(cx.listener(|state, _: &PageDown, _, cx| {
            state.list.scroll_by(state.viewport.size.height * 0.9);
            cx.notify();
        }))
        .on_action(cx.listener(|state, _: &First, _, cx| {
            state.list.scroll_to(ListOffset::default());
            cx.notify();
        }))
        .on_action(cx.listener(|state, _: &Last, _, cx| {
            state.list.scroll_to_end();
            cx.notify();
        }))
        .into_any_element()
}

fn release(state: &mut DiffState, cx: &mut Context<DiffState>) {
    if state.scrollbar_drag {
        state.scrollbar_drag = false;
        state.list.scrollbar_drag_ended();
    }
    state.finish_selection(cx);
}

fn scrub(state: &DiffState, y: Pixels) {
    let height = f32::from(state.viewport.size.height).max(1.);
    let fraction = (f32::from(y - state.viewport.top()) / height).clamp(0., 1.);
    let max = state.list.max_offset_for_scrollbar().y;
    state
        .list
        .set_offset_from_scrollbar(point(px(0.), -max * fraction));
}

fn render_row(
    owner: &Entity<DiffState>,
    index: usize,
    header: Option<&Header>,
    add: Option<&Add>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let state = owner.read(cx);
    let Some((file_index, row)) = state.review.row(index) else {
        return div().into_any_element();
    };
    let file = state.review.file(file_index).clone();
    let row = row.clone();
    let geometry = state.geometry.clone();
    let collapsed = state.review.is_collapsed(file.path());
    let split = state.review.mode() == DiffMode::Split;
    let content = match row {
        Row::Header => {
            let state = owner.clone();
            let path = file.path().clone();
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(8.))
                .h(px(34.))
                .bg(rgb(0x1d1f24))
                .border_t_1()
                .border_b_1()
                .border_color(rgb(BORDER))
                .child(
                    div()
                        .id(("collapse", file_index))
                        .w(px(18.))
                        .cursor_pointer()
                        .text_color(rgb(MUTED))
                        .child(if collapsed { "▸" } else { "▾" })
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            state.update(cx, |state, cx| {
                                state.set_file_collapsed(&path, !collapsed, cx)
                            });
                            cx.stop_propagation();
                        }),
                )
                .child(div().flex_1().min_w_0().child(match header {
                    Some(header) => header(&file, window, cx),
                    None => div().child(file.path().clone()).into_any_element(),
                }))
                .into_any_element()
        }
        Row::Hunk(hunk) => div()
            .h(px(24.))
            .px(px(12.))
            .bg(rgb(0x1c2634))
            .text_color(rgb(0x849dbb))
            .text_size(px(11.))
            .line_height(px(24.))
            .overflow_hidden()
            .whitespace_nowrap()
            .child(file.hunks()[hunk].header.clone())
            .into_any_element(),
        Row::Notice => div()
            .px(px(16.))
            .py(px(10.))
            .text_color(rgb(MUTED))
            .child(file.message().unwrap_or("No textual changes").to_string())
            .into_any_element(),
        Row::Fold(range) => {
            let state = owner.clone();
            div()
                .id(("fold", index))
                .h(px(24.))
                .px(px(16.))
                .line_height(px(24.))
                .text_color(rgb(0x91adcf))
                .text_size(px(11.))
                .cursor_pointer()
                .hover(|d| d.bg(rgb(0x242b36)))
                .child(format!("···  {} unchanged lines", range.len()))
                .on_click(move |_, _, cx| {
                    state.update(cx, |state, cx| {
                        state.expand_fold(file_index, range.start, cx)
                    });
                })
                .into_any_element()
        }
        Row::Code { left, right } if split => div()
            .flex()
            .w_full()
            .min_h(px(LINE_HEIGHT))
            .items_stretch()
            .child(cell(
                owner,
                &file,
                index,
                left,
                DiffSide::Original,
                false,
                add,
                cx,
            ))
            .child(cell(
                owner,
                &file,
                index,
                right,
                DiffSide::Modified,
                false,
                add,
                cx,
            ))
            .into_any_element(),
        Row::Code { left, right } => {
            let side = if right.is_some() {
                DiffSide::Modified
            } else {
                DiffSide::Original
            };
            cell(owner, &file, index, right.or(left), side, true, add, cx)
        }
    };
    let state = owner.clone();
    div()
        .id(("review-row", index))
        .relative()
        .w_full()
        .child(content)
        .on_scroll_wheel(move |event, _, cx| {
            let delta = event.delta.pixel_delta(px(LINE_HEIGHT));
            if delta.x.abs() > delta.y.abs() {
                let moved = state.update(cx, |state, cx| state.pan(f32::from(delta.x), cx));
                if moved {
                    cx.stop_propagation();
                }
            }
        })
        .child(
            canvas(
                move |bounds, _, _| {
                    let mut geometry = geometry.borrow_mut();
                    if let Some(existing) = geometry.iter_mut().find(|(row, _)| *row == index) {
                        existing.1 = bounds;
                    } else {
                        geometry.push((index, bounds));
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
        .into_any_element()
}

fn cell(
    owner: &Entity<DiffState>,
    file: &DiffFile,
    row: usize,
    line: Option<usize>,
    side: DiffSide,
    unified: bool,
    add: Option<&Add>,
    cx: &App,
) -> AnyElement {
    let state = owner.read(cx);
    let Some(index) = line else {
        return div()
            .flex_1()
            .min_w_0()
            .bg(rgb(0x141518))
            .border_l_1()
            .border_color(rgb(BORDER))
            .into_any_element();
    };
    let source = &file.lines()[index];
    let number = source.number(side).unwrap();
    let selected = state.selection.as_ref().is_some_and(|selection| {
        selection.contains(file.path(), side, number)
            || unified
                && source.kind == LineKind::Context
                && source
                    .number(selection.side())
                    .is_some_and(|number| selection.contains(file.path(), selection.side(), number))
    });
    let background = if selected {
        SELECTED
    } else {
        match source.kind {
            LineKind::Context => BG,
            LineKind::Added => 0x192b23,
            LineKind::Removed => 0x302025,
        }
    };
    let mut display = String::new();
    for ch in file.text(index).chars() {
        if display.len() >= 32_768 {
            display.push_str(" … [line truncated for display]");
            break;
        }
        if ch == '\t' {
            display.push_str("    ");
        } else if ch == '\r' {
            display.push('␍');
        } else if ch.is_control() {
            display.push('�');
        } else {
            display.push(ch);
        }
    }
    let highlights = rev_diff::tokens(&display)
        .into_iter()
        .map(|token| {
            let color = match token.kind {
                TokenKind::Comment => 0x758476,
                TokenKind::String => 0xb4cc95,
                TokenKind::Number => 0xd7b487,
                TokenKind::Keyword => 0xb49bd1,
            };
            (
                token.bytes,
                HighlightStyle {
                    color: Some(rgb(color).into()),
                    ..Default::default()
                },
            )
        })
        .collect::<Vec<_>>();
    let code = StyledText::new(display).with_highlights(highlights);
    let wrap = state.wrap;
    let horizontal = state.horizontal;
    let width = state.max_width;
    let mut code_box = div().flex_1().min_w_0().overflow_hidden().px(px(8.));
    if wrap {
        code_box = code_box.whitespace_normal().child(code);
    } else {
        code_box = code_box.h(px(LINE_HEIGHT)).whitespace_nowrap().child(
            div()
                .relative()
                .left(px(-horizontal))
                .w(px(width.max(1.)))
                .child(code),
        );
    }
    let position = DiffLinePosition::new(file.path().clone(), side, number);
    let pressed = owner.clone();
    let moved = owner.clone();
    let plus_owner = owner.clone();
    let plus = add.cloned();
    let gutter = if unified {
        div()
            .flex()
            .justify_end()
            .child(line_number(
                owner,
                file.path(),
                DiffSide::Original,
                source.old,
                row,
            ))
            .child(line_number(
                owner,
                file.path(),
                DiffSide::Modified,
                source.new,
                row,
            ))
    } else {
        div().child(format!("{number:>6}"))
    };
    div()
        .id((
            if side == DiffSide::Original {
                "old"
            } else {
                "new"
            },
            row,
        ))
        .group("review-source")
        .flex()
        .flex_1()
        .min_w_0()
        .min_h(px(LINE_HEIGHT))
        .font_family("Menlo")
        .text_size(px(12.))
        .line_height(px(LINE_HEIGHT))
        .bg(rgb(background))
        .when(!unified && side == DiffSide::Modified, |d| {
            d.border_l_1().border_color(rgb(BORDER))
        })
        .cursor_crosshair()
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            pressed.update(cx, |state, cx| {
                state.begin_selection(position.clone(), window, cx)
            });
            cx.stop_propagation();
        })
        .on_mouse_move(move |event, _, cx| {
            if event.pressed_button == Some(MouseButton::Left) {
                moved.update(cx, |state, cx| state.drag_to(row, cx));
            }
        })
        .child(
            div()
                .relative()
                .flex_shrink_0()
                .w(px(if unified { 102. } else { GUTTER }))
                .text_color(rgb(MUTED))
                .text_right()
                .pr(px(8.))
                .child(gutter)
                .when(plus.is_some(), |gutter| {
                    gutter.child(
                        div()
                            .id(("select-range", row))
                            .absolute()
                            .left(px(2.))
                            .top(px(2.))
                            .size(px(18.))
                            .line_height(px(18.))
                            .text_center()
                            .rounded(px(3.))
                            .bg(rgb(0x315d91))
                            .text_color(rgb(0xe8f1ff))
                            .opacity(0.)
                            .group_hover("review-source", |d| d.opacity(1.))
                            .child("+")
                            .on_click(move |_, window, cx| {
                                if let Some(range) = plus_owner.read(cx).selected_lines()
                                    && let Some(add) = &plus
                                {
                                    add(&range, window, cx);
                                }
                            }),
                    )
                }),
        )
        .child(
            div()
                .flex_shrink_0()
                .w(px(12.))
                .text_color(rgb(match source.kind {
                    LineKind::Added => 0x8fd3a5,
                    LineKind::Removed => 0xe29595,
                    LineKind::Context => MUTED,
                }))
                .child(match source.kind {
                    LineKind::Added => "+",
                    LineKind::Removed => "−",
                    LineKind::Context => " ",
                }),
        )
        .child(code_box)
        .into_any_element()
}

/// A context row has two valid source coordinates. The old gutter must select
/// old lines even when its shared text cell defaults to the modified side.
fn line_number(
    owner: &Entity<DiffState>,
    path: &str,
    side: DiffSide,
    number: Option<usize>,
    row: usize,
) -> AnyElement {
    let owner = owner.clone();
    let path = path.to_string();
    div()
        .id((
            if side == DiffSide::Original {
                "old-number"
            } else {
                "new-number"
            },
            row,
        ))
        .w(px(38.))
        .flex_shrink_0()
        .text_right()
        .child(number.map(|n| n.to_string()).unwrap_or_default())
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            if let Some(number) = number {
                owner.update(cx, |state, cx| {
                    state.begin_selection(
                        DiffLinePosition::new(path.clone(), side, number),
                        window,
                        cx,
                    )
                });
                cx.stop_propagation();
            }
        })
        .into_any_element()
}
