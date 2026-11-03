use super::*;
impl Render for Rev {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("rev")
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x17181b))
            .text_color(rgb(0xe4e6eb))
            .track_focus(&self.focus)
            .child(
                gpui_kit::component::TitleBar::new()
                    .h(px(44.))
                    .pl(px(80.))
                    .child("Rev"),
            )
            .child(
                div().px_3().py_2().child(
                    self.review
                        .as_ref()
                        .map(|r| r.name.clone())
                        .unwrap_or_else(|| "Open a repository from the CLI".into()),
                ),
            )
            .when_some(self.error.clone(), |d, error| {
                d.child(div().p_3().child(error))
            })
            .child(Diff::new(&self.diff).size_full().soft_wrap(self.wrap))
    }
}
