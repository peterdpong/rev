use crate::{
    backend::Backend,
    client_state::{self, ClientState},
    model::{ReviewRequest, ReviewResponse},
};
use gpui_kit::component::{
    Disableable, Selectable, Sizable, TitleBar,
    button::{Button, ButtonVariants},
    diff::{Diff, DiffFile, DiffMode, DiffState},
};
use gpui_kit::prelude::*;
use gpui_kit::*;
use serde_json::json;
use std::path::PathBuf;

gpui_kit::actions!(rev, [Quit, OpenFolder]);

pub fn window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some("Rev".into()),
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

pub struct Rev {
    backend: Option<Backend>,
    request: Option<ReviewRequest>,
    loading: bool,
    selecting_folder: bool,
    notices: Vec<String>,
    focus: FocusHandle,
    pub(crate) diff: Entity<DiffState>,
    split: bool,
    wrap: bool,
    title: SharedString,
    pub(crate) error: Option<String>,
    client: ClientState,
    data_dir: PathBuf,
}

impl Rev {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let data_dir = client_state::data_dir();
        let (client, error) = match ClientState::load(&data_dir) {
            Ok(client) => (client, None),
            Err(error) => (
                ClientState::default(),
                Some(format!("Couldn't read saved tabs: {error}")),
            ),
        };
        Self {
            backend: None,
            request: None,
            loading: false,
            selecting_folder: false,
            notices: vec![],
            focus,
            diff: cx.new(|cx| DiffState::new(vec![], cx)),
            split: false,
            wrap: true,
            title: "No folder open".into(),
            error,
            client,
            data_dir,
        }
    }

    /// Open the review that was active when the app last closed.
    pub fn reopen_last(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut app = Self::new(window, cx);
        if let Some(tab) = app.client.navigation.active_tab() {
            app.open_repo(tab.request.clone(), window, cx);
        }
        app
    }

    pub fn from_repo(request: ReviewRequest, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut app = Self::new(window, cx);
        app.open_repo(request, window, cx);
        app
    }

    fn open_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading || self.selecting_folder {
            return;
        }

        self.selecting_folder = true;
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open repository".into()),
        });

        cx.spawn_in(window, async move |this, cx| {
            let selection = selection.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.selecting_folder = false;
                match selection {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.first() {
                            this.open_repo(
                                ReviewRequest {
                                    repo: path.to_string_lossy().into_owned(),
                                    ..Default::default()
                                },
                                window,
                                cx,
                            );
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => this.error = Some(error.to_string()),
                    Err(error) => this.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn open_repo(&mut self, request: ReviewRequest, window: &Window, cx: &mut Context<Self>) {
        self.title = request.repo.clone().into();
        self.request = Some(request);
        self.notices.clear();
        self.diff.update(cx, |diff, cx| diff.set_files([], cx));
        self.load_review(window, cx);
    }

    fn load_review(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.loading || self.selecting_folder {
            return;
        }

        let Some(request) = self.request.clone() else {
            return;
        };

        // Start the local service only after a repository folder is opened.
        let backend = match self.backend.clone() {
            Some(backend) => backend,
            None => match Backend::start() {
                Ok(backend) => {
                    let service = backend.clone();
                    cx.on_app_quit(move |_, _| {
                        service.close();
                        async {}
                    })
                    .detach();
                    self.backend = Some(backend.clone());
                    backend
                }
                Err(error) => {
                    self.error = Some(error);
                    cx.notify();
                    return;
                }
            },
        };

        self.loading = true;
        self.error = None;
        let opened = request.clone();
        let task = cx.background_executor().spawn(async move {
            backend.call("review", json!(request)).and_then(|value| {
                serde_json::from_value::<ReviewResponse>(value).map_err(|error| error.to_string())
            })
        });

        cx.spawn_in(window, async move |this, cx| {
            let response = task.await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.loading = false;

                match response {
                    Ok(response) => {
                        this.client
                            .navigation
                            .remember_review(&response.review, &opened);
                        if let Err(error) = this.client.save(&this.data_dir) {
                            this.error = Some(format!("Couldn't save open tabs: {error}"));
                        }
                        this.title = format!(
                            "{} · {} → {}",
                            response.review.name, response.review.head, response.review.base
                        )
                        .into();

                        let mut files = Vec::new();
                        this.notices.clear();

                        for patch in response.patches {
                            let parsed = if let Some(error) = patch.error {
                                Err(error)
                            } else if patch.too_large {
                                Err("Diff exceeds the 2 MB inline limit.".into())
                            } else {
                                DiffFile::parse(&patch.patch).map_err(|error| error.to_string())
                            };

                            match parsed {
                                Ok(parsed) if !parsed.is_empty() => files.extend(parsed),
                                Ok(_) => this
                                    .notices
                                    .push(format!("{} · No textual changes", patch.path)),
                                Err(error) => {
                                    this.notices.push(format!("{} · {error}", patch.path))
                                }
                            }
                        }

                        this.diff.update(cx, |diff, cx| diff.set_files(files, cx));
                    }
                    Err(error) => this.error = Some(error),
                }

                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

impl Render for Rev {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("rev")
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x17191d))
            .text_color(rgb(0xe4e6eb))
            .text_size(px(13.))
            .font_family(".AppleSystemUIFont")
            .track_focus(&self.focus)
            .key_context("Rev")
            .on_action(cx.listener(|this, _: &OpenFolder, window, cx| this.open_folder(window, cx)))
            .child(
                TitleBar::new()
                    .h(px(44.))
                    .pl(px(80.))
                    .bg(rgb(0x131519))
                    .border_b_1()
                    .border_color(rgb(0x2b2e34))
                    .child("Rev"),
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
                        Button::new("open-folder")
                            .small()
                            .ghost()
                            .label("Open Folder…")
                            .disabled(self.loading || self.selecting_folder)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.open_folder(window, cx)),
                            ),
                    )
                    .when(self.request.is_some(), |bar| {
                        bar.child(
                            Button::new("refresh")
                                .small()
                                .ghost()
                                .label(if self.loading {
                                    "Loading…"
                                } else {
                                    "Refresh"
                                })
                                .disabled(self.loading || self.selecting_folder)
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.load_review(window, cx)),
                                ),
                        )
                    })
                    .child(
                        Button::new("view-mode")
                            .small()
                            .ghost()
                            .label(if self.split { "Split" } else { "Unified" })
                            .disabled(self.request.is_none())
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
                            .disabled(self.request.is_none())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.wrap = !this.wrap;
                                cx.notify();
                            })),
                    ),
            )
            .children(
                self.notices
                    .iter()
                    .map(|notice| div().px_3().text_color(rgb(0x929aa7)).child(notice.clone())),
            )
            .when_some(self.error.clone(), |d, error| {
                d.child(div().p_3().text_color(rgb(0xee9292)).child(error))
            })
            .when(self.request.is_none(), |d| {
                d.child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap_3()
                        .child("Open a local Git repository to review its changes.")
                        .child(
                            Button::new("open-first-folder")
                                .label("Open Folder…")
                                .disabled(self.selecting_folder)
                                .on_click(
                                    cx.listener(|this, _, window, cx| this.open_folder(window, cx)),
                                ),
                        ),
                )
            })
            .when(self.request.is_some(), |d| {
                d.child(
                    div().id("review-surface").flex_1().min_h_0().child(
                        Diff::new(&self.diff)
                            .size_full()
                            .soft_wrap(self.wrap)
                            .text_size(px(12.)),
                    ),
                )
            })
    }
}
