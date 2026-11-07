//! Closing the window while changes could not reach the server: retry, or
//! give them up. (The client keeps no local copy.)

use super::{
    popups::modal,
    state::{Confirm, Popup, st},
    theme::*,
    widgets,
};
use nsh_core::prelude::*;

pub fn dialog() -> impl IntoElement {
    let s = st();
    modal(
        480.0,
        "修改尚未保存",
        move || s.popup.set(None),
        view()
            .p(px(20.0))
            .gap(px(12.0))
            .child(
                text("修改只在当前窗口中，还没能保存到服务器。检查网络后重试；放弃则关闭窗口，这些修改将丢失。")
                    .text_size(px(14.0))
                    .text_color(rgb(TEXT)),
            )
            .child(text(move || s.cloud_status.get()).text_size(px(13.0)).text_color(rgb(PRIMARY)))
            .child(when(
                move || s.cloud_error.get().is_some(),
                move || text(move || s.cloud_error.get().unwrap_or_default()).text_size(px(13.0)).text_color(rgb(DANGER)),
            ))
            .child(
                view()
                    .flex_row()
                    .gap(px(8.0))
                    .child(widgets::primary_button("重试保存", move || s.save_now()))
                    .child(widgets::button("放弃修改并退出", move || {
                        s.popup.set(Some(Popup::Confirm {
                            title: "放弃修改并退出".into(),
                            text: "修改尚未保存到服务器。客户端没有本地副本，退出后这些修改将无法恢复。".into(),
                            action: Confirm::DiscardAndClose,
                        }))
                    })),
            ),
    )
}
