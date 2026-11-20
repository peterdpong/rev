use super::*;

impl Rev {
    pub(super) fn navigate_tab(
        &mut self,
        direction: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.loading || self.workspace_busy || self.submitting {
            return;
        }
        let tabs: Vec<_> = self
            .workspace
            .projects
            .iter()
            .flat_map(|p| &p.tabs)
            .collect();
        if tabs.len() < 2 {
            return;
        }
        let index = tabs
            .iter()
            .position(|tab| Some(&tab.id) == self.workspace.active_tab_id.as_ref());
        let next = index.map_or_else(
            || if direction < 0 { tabs.len() - 1 } else { 0 },
            |index| (index as isize + direction).rem_euclid(tabs.len() as isize) as usize,
        );
        let tab = tabs[next].clone();
        self.close_modal(window, cx);
        self.load_review(tab.request, tab.file, window, cx);
    }

    pub(super) fn navigation_request(&self) -> Option<ReviewRequest> {
        self.workspace
            .active_tab()
            .map(|tab| tab.request.clone())
            .or_else(|| self.review.as_ref().map(|review| review.request.clone()))
    }
}
