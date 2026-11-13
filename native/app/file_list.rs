use super::controls::*;
use super::file_tree::{self, EntryKind, FileTreeCache, TreeEntry};
use crate::{model::File, theme::*};
use gpui_kit::prelude::*;
use gpui_kit::*;
use gpui_kit::{
    assets::IconName,
    base::Button as BaseButton,
    component::{
        Icon, Sizable,
        input::{Input, InputState},
    },
};
use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

pub(super) enum FileListEvent {
    Selected(String),
}

pub(super) struct FileList {
    repo: String,
    filter: Entity<InputState>,
    filter_text: String,
    tree: FileTreeCache,
    collapsed_folders: HashMap<String, HashSet<String>>,
    filtered_folders: HashSet<String>,
    scroll: UniformListScrollHandle,
    selected: Option<String>,
    viewed: HashMap<String, String>,
    loading: bool,
    _filter_subscription: Subscription,
    #[cfg(test)]
    rows_rendered: std::cell::RefCell<Vec<String>>,
}

impl EventEmitter<FileListEvent> for FileList {}

impl FileList {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Find a file…"));
        let subscription = cx.observe(&filter, |this, filter, cx| {
            let value = filter.read(cx).value().to_string();
            if value != this.filter_text {
                this.filter_text = value;
                this.filtered_folders.clear();
                this.tree.set_filter(&this.filter_text);
                this.tree.invalidate_rows();
                this.scroll.scroll_to_item(0, ScrollStrategy::Top);
                cx.notify();
            }
        });

        Self {
            repo: String::new(),
            filter,
            filter_text: String::new(),
            tree: FileTreeCache::default(),
            collapsed_folders: HashMap::new(),
            filtered_folders: HashSet::new(),
            scroll: UniformListScrollHandle::default(),
            selected: None,
            viewed: HashMap::new(),
            loading: false,
            _filter_subscription: subscription,
            #[cfg(test)]
            rows_rendered: Default::default(),
        }
    }

    pub(super) fn set_review(
        &mut self,
        repo: &str,
        files: &[File],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.repo = repo.to_owned();
        self.filter_text.clear();
        self.filtered_folders.clear();
        self.tree.set_filter("");
        self.tree.set_files(files);
        self.tree.invalidate_rows();
        self.scroll = UniformListScrollHandle::default();
        self.selected = None;
        self.viewed.clear();

        self.filter
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    pub(super) fn update_files(&mut self, files: &[File], cx: &mut Context<Self>) {
        if self.tree.set_files(files) {
            cx.notify();
        }
    }

    pub(super) fn sync_status(
        &mut self,
        selected: Option<&str>,
        viewed: &HashMap<String, String>,
        loading: bool,
        cx: &mut Context<Self>,
    ) {
        if self.selected.as_deref() != selected || &self.viewed != viewed || self.loading != loading
        {
            self.selected = selected.map(str::to_owned);
            self.viewed.clone_from(viewed);
            self.loading = loading;
            cx.notify();
        }
    }

    pub(super) fn counts(&self) -> (usize, usize) {
        (self.tree.added, self.tree.removed)
    }

    pub(super) fn file_counts(&self, path: &str) -> Option<(usize, usize)> {
        self.tree.file(path).map(|file| (file.added, file.removed))
    }

    pub(super) fn forget_repository(&mut self, repo: &str) {
        self.collapsed_folders.remove(repo);
    }

    pub(super) fn select_file(&mut self, path: &str, cx: &mut Context<Self>) {
        let mut revealed = false;
        if let Some(collapsed) = self.collapsed_folders.get_mut(&self.repo) {
            revealed |= file_tree::reveal_file(collapsed, path);
        }
        revealed |= file_tree::reveal_file(&mut self.filtered_folders, path);
        if revealed {
            self.tree.invalidate_rows();
        }

        if let Some(index) = self.entries().iter().position(|entry| entry.path == path) {
            self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        }

        self.selected = Some(path.to_owned());
        cx.notify();
    }

    pub(super) fn adjacent_file(
        &mut self,
        current: Option<&str>,
        direction: isize,
    ) -> Option<String> {
        let entries = self.entries();
        let paths: Vec<_> = entries
            .iter()
            .filter_map(|entry| match &entry.kind {
                EntryKind::File(file) => Some(file.path.as_str()),
                EntryKind::Folder { .. } => None,
            })
            .collect();
        if paths.is_empty() {
            return None;
        }

        let next = match paths.iter().position(|path| Some(*path) == current) {
            Some(index) => (index as isize + direction).clamp(0, paths.len() as isize - 1) as usize,
            None if direction < 0 => paths.len() - 1,
            None => 0,
        };
        Some(paths[next].to_owned())
    }

    fn entries(&mut self) -> Rc<Vec<TreeEntry>> {
        let collapsed = if self.filter_text.is_empty() {
            self.collapsed_folders.get(&self.repo)
        } else {
            Some(&self.filtered_folders)
        };

        self.tree.entries(collapsed)
    }

    fn toggle_folder(&mut self, path: &str, cx: &mut Context<Self>) {
        let collapsed = if self.filter_text.is_empty() {
            self.collapsed_folders.entry(self.repo.clone()).or_default()
        } else {
            &mut self.filtered_folders
        };
        if !collapsed.remove(path) {
            collapsed.insert(path.to_owned());
        }

        self.tree.invalidate_rows();
        cx.notify();
    }

    fn file_tree_row(&self, entry: &TreeEntry, cx: &mut Context<Self>) -> AnyElement {
        #[cfg(test)]
        self.rows_rendered.borrow_mut().push(entry.path.clone());

        let repo = &self.repo;
        let loading = self.loading;
        let path = entry.path.clone();
        let selected =
            matches!(&entry.kind, EntryKind::File(_)) && self.selected.as_ref() == Some(&path);
        let viewed = self.viewed.contains_key(&path);

        let label = div()
            .flex_1()
            .min_w_0()
            .truncate()
            .child(entry.label.clone());

        let item = BaseButton::new(SharedString::from(format!("file-tree-{repo}-{path}")))
            .w_full()
            .min_w_0()
            .h(px(LIST_ROW_HEIGHT))
            .flex_shrink_0()
            .justify_start()
            .gap(px(6.))
            .pl(px(8. + entry.depth as f32 * 12.))
            .pr_2()
            .rounded(px(CONTROL_RADIUS))
            .border_1()
            .border_color(rgba(0x00000000))
            .text_size(px(12.))
            .when(selected, selected_surface)
            .hover(move |d| {
                if selected || loading {
                    d
                } else {
                    d.bg(rgb(HOVER))
                }
            })
            .focus_visible(|d| d.border_color(rgb(FOCUS)))
            .styles(|styles| styles.disabled(|d| d.opacity(0.5)))
            .disabled(loading)
            .accessibility_label(path.clone());

        let item = match &entry.kind {
            EntryKind::Folder { expanded } => item
                .aria_expanded(*expanded)
                .child(
                    Icon::new(if *expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .size(px(12.))
                    .flex_shrink_0()
                    .text_color(rgb(MUTED)),
                )
                .child(FolderIcon {
                    id: format!("file-folder-{repo}-{path}").into(),
                    expanded: *expanded,
                })
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| this.toggle_folder(&path, cx))),
            EntryKind::File(file) => item
                .aria_selected(selected)
                .child(div().size(px(12.)).flex_shrink_0())
                .child(file_status_icon(
                    SharedString::from(format!("file-status-{repo}-{path}")),
                    &file.status,
                ))
                .child(label)
                .when(viewed, |item| {
                    item.child(
                        Icon::new(IconName::Check)
                            .size(px(14.))
                            .flex_shrink_0()
                            .text_color(rgb(SUCCESS)),
                    )
                })
                .on_click(cx.listener(move |_, _, _, cx| {
                    cx.emit(FileListEvent::Selected(path.clone()));
                })),
        };

        // Include the gap in the fixed row height so viewport calculations stay exact.
        div()
            .w_full()
            .h(px(LIST_ROW_HEIGHT + 2.))
            .pb(px(2.))
            .child(item)
            .into_any_element()
    }
}

impl Render for FileList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entries = self.entries();
        let empty = entries.is_empty();
        let count = entries.len();

        let body = div()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .p(px(UI_SPACING))
            .when(empty, |d| {
                d.child(div().p_2().text_size(px(12.)).text_color(rgb(MUTED)).child(
                    if self.filter_text.is_empty() {
                        "No changed files"
                    } else {
                        "No matching files"
                    },
                ))
            })
            .when(!empty, |d| {
                d.child(
                    uniform_list(
                        "files-scroll",
                        count,
                        cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                            range
                                .map(|index| this.file_tree_row(&entries[index], cx))
                                .collect::<Vec<_>>()
                        }),
                    )
                    .size_full()
                    .track_scroll(&self.scroll),
                )
            });

        column()
            .w(px(210.))
            .bg(rgb(SUBPANEL))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(rgb(BORDER))
            .child(
                file_header_row().child(
                    Input::new(&self.filter)
                        .id("filter-files")
                        .focus_bordered(false)
                        .small()
                        .min_h(px(CONTROL_HEIGHT))
                        .max_h(px(CONTROL_HEIGHT))
                        .px(px(CONTROL_PADDING))
                        .py_0()
                        .gap(px(6.))
                        .rounded(px(CONTROL_RADIUS))
                        .bg(rgb(SURFACE))
                        .border_1()
                        .border_color(rgb(BORDER))
                        .text_size(px(12.))
                        .line_height(px(HEADER_PILL_LINE_HEIGHT))
                        .prefix(
                            Icon::new(IconName::Search)
                                .size(px(14.))
                                .text_color(rgb(MUTED)),
                        )
                        .cleanable(true)
                        .aria_label("Find a file"),
                ),
            )
            .child(body)
            .into_any_element()
    }
}

fn file_status_icon(id: impl Into<ElementId>, status: &str) -> Stateful<Div> {
    let (icon, label, color) = match status {
        "A" => (IconName::FilePlus, "Added", SUCCESS),
        "M" => (IconName::FilePenLine, "Modified", WARNING),
        "D" => (IconName::FileMinus, "Deleted", DANGER),
        "R" => (IconName::FileSymlink, "Renamed", BLUE),
        "C" => (IconName::Copy, "Copied", BLUE),
        "T" => (IconName::FileType, "File type changed", PURPLE),
        "U" => (IconName::FileExclamationPoint, "Unmerged", DANGER),
        "?" => (IconName::FileQuestionMark, "Untracked", BLUE),
        _ => (IconName::FileQuestionMark, "Unknown status", MUTED),
    };

    div()
        .id(id)
        .size(px(14.))
        .flex_shrink_0()
        .role(accesskit::Role::Image)
        .aria_label(label)
        .child(Icon::new(icon).size(px(14.)).text_color(rgb(color)))
}

#[cfg(test)]
mod tests {
    use super::{FileList, FileListEvent};
    use crate::model::File;
    use gpui_kit::{
        AppContext, Context, Entity, IntoElement, Render, ScrollStrategy, TestAppContext,
        VisualTestContext, Window, div, prelude::*, px,
    };
    use std::{cell::RefCell, rc::Rc};

    struct Host {
        list: Entity<FileList>,
    }

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().w(px(210.)).h(px(300.)).child(self.list.clone())
        }
    }

    fn fixture(window: &mut Window, cx: &mut Context<FileList>) -> FileList {
        let files: Vec<_> = (0..20_000)
            .map(|index| File {
                path: format!("file{index:05}.rs"),
                status: "M".into(),
                added: 20,
                removed: 0,
            })
            .collect();

        let mut list = FileList::new(window, cx);
        list.set_review("/repo", &files, window, cx);
        list
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            _ = window.draw(cx);
        });
    }

    #[gpui::test]
    fn twenty_thousand_files_render_only_the_viewport_and_scroll_to_the_last_file(
        cx: &mut TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::diff::init(cx);
            crate::theme::init(cx);
        });

        let (host, cx) = cx.add_window_view(|window, cx| Host {
            list: cx.new(|cx| fixture(window, cx)),
        });
        let view = cx.update(|_, cx| host.read(cx).list.clone());

        draw(cx);
        cx.update(|_, cx| {
            let app = view.read(cx);
            let rendered = app.rows_rendered.take();
            assert!(
                !rendered.is_empty() && rendered.len() < 100,
                "rendered {} rows",
                rendered.len()
            );
            assert_eq!(
                app.scroll
                    .0
                    .borrow()
                    .last_item_size
                    .unwrap()
                    .contents
                    .height,
                px(28. * 20_000.)
            );
        });

        view.update(cx, |app, cx| {
            app.scroll.scroll_to_item(19_999, ScrollStrategy::Nearest);
            cx.notify();
        });
        draw(cx);
        cx.update(|_, cx| {
            let app = view.read(cx);
            let rendered = app.rows_rendered.take();
            assert!(
                !rendered.is_empty() && rendered.len() < 100,
                "rendered {} rows after scroll",
                rendered.len()
            );
            assert!(
                rendered.iter().any(|path| path == "file19999.rs"),
                "rendered {rendered:?}"
            );
        });

        cx.update(|window, cx| {
            view.read(cx)
                .filter
                .clone()
                .update(cx, |input, cx| input.set_value("file19999", window, cx));
        });
        draw(cx);
        view.update(cx, |app, _| {
            assert_eq!(app.entries().len(), 1);
            assert!(app.rows_rendered.borrow().len() < 10);
            assert_eq!(app.scroll.0.borrow().base_handle.offset().y, px(0.));
        });
    }

    #[gpui::test]
    fn file_clicks_emit_selection_and_loading_disables_them(cx: &mut TestAppContext) {
        use gpui_kit::test::TestQueryExt;

        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::theme::init(cx);
        });

        let (host, cx) = cx.add_window_view(|window, cx| Host {
            list: cx.new(|cx| fixture(window, cx)),
        });
        let list = cx.update(|_, cx| host.read(cx).list.clone());

        let selected = Rc::new(RefCell::new(Vec::new()));
        let _subscription = cx.update(|_, cx| {
            let selected = selected.clone();
            cx.subscribe(&list, move |_, event, _| {
                let FileListEvent::Selected(path) = event;
                selected.borrow_mut().push(path.clone());
            })
        });

        draw(cx);
        let bounds = cx.update(|window, _| window.find_by_label("file00000.rs").bounds());
        cx.simulate_click(bounds.center(), Default::default());
        draw(cx);
        assert_eq!(*selected.borrow(), vec!["file00000.rs"]);

        list.update(cx, |list, cx| {
            list.sync_status(None, &Default::default(), true, cx)
        });
        draw(cx);
        cx.simulate_click(bounds.center(), Default::default());
        draw(cx);
        assert_eq!(selected.borrow().len(), 1);
    }

    #[gpui::test]
    fn filtering_preserves_repository_folders_and_selection_reveals_files(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        let cx = cx.add_empty_window();
        let list = cx.update(|window, cx| {
            cx.new(|cx| {
                let mut list = FileList::new(window, cx);
                let files = ["src/a.rs", "src/b.rs", "test.rs"].map(|path| File {
                    path: path.into(),
                    status: "M".into(),
                    added: 1,
                    removed: 0,
                });
                list.set_review("/repo", &files, window, cx);
                list
            })
        });

        list.update(cx, |list, cx| {
            list.toggle_folder("src", cx);
            assert_eq!(list.adjacent_file(None, 1).as_deref(), Some("test.rs"));
        });

        cx.update(|window, cx| {
            list.read(cx)
                .filter
                .clone()
                .update(cx, |input, cx| input.set_value("src", window, cx))
        });
        cx.run_until_parked();
        list.update(cx, |list, cx| {
            assert_eq!(list.adjacent_file(None, 1).as_deref(), Some("src/a.rs"));
            list.toggle_folder("src", cx);
            assert!(list.adjacent_file(None, 1).is_none());
        });

        cx.update(|window, cx| {
            list.read(cx)
                .filter
                .clone()
                .update(cx, |input, cx| input.set_value("", window, cx))
        });
        cx.run_until_parked();
        list.update(cx, |list, cx| {
            assert_eq!(list.adjacent_file(None, 1).as_deref(), Some("test.rs"));
            list.select_file("src/b.rs", cx);
            assert_eq!(
                list.adjacent_file(Some("src/b.rs"), -1).as_deref(),
                Some("src/a.rs")
            );
        });

        cx.update(|window, cx| {
            list.update(cx, |list, cx| {
                list.toggle_folder("src", cx);
                list.set_review("/other", &[], window, cx);
                assert!(list.filter.read(cx).value().is_empty());
                assert!(list.adjacent_file(None, 1).is_none());
                let files = [File {
                    path: "src/a.rs".into(),
                    status: "M".into(),
                    added: 1,
                    removed: 0,
                }];
                list.set_review("/repo", &files, window, cx);
                assert!(list.adjacent_file(None, 1).is_none());
            })
        });
    }
}
