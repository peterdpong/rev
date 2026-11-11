use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Instant};

use super::{DiffFile, DiffLinePosition, DiffLineRange, DiffMode, DiffSide};
use gpui::*;
use rev_diff::{Anchor, Review, RowEdit};

pub(super) const LINE_HEIGHT: f32 = 22.;
pub(super) const GUTTER: f32 = 64.;

#[derive(Clone, Debug)]
pub enum DiffEvent {
    VisibleFileChanged(String),
    FileExpanded(String),
    SelectionStarted,
    SelectionEnded(DiffLineRange),
    SelectionChanged(Option<DiffLineRange>),
    CopyUnavailable(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct DiffScrollPosition {
    anchor: Anchor,
    offset: Pixels,
    horizontal: f32,
}

pub struct DiffState {
    pub(super) review: Review,
    pub(super) list: ListState,
    pub(super) focus: FocusHandle,
    pub(super) selection: Option<DiffLineRange>,
    pub(super) drag: Option<DiffLinePosition>,
    pub(super) geometry: Rc<RefCell<Vec<(usize, Bounds<Pixels>)>>>,
    pub(super) viewport: Bounds<Pixels>,
    pub(super) wrap: bool,
    pub(super) horizontal: f32,
    pub(super) widths: HashMap<String, f32>,
    pub(super) max_width: f32,
    pub(super) scrollbar_drag: bool,
    drag_point: Option<Point<Pixels>>,
    drag_frame_pending: bool,
    drag_time: Instant,
    visible: Option<String>,
    hinted_width: Pixels,
}

impl EventEmitter<DiffEvent> for DiffState {}

impl DiffState {
    pub fn new(files: impl IntoIterator<Item = DiffFile>, cx: &mut Context<Self>) -> Self {
        let list = ListState::new(0, ListAlignment::Top, px(150.));
        let owner = cx.weak_entity();
        list.set_scroll_handler(move |event, _, cx| {
            // This callback runs while GPUI borrows its list. Only inspect our
            // source index here; querying the ListState would reborrow it.
            let _ = owner.update(cx, |state: &mut Self, cx| {
                state.visible_row(event.visible_range.start, cx);
                cx.notify();
            });
        });
        let mut state = Self {
            review: Review::default(),
            list,
            focus: cx.focus_handle(),
            selection: None,
            drag: None,
            geometry: Default::default(),
            viewport: Bounds::default(),
            wrap: false,
            horizontal: 0.,
            widths: HashMap::new(),
            max_width: 0.,
            scrollbar_drag: false,
            visible: None,
            hinted_width: px(-1.),
            drag_point: None,
            drag_frame_pending: false,
            drag_time: Instant::now(),
        };
        state.set_files(files, cx);
        state
    }

    pub fn selected_lines(&self) -> Option<DiffLineRange> {
        self.selection.clone()
    }

    pub fn set_selected_lines(&mut self, range: Option<DiffLineRange>, cx: &mut Context<Self>) {
        self.selection = range.filter(|range| {
            range.side() == range.end_side()
                && self.review.file_index(range.path()).is_some_and(|index| {
                    let file = self.review.file(index);
                    file.line_index(range.side(), range.start()).is_some()
                        && file.line_index(range.side(), range.end()).is_some()
                })
        });
        cx.notify();
    }

    pub fn set_files(&mut self, files: impl IntoIterator<Item = DiffFile>, cx: &mut Context<Self>) {
        let files: Vec<_> = files.into_iter().collect();
        self.widths = files
            .iter()
            .map(|file| (file.path().clone(), width_hint(file)))
            .collect();
        self.max_width = self.widths.values().copied().fold(0., f32::max);
        self.review = Review::new(files, self.review.mode());
        self.selection = None;
        self.drag = None;
        self.visible = None;
        self.horizontal = 0.;
        self.geometry.borrow_mut().clear();
        self.list
            .reset_with_uniform_height(self.review.len(), px(LINE_HEIGHT));
        cx.notify();
    }

    pub fn replace_file(&mut self, file: DiffFile, cx: &mut Context<Self>) -> bool {
        let anchor = self.scroll_position();
        let path = file.path().clone();
        let width = width_hint(&file);
        let Some(edit) = self.review.replace(file) else {
            return false;
        };
        self.widths.insert(path.clone(), width);
        self.max_width = self.widths.values().copied().fold(0., f32::max);
        if self
            .selection
            .as_ref()
            .is_some_and(|selection| selection.path() == &path)
        {
            self.selection = None;
            self.drag = None;
            cx.emit(DiffEvent::SelectionChanged(None));
        }
        self.apply_edit(edit, anchor, cx);
        true
    }

    fn apply_edit(
        &mut self,
        edit: RowEdit,
        anchor: Option<DiffScrollPosition>,
        cx: &mut Context<Self>,
    ) {
        self.list.splice(edit.old, edit.new_len);
        // GPUI's public list API cannot seed only the inserted rows. Reseeding
        // retains prior heights as hints but walks the whole list and marks its
        // measurements stale. The core's file-local benchmark excludes this
        // adapter cost; a range-aware height index is a future experiment.
        self.list.clone().with_uniform_item_height(px(LINE_HEIGHT));
        self.geometry.borrow_mut().clear();
        if let Some(anchor) = anchor {
            self.restore_scroll_position(&anchor, cx);
        }
        cx.notify();
    }

    pub fn set_mode(&mut self, mode: DiffMode, cx: &mut Context<Self>) {
        let anchor = self.scroll_position();
        if !self.review.set_mode(mode) {
            return;
        }
        self.list
            .reset_with_uniform_height(self.review.len(), px(LINE_HEIGHT));
        if let Some(anchor) = anchor {
            self.restore_scroll_position(&anchor, cx);
        }
        cx.notify();
    }

    pub fn set_file_collapsed(&mut self, path: &str, collapsed: bool, cx: &mut Context<Self>) {
        let anchor = self.scroll_position();
        if let Some(edit) = self.review.collapse(path, collapsed) {
            self.apply_edit(edit, anchor, cx);
            if !collapsed {
                cx.emit(DiffEvent::FileExpanded(path.into()));
            }
        }
    }

    pub(super) fn expand_fold(&mut self, file: usize, start: usize, cx: &mut Context<Self>) {
        let anchor = self.scroll_position();
        let edit = self.review.expand_fold(file, start);
        self.apply_edit(edit, anchor, cx);
    }

    pub fn scroll_to_file(&mut self, path: &str, cx: &mut Context<Self>) {
        if let Some(file) = self.review.file_index(path) {
            let row = self.review.file_start(file);
            self.list.scroll_to(ListOffset {
                item_ix: row,
                offset_in_item: px(0.),
            });
            self.visible_row(row, cx);
            cx.notify();
        }
    }

    pub fn scroll_position(&self) -> Option<DiffScrollPosition> {
        let offset = self.list.logical_scroll_top();
        Some(DiffScrollPosition {
            anchor: self
                .review
                .anchor(offset.item_ix.min(self.review.len().saturating_sub(1)))?,
            offset: offset.offset_in_item,
            horizontal: self.horizontal,
        })
    }

    pub fn restore_scroll_position(
        &mut self,
        position: &DiffScrollPosition,
        cx: &mut Context<Self>,
    ) {
        if let Some(row) = self.review.resolve(&position.anchor) {
            self.horizontal = position.horizontal.min(self.horizontal_limit());
            self.list.scroll_to(ListOffset {
                item_ix: row,
                offset_in_item: position.offset,
            });
            self.visible_row(row, cx);
            cx.notify();
        }
    }

    fn visible_row(&mut self, row: usize, cx: &mut Context<Self>) {
        if let Some((file, _)) = self.review.row(row) {
            let path = self.review.file(file).path();
            if self.visible.as_ref() != Some(path) {
                self.visible = Some(path.clone());
                cx.emit(DiffEvent::VisibleFileChanged(path.clone()));
            }
        }
    }

    pub(super) fn measure_viewport(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        self.viewport = bounds;
        self.visible_row(self.list.logical_scroll_top().item_ix, cx);
        if self.hinted_width != bounds.size.width {
            self.hinted_width = bounds.size.width;
            self.list.clone().with_uniform_item_height(px(LINE_HEIGHT));
            self.horizontal = self.horizontal.min(self.horizontal_limit());
            cx.notify();
        }
    }

    pub(super) fn set_wrap(&mut self, wrap: bool, cx: &mut Context<Self>) {
        if self.wrap != wrap {
            self.wrap = wrap;
            self.horizontal = 0.;
            self.list.remeasure();
            self.list.clone().with_uniform_item_height(px(LINE_HEIGHT));
            cx.notify();
        }
    }

    pub(super) fn horizontal_limit(&self) -> f32 {
        if self.wrap {
            return 0.;
        }
        let lanes = if self.review.mode() == DiffMode::Split {
            2.
        } else {
            1.
        };
        let gutter = if lanes == 2. { GUTTER } else { 102. };
        (self.max_width - (f32::from(self.viewport.size.width) / lanes - gutter - 28.)).max(0.)
    }

    pub fn can_scroll_horizontally(&self, delta: f32) -> bool {
        if delta > 0. {
            self.horizontal > 0.5
        } else {
            self.horizontal < self.horizontal_limit() - 0.5
        }
    }

    pub(super) fn pan(&mut self, delta: f32, cx: &mut Context<Self>) -> bool {
        let next = (self.horizontal - delta).clamp(0., self.horizontal_limit());
        if (next - self.horizontal).abs() < 0.1 {
            return false;
        }
        self.horizontal = next;
        cx.notify();
        true
    }

    pub(super) fn begin_selection(
        &mut self,
        position: DiffLinePosition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus.focus(window, cx);
        self.selection = Some(DiffLineRange::new(
            position.path().clone(),
            position.side(),
            position.line(),
            position.line(),
        ));
        self.drag = Some(position);
        self.drag_point = None;
        self.drag_time = Instant::now();
        cx.emit(DiffEvent::SelectionStarted);
        cx.notify();
    }

    pub(super) fn drag_to(&mut self, row: usize, cx: &mut Context<Self>) {
        let Some(anchor) = &self.drag else {
            return;
        };
        let Some(target) = self.review.drag_target(anchor, row) else {
            return;
        };
        let selection = DiffLineRange::new(
            anchor.path().clone(),
            anchor.side(),
            anchor.line(),
            target.line(),
        );
        if self.selection.as_ref() != Some(&selection) {
            self.selection = Some(selection.clone());
            cx.emit(DiffEvent::SelectionChanged(Some(selection)));
            cx.notify();
        }
    }

    pub(super) fn drag_at(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.drag.is_none() {
            return;
        }
        let target = self
            .geometry
            .borrow()
            .iter()
            .min_by(|(_, a), (_, b)| {
                let distance = |bounds: &Bounds<Pixels>| {
                    let y = f32::from(point.y);
                    (y - f32::from(bounds.top())).min(0.).abs()
                        + (y - f32::from(bounds.bottom())).max(0.)
                };
                distance(a).total_cmp(&distance(b))
            })
            .map(|(row, _)| *row);
        if let Some(row) = target {
            self.drag_to(row, cx);
        }
        self.drag_point = Some(point);
        let direction = if point.y < self.viewport.top() + px(18.) {
            -1.
        } else if point.y > self.viewport.bottom() - px(18.) {
            1.
        } else {
            0.
        };
        let elapsed = self.drag_time.elapsed().as_secs_f32().min(0.05);
        self.drag_time = Instant::now();
        if direction != 0. {
            self.list.scroll_by(px(direction * 660. * elapsed));
            cx.notify();
            if !self.drag_frame_pending {
                self.drag_frame_pending = true;
                let owner = cx.weak_entity();
                // Continue while held at the edge, not just on mouse movement.
                // Time-based speed is the same on 60 Hz and 120 Hz displays.
                window.on_next_frame(move |window, cx| {
                    let _ = owner.update(cx, |state, cx| {
                        state.drag_frame_pending = false;
                        if let Some(point) = state.drag_point {
                            state.drag_at(point, window, cx);
                        }
                    });
                });
            }
        }
    }

    pub(super) fn finish_selection(&mut self, cx: &mut Context<Self>) {
        self.drag_point = None;
        if self.drag.take().is_some()
            && let Some(selection) = self.selection.clone()
        {
            cx.emit(DiffEvent::SelectionEnded(selection));
            cx.notify();
        }
    }

    pub(super) fn copy(&self, cx: &mut Context<Self>) {
        let Some(selection) = &self.selection else {
            cx.propagate();
            return;
        };
        if let Some(file) = self.review.file_index(selection.path()) {
            match self.review.file(file).copy(selection) {
                Ok(text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
                Err(error) => cx.emit(DiffEvent::CopyUnavailable(error.into())),
            }
        }
    }

    pub(super) fn select_all(&mut self, cx: &mut Context<Self>) {
        let Some(path) = &self.visible else {
            return;
        };
        let Some(file) = self.review.file_index(path) else {
            return;
        };
        let file = self.review.file(file);
        let mut side = self
            .selection
            .as_ref()
            .filter(|range| range.path() == path)
            .map_or(DiffSide::Modified, DiffLineRange::side);
        if file.source_map(side).is_empty() {
            side = DiffSide::Original;
        }
        let map = file.source_map(side);
        if let (Some(&(start, _)), Some(&(end, _))) = (map.first(), map.last()) {
            let range = DiffLineRange::new(path.clone(), side, start, end);
            self.selection = Some(range.clone());
            cx.emit(DiffEvent::SelectionEnded(range));
            cx.notify();
        }
    }

    pub(super) fn extend(&mut self, direction: isize, cx: &mut Context<Self>) {
        let Some(selection) = self.selection.clone() else {
            return;
        };
        let Some(file) = self.review.file_index(selection.path()) else {
            return;
        };
        let map = self.review.file(file).source_map(selection.side());
        let Ok(index) = map.binary_search_by_key(&selection.end(), |(line, _)| *line) else {
            return;
        };
        let index = index.saturating_add_signed(direction).min(map.len() - 1);
        let range = DiffLineRange::new(
            selection.path().clone(),
            selection.side(),
            selection.start(),
            map[index].0,
        );
        let position = DiffLinePosition::new(range.path().clone(), range.side(), range.end());
        if let Some(row) = self.review.source_row(&position) {
            self.list.scroll_to_reveal_item(row);
        }
        self.selection = Some(range.clone());
        cx.emit(DiffEvent::SelectionEnded(range));
        cx.notify();
    }
}

fn width_hint(file: &DiffFile) -> f32 {
    let columns = (0..file.lines().len())
        .map(|line| {
            let text = file.text(line);
            (text.len() + text.bytes().filter(|byte| *byte == b'\t').count() * 3).min(32_768)
        })
        .max()
        .unwrap_or(0);
    columns as f32 * 8. + 24.
}
