use crate::{
    backend::Backend,
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

gpui_kit::actions!(rev, [Quit]);

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
    notices: Vec<String>,
    focus: FocusHandle,
    pub(crate) diff: Entity<DiffState>,
    split: bool,
    wrap: bool,
    title: SharedString,
    pub(crate) error: Option<String>,
}

impl Rev {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::from_patch(
            include_str!("../fixtures/sample.diff"),
            "Sample diff".into(),
            window,
            cx,
        )
    }

    pub fn from_patch(
        patch: &str,
        title: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (files, error) = match DiffFile::parse(patch) {
            Ok(files) => (files, None),
            Err(error) => (vec![], Some(error.to_string())),
        };
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        Self {
            backend: None,
            request: None,
            loading: false,
            notices: vec![],
            focus,
            diff: cx.new(|cx| DiffState::new(files, cx)),
            split: false,
            wrap: true,
            title,
            error,
        }
    }

    pub fn from_repo(
        backend: Backend,
        request: ReviewRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut app = Self::new(window, cx);
        app.backend = Some(backend);
        app.title = request.repo.clone().into();
        app.request = Some(request);
        app.diff.update(cx, |diff, cx| diff.set_files([], cx));
        app.load_review(window, cx);
        app
    }

    fn load_review(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }

        let (Some(backend), Some(request)) = (self.backend.clone(), self.request.clone()) else {
            return;
        };

        self.loading = true;
        self.error = None;
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
                                .disabled(self.loading)
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
            .child(
                div().id("review-surface").flex_1().min_h_0().child(
                    Diff::new(&self.diff)
                        .size_full()
                        .soft_wrap(self.wrap)
                        .text_size(px(12.)),
                ),
            )
    }
}
