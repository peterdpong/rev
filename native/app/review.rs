use super::controls::{button, column};
use gpui_kit::component::input::InputState;
use super::*;
use crate::theme::*;
use gpui_kit::component::text::TextView;

impl Rev {
    pub(super) fn open_review_details(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading || self.patch_loading || self.folder_picker_open {
            return;
        }
        if self
            .modal
            .as_ref()
            .is_some_and(|m| m.kind == ModalKind::Details)
        {
            self.close_modal(window, cx);
            return;
        }
        let Some(review) = &self.review else {
            return;
        };
        let request = review.request.clone();
        self.close_modal(window, cx);
        self.modal = Some(Modal {
            kind: ModalKind::Details,
            base_input: cx.new(|cx| InputState::new(window, cx)),
            head_input: cx.new(|cx| InputState::new(window, cx)),
            request,
            serial: self.modal_serial,
            anchor: Some("review-details".into()),
            anchor_bounds: Default::default(),
            local_branches: Vec::new(),
            branches_loading: false,
            branches_error: None,
        });
        self.modal_focus.focus(window, cx);
        cx.notify();
    }
    pub(super) fn review_details(&self) -> AnyElement {
        let Some(review) = &self.review else {
            return div().into_any_element();
        };
        let mut content = column()
            .gap(px(POPOVER_PADDING))
            .text_size(px(12.))
            .line_height(px(18.));
        if let Some(pr) = &review.pull_request {
            content = content
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(pr.title.clone()),
                )
                .child(div().text_color(rgb(MUTED)).child(format!(
                    "#{} · {} · {}",
                    pr.number,
                    pr.author,
                    pr.state.to_lowercase()
                )))
                .when(!pr.body.is_empty(), |d| {
                    d.child(TextView::markdown("pr-description", pr.body.clone()))
                })
                .child(button("open-pr-browser", "Open on GitHub ↗").on_click({
                    let url = pr.url.clone();
                    move |_, _, cx| cx.open_url(&url)
                }));
        }
        if let Some(commit) = &review.head_commit {
            content = content.child(
                column()
                    .gap(px(UI_SPACING))
                    .when(review.pull_request.is_some(), |d| {
                        d.pt(px(POPOVER_PADDING))
                            .border_t_1()
                            .border_color(rgb(BORDER))
                    })
                    .child(div().text_color(rgb(MUTED)).text_size(px(11.)).child(
                        if review.request.commit_range.is_some() {
                            "Selected commit"
                        } else {
                            "Latest commit"
                        },
                    ))
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(commit.subject.clone()),
                    )
                    .child(div().text_color(rgb(MUTED)).child(format!(
                        "{} · {} · {}",
                        commit.sha.chars().take(8).collect::<String>(),
                        commit.author,
                        commit.date.get(..10).unwrap_or(&commit.date)
                    )))
                    .when(!commit.body.is_empty(), |d| {
                        d.child(TextView::markdown(
                            "commit-description",
                            commit.body.clone(),
                        ))
                    }),
            );
        }
        content.into_any_element()
    }
}
