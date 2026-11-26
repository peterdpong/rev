use super::controls::*;
use super::settings_modal::SETTINGS_ROW_HEIGHT;
use super::*;
use crate::theme::*;
use gpui_kit::{assets::IconName, component::Icon};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum ConnectionState {
    Ready,
    SignedOut,
    Missing,
    Error,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ConnectionStatus {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) description: String,
    pub(super) state: ConnectionState,
    #[serde(default)]
    pub(super) version: Option<String>,
    #[serde(default)]
    pub(super) account: Option<String>,
    #[serde(default)]
    pub(super) detail: Option<String>,
}

impl ConnectionStatus {
    fn badge(&self) -> (&'static str, u32) {
        match self.state {
            ConnectionState::Ready => ("Connected", palette().success),
            ConnectionState::SignedOut => ("Signed out", palette().warning),
            ConnectionState::Missing => ("Not installed", palette().muted),
            ConnectionState::Error => ("Error", palette().danger),
        }
    }

    fn summary(&self) -> String {
        let mut parts = Vec::new();
        if let Some(version) = &self.version {
            parts.push(format!("v{version}"));
        }
        match self.state {
            ConnectionState::Ready => {
                if let Some(account) = &self.account {
                    parts.push(format!("Signed in as {account}"));
                }
            }
            ConnectionState::SignedOut => parts.push("Not signed in".into()),
            ConnectionState::Missing => parts.push("Not installed".into()),
            ConnectionState::Error => {}
        }
        if let Some(detail) = &self.detail {
            parts.push(detail.clone());
        }
        parts.join(" · ")
    }
}

#[derive(Default)]
pub(super) struct Connections {
    pub(super) checking: bool,
    pub(super) statuses: Option<Result<Vec<ConnectionStatus>, String>>,
}

impl Rev {
    pub(super) fn check_connections(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.connections.checking {
            return;
        }
        self.connections.checking = true;
        cx.notify();
        self.rpc_quiet(
            "connections",
            Value::Null,
            window,
            cx,
            |this, result, _, cx| {
                this.connections.checking = false;
                this.connections.statuses = Some(result.and_then(decode::<Vec<ConnectionStatus>>));
                cx.notify();
            },
        );
    }

    pub(super) fn connection_settings(&self, cx: &mut Context<Self>) -> Div {
        let checking = self.connections.checking;
        column()
            .flex_1()
            .min_w_0()
            .px(px(POPOVER_PADDING))
            .py(px(UI_SPACING))
            .text_size(px(12.))
            .child(
                row()
                    .h(px(SETTINGS_ROW_HEIGHT))
                    .justify_between()
                    .child(
                        div()
                            .text_color(rgb(palette().muted))
                            .child("Command-line tools rev uses"),
                    )
                    .child(
                        button(
                            "check-connections",
                            if checking {
                                "Checking…"
                            } else {
                                "Check again"
                            },
                        )
                        .h(px(HEADER_PILL_HEIGHT))
                        .disabled(checking)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.check_connections(window, cx)),
                        ),
                    ),
            )
            .map(|d| match &self.connections.statuses {
                None => d.child(connection_note("Checking…")),
                Some(Err(error)) => d.child(connection_note(error.clone())),
                Some(Ok(statuses)) => d.children(statuses.iter().map(connection_row)),
            })
    }
}

fn connection_row(status: &ConnectionStatus) -> Div {
    let (badge, color) = status.badge();
    let id = status.id.clone();
    row()
        .debug_selector(move || format!("connection-{id}"))
        .items_start()
        .gap(px(10.))
        .py(px(10.))
        .border_t_1()
        .border_color(rgb(palette().border))
        .child(
            Icon::new(if status.id == "gh" {
                IconName::Github
            } else {
                IconName::Command
            })
            .size(px(16.))
            .mt(px(1.))
            .flex_shrink_0()
            .text_color(rgb(palette().muted)),
        )
        .child(
            column()
                .flex_1()
                .min_w_0()
                .gap(px(2.))
                .line_height(px(16.))
                .child(
                    row()
                        .gap(px(6.))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(status.name.clone()),
                        )
                        .child(
                            div()
                                .text_color(rgb(palette().muted))
                                .truncate()
                                .child(status.description.clone()),
                        ),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(palette().muted))
                        .whitespace_normal()
                        .child(status.summary()),
                ),
        )
        .child(
            row()
                .flex_shrink_0()
                .gap(px(5.))
                .text_size(px(11.))
                .line_height(px(16.))
                .text_color(rgb(color))
                .child(div().size(px(6.)).rounded_full().bg(rgb(color)))
                .child(badge),
        )
}

fn connection_note(text: impl Into<SharedString>) -> Div {
    div()
        .py(px(10.))
        .border_t_1()
        .border_color(rgb(palette().border))
        .text_color(rgb(palette().muted))
        .child(text.into())
}

#[cfg(test)]
mod tests {
    use super::{ConnectionState, ConnectionStatus};

    #[test]
    fn statuses_summarize_version_account_and_next_step() {
        let status: ConnectionStatus = serde_json::from_value(serde_json::json!({
            "id": "gh",
            "name": "GitHub CLI",
            "description": "Pull request details",
            "state": "ready",
            "path": "/opt/homebrew/bin/gh",
            "version": "2.62.0",
            "account": "octocat"
        }))
        .unwrap();
        assert_eq!(status.summary(), "v2.62.0 · Signed in as octocat");

        let signed_out = ConnectionStatus {
            state: ConnectionState::SignedOut,
            account: None,
            detail: Some("Run gh auth login in a terminal.".into()),
            ..status.clone()
        };
        assert_eq!(
            signed_out.summary(),
            "v2.62.0 · Not signed in · Run gh auth login in a terminal."
        );

        let missing = ConnectionStatus {
            state: ConnectionState::Missing,
            version: None,
            account: None,
            detail: Some("Install it with brew install gh.".into()),
            ..status
        };
        assert_eq!(
            missing.summary(),
            "Not installed · Install it with brew install gh."
        );
    }
}
