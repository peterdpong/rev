use super::view::{BLUE, BORDER, PANEL, button, column, row};
use super::*;
use gpui_kit::component::text::TextView;
impl Rev {
    pub(super) fn threads_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut list = column().gap_2();
        for thread in self
            .state
            .threads
            .clone()
            .into_iter()
            .filter(|thread| !thread.archived)
        {
            let id = thread.id.clone();
            let mut card = column()
                .gap_2()
                .p_3()
                .border_1()
                .border_color(rgb(BORDER))
                .rounded_md()
                .child(
                    button(
                        SharedString::from(format!("conversation-{id}")),
                        thread.body.clone(),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_thread(id.clone(), window, cx)
                    })),
                );
            for (index, event) in thread
                .turns
                .iter()
                .flat_map(|turn| &turn.events)
                .enumerate()
            {
                card = card.child(
                    TextView::markdown(
                        SharedString::from(format!("event-{}-{index}", thread.id)),
                        event.text.clone(),
                    )
                    .selectable(true),
                );
            }
            list = list.child(card);
        }
        for comment in self
            .state
            .comments
            .clone()
            .into_iter()
            .filter(|comment| comment.status == "queued")
        {
            list = list.child(
                column()
                    .p_3()
                    .border_1()
                    .border_color(rgb(BORDER))
                    .rounded_md()
                    .child(div().text_color(rgb(BLUE)).child(comment.selection.label()))
                    .child(div().child(comment.body)),
            );
        }
        let queued = self
            .state
            .comments
            .iter()
            .any(|comment| comment.status == "queued");
        column()
            .size_full()
            .bg(rgb(PANEL))
            .border_l_1()
            .border_color(rgb(BORDER))
            .child(
                row()
                    .h(px(44.))
                    .px_3()
                    .child(div().flex_1().child("Agent threads"))
                    .child(button("close-threads", "×").on_click(
                        cx.listener(|this, _, window, cx| this.hide_threads(window, cx)),
                    )),
            )
            .child(
                div()
                    .id("agent-conversations")
                    .flex_1()
                    .min_h_0()
                    .p_3()
                    .overflow_y_scroll()
                    .child(list),
            )
            .when(self.active_thread.is_some(), |panel| {
                panel.child(
                    column()
                        .p_3()
                        .key_context("RevThread")
                        .child(Textarea::new(&self.thread_reply).h(px(72.)))
                        .child(button("send-follow-up", "Send").on_click(
                            cx.listener(|this, _, window, cx| this.reply_thread(window, cx)),
                        )),
                )
            })
            .when(queued, |panel| {
                panel.child(
                    div().p_3().child(
                        button("apply-changes", "Apply changes")
                            .disabled(self.repository_agent_running())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.apply_queued_changes(window, cx)
                            })),
                    ),
                )
            })
            .into_any_element()
    }
}
