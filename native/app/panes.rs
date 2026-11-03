//! Independent render boundaries keep scrolling one pane from rebuilding its siblings.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum PaneKind {
    Diff,
    Files,
}

pub(super) struct Pane {
    owner: WeakEntity<Rev>,
    kind: PaneKind,
    _subscriptions: Vec<Subscription>,
}

impl Pane {
    pub(super) fn new(
        owner: Entity<Rev>,
        kind: PaneKind,
        diff: &Entity<DiffState>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut subscriptions = vec![cx.observe(&owner, |_, _, cx| {
            cx.notify();
        })];
        if matches!(kind, PaneKind::Diff) {
            subscriptions.push(cx.observe(diff, |_, _, cx| cx.notify()));
        }
        Self {
            owner: owner.downgrade(),
            kind,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for Pane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.owner
            .update(cx, |app, cx| match self.kind {
                PaneKind::Diff => app.diff_content(cx),
                PaneKind::Files => app.file_list(cx),
            })
            .unwrap_or_else(|_| div().into_any_element())
    }
}
