use super::{
    popups::modal,
    state::{Confirm, Popup, st},
    theme::*,
    widgets,
};
use nsh_core::prelude::*;
pub fn dialog() -> impl IntoElement {
    let s = st();
    let config = s.server.with(|v| v.clone());
    let address = signal(config.as_ref().map(|c| c.url.clone()).unwrap_or_else(crate::args::api_url));
    let document = signal(config.as_ref().map(|c| c.document.clone()).unwrap_or_else(crate::args::document_id));
    let token = signal(config.map(|c| c.token).unwrap_or_else(|| std::env::var("RUTABLE_API_TOKEN").unwrap_or_default()));
    modal(
        590.0,
        "连接服务器",
        move || s.popup.set(None),
        view()
            .p(px(20.0))
            .gap(px(12.0))
            .child(
                text("数据和附件保存在服务器 SQLite 中。此程序只使用内存，不创建本地数据文件或附件缓存。")
                    .text_size(px(14.0))
                    .text_color(rgb(TEXT)),
            )
            .child(text("服务器地址").text_size(px(12.0)).text_color(rgb(TEXT2)))
            .child(
                nsh_core::element::input()
                    .h(px(38.0))
                    .px(px(10.0))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .rounded(px(6.0))
                    .value(move || address.get())
                    .on_change(move |v: &str| address.set(v.into())),
            )
            .child(text("文档 ID").text_size(px(12.0)).text_color(rgb(TEXT2)))
            .child(
                nsh_core::element::input()
                    .h(px(38.0))
                    .px(px(10.0))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .rounded(px(6.0))
                    .value(move || document.get())
                    .on_change(move |v: &str| document.set(v.into())),
            )
            .child(text("访问密钥（仅在当前进程内使用，也可从环境变量读取）").text_size(px(12.0)).text_color(rgb(TEXT2)))
            .child(
                nsh_core::element::input()
                    .h(px(38.0))
                    .px(px(10.0))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .rounded(px(6.0))
                    .masked(true)
                    .value(move || token.get())
                    .on_change(move |v: &str| token.set(v.into())),
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
                    .child(widgets::primary_button("连接并读取", move || {
                        match crate::api::Config::new(&address.get_untracked(), &document.get_untracked(), token.get_untracked()) {
                            Ok(config) => s.connect_server(config),
                            Err(error) => s.cloud_error.set(Some(error.to_string())),
                        }
                    }))
                    .child(widgets::button("立即保存", move || s.save_now())),
            )
            .child(when(
                move || s.loaded.get(),
                move || {
                    widgets::link_button("重新读取服务器（放弃未保存的内存修改）", move || {
                        s.popup.set(Some(Popup::Confirm {
                            title: "重新读取服务器".into(),
                            text: "未提交的修改只在当前窗口内存中，重新读取后将丢失。已保存在服务器中的数据不会受影响。".into(),
                            action: Confirm::ReloadServer,
                        }))
                    })
                },
            ))
            .child(when(
                move || s.cloud_error.get().is_some() && s.is_dirty(),
                move || {
                    widgets::link_button("放弃未保存修改并退出", move || {
                        s.popup.set(Some(Popup::Confirm {
                            title: "放弃修改并退出".into(),
                            text: "修改尚未保存到服务器。客户端没有本地副本，退出后这些修改将无法恢复。".into(),
                            action: Confirm::DiscardAndClose,
                        }))
                    })
                },
            )),
    )
}
