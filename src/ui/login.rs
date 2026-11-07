//! Before anything else: the server address with its first password, or
//! logging in. The address and the login are remembered by this computer
//! (the login encrypted), so this shows once.

use nsh_core::prelude::*;

use super::{state::st, theme::*, widgets};

#[derive(Clone, Debug, PartialEq)]
enum Phase {
    Checking,
    /// No password yet; `key`: the server key is known (environment).
    Setup { key: bool },
    Login,
    Offline(String),
}

enum Found {
    In(crate::api::Config),
    Setup,
    Login,
}

/// What submitting the form led to: in, or the server (perhaps a newly typed
/// address) needs the other form.
enum Sent {
    Token(String),
    NeedSetup,
    NeedLogin,
}

/// Remembered login, or what the server needs first.
fn check(url: &str) -> Result<Found, crate::api::Error> {
    // Scripts name a base and carry the server key: straight in.
    if let (Some(_), Ok(key)) = (crate::args::explicit_document(), std::env::var("RUTABLE_API_TOKEN")) {
        if !key.trim().is_empty() {
            return Ok(Found::In(crate::api::Config::new(url, "default", key)?));
        }
    }
    if let Some(token) = crate::login::load(url) {
        let c = crate::api::Config::new(url, "default", token)?;
        match c.bases() {
            Ok(_) => return Ok(Found::In(c)),
            Err(crate::api::Error::Unauthorized) => crate::login::forget(url),
            Err(e) => return Err(e),
        }
    }
    Ok(if crate::api::password_set(url)? { Found::Login } else { Found::Setup })
}

fn enter(url: &str, token: String) {
    crate::login::save(url, &token);
    crate::login::save_server(url);
    match crate::api::Config::new(url, "default", token) {
        Ok(c) => st().signed_in(c, crate::args::explicit_document()),
        Err(e) => st().toast(&e.to_string()),
    }
}

pub fn login() -> impl IntoElement {
    let url = signal(crate::args::api_url());
    let phase = signal(Phase::Checking);
    let (key, pass, again, err, busy) = (signal(String::new()), signal(String::new()), signal(String::new()), signal(String::new()), signal(false));
    crate::win::set_title("Rutable");
    let run_check = move || {
        phase.set(Phase::Checking);
        err.set(String::new());
        let u = url.get_untracked();
        let (tx, rx) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let _ = tx.send_blocking(check(&u));
        });
        st().spawn(async move {
            match rx.recv().await {
                Ok(Ok(Found::In(c))) => st().signed_in(c, crate::args::explicit_document()),
                Ok(Ok(Found::Setup)) => phase.set(Phase::Setup { key: std::env::var("RUTABLE_API_TOKEN").is_ok_and(|k| !k.trim().is_empty()) }),
                Ok(Ok(Found::Login)) => phase.set(Phase::Login),
                Ok(Err(e)) => phase.set(Phase::Offline(e.to_string())),
                Err(_) => phase.set(Phase::Offline("请求中断".into())),
            }
        });
    };
    run_check();
    let submit = move || {
        if busy.get_untracked() {
            return;
        }
        let u = url.get_untracked().trim().trim_end_matches('/').to_string();
        if u.is_empty() {
            return err.set("请填写服务器地址".into());
        }
        let p = pass.get_untracked();
        let setup = match phase.get_untracked() {
            Phase::Setup { key: known } => {
                if p.chars().count() < 8 {
                    return err.set("密码至少 8 个字符".into());
                }
                if p != again.get_untracked() {
                    return err.set("两次输入的密码不一致".into());
                }
                Some(if known { std::env::var("RUTABLE_API_TOKEN").unwrap_or_default() } else { key.get_untracked() })
            }
            Phase::Offline(_) => return run_check(),
            Phase::Login => {
                if p.is_empty() {
                    return err.set("请输入密码".into());
                }
                None
            }
            _ => return,
        };
        busy.set(true);
        err.set(String::new());
        let (tx, rx) = async_channel::bounded(1);
        let u2 = u.clone();
        std::thread::spawn(move || {
            // The address may have been changed: ask that server what it needs.
            let r = crate::api::password_set(&u2).and_then(|set| match (setup, set) {
                (Some(_), true) => Ok(Sent::NeedLogin),
                (None, false) => Ok(Sent::NeedSetup),
                (Some(k), false) => crate::api::setup(&u2, k.trim(), &p).map(Sent::Token),
                (None, true) => crate::api::login(&u2, &p).map(Sent::Token),
            });
            let _ = tx.send_blocking(r);
        });
        st().spawn(async move {
            let r = rx.recv().await;
            busy.set(false);
            match r {
                Ok(Ok(Sent::Token(token))) => enter(&u, token),
                Ok(Ok(Sent::NeedSetup)) => {
                    phase.set(Phase::Setup { key: std::env::var("RUTABLE_API_TOKEN").is_ok_and(|k| !k.trim().is_empty()) });
                    err.set("这台服务器还没有访问密码，请先设置".into());
                }
                Ok(Ok(Sent::NeedLogin)) => {
                    phase.set(Phase::Login);
                    err.set("这台服务器已设置过密码，请直接登录".into());
                }
                Ok(Err(e)) => err.set(e.to_string()),
                Err(_) => err.set("请求中断".into()),
            }
        });
    };
    let field = move |label: &'static str, v: Signal<String>, focus: bool, masked: bool| {
        view().gap(px(6.0)).child(text(label).text_size(px(13.0)).text_color(rgb(TEXT2))).child(
            nsh_core::element::input()
                .h(px(40.0))
                .px(px(12.0))
                .border_1()
                .border_color(rgb(BORDER))
                .rounded(px(8.0))
                .text_size(px(15.0))
                .masked(masked)
                .auto_focus(focus)
                .value(move || v.get())
                .on_change(move |t: &str| v.set(t.to_string()))
                .on_submit(move |_: &str| submit()),
        )
    };
    view().flex_1().bg(rgb(BG_GREY)).items_center().justify_center().child(
        view()
            .w(px(420.0))
            .bg(rgb(BG))
            .rounded(px(12.0))
            .border_1()
            .border_color(rgb(BORDER))
            .shadow_lg()
            .p(px(32.0))
            .gap(px(18.0))
            .child(
                view()
                    .flex_row()
                    .items_center()
                    .gap(px(10.0))
                    .child(super::home::base_icon("", 36.0))
                    .child(text("Rutable 多维表格").text_size(px(20.0)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(TEXT))),
            )
            .child(dynamic(move || match phase.get() {
                Phase::Checking => vec![text(move || format!("正在连接 {}…", url.get())).text_size(px(14.0)).text_color(rgb(TEXT2)).into_element()],
                Phase::Offline(e) => vec![
                    text(e).text_size(px(14.0)).text_color(rgb(DANGER)).into_element(),
                    field("服务器地址", url, false, false).into_element(),
                    view().flex_row().child(widgets::primary_button("连接", run_check)).into_element(),
                ],
                Phase::Setup { key: known } => {
                    let mut v = vec![
                        text("设置访问密码").text_size(px(17.0)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT)).into_element(),
                        text("首次使用：为这台服务器设置访问密码。之后每台电脑用这个密码登录一次即可，客户端会记住登录。")
                            .text_size(px(13.0))
                            .text_color(rgb(TEXT2))
                            .into_element(),
                    ];
                    v.push(field("服务器地址", url, false, false).into_element());
                    if !known {
                        v.push(field("服务器密钥（部署时的 RUTABLE_API_TOKEN，证明你是服务器的主人）", key, true, true).into_element());
                    }
                    v.push(field("密码（至少 8 个字符）", pass, known, true).into_element());
                    v.push(field("再次输入密码", again, false, true).into_element());
                    v
                }
                Phase::Login => vec![
                    text("输入访问密码").text_size(px(17.0)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT)).into_element(),
                    field("服务器地址", url, false, false).into_element(),
                    field("密码", pass, true, true).into_element(),
                    text("这台电脑会记住服务器地址和登录（180 天）；修改密码后需重新登录。").text_size(px(12.0)).text_color(rgb(TEXT3)).into_element(),
                ],
            }))
            .child(when(move || !err.get().is_empty(), move || text(move || err.get()).text_size(px(13.0)).text_color(rgb(DANGER))))
            .child(when(
                move || matches!(phase.get(), Phase::Setup { .. } | Phase::Login),
                move || {
                    view()
                        .h(px(40.0))
                        .rounded(px(8.0))
                        .bg(move || rgb(if busy.get() { PRIMARY_HOVER } else { PRIMARY }))
                        .hover(|s| s.bg(rgb(PRIMARY_HOVER)))
                        .items_center()
                        .justify_center()
                        .on_press(submit)
                        .child(
                            text(move || match (busy.get(), phase.get()) {
                                (true, _) => "请稍候…",
                                (_, Phase::Setup { .. }) => "设置并进入",
                                _ => "登录",
                            })
                            .text_size(px(15.0))
                            .text_color(rgb(0xFFFFFF)),
                        )
                },
            )),
    )
}
