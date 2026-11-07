//! The list of bases (多维表格) the window opens on: open, create, rename,
//! pick an emoji, drag to reorder, delete into 最近删除 and restore. Also
//! the account menu: 复制给 AI (MCP), AI 密钥, 修改密码, 退出登录.

use nsh_core::prelude::*;

use super::{
    icons,
    popups::modal,
    state::{Confirm, Popup, st},
    theme::*,
    widgets,
};
use crate::api::BaseEntry;

/// Emoji to pick from, by group.
pub const EMOJI: &[(&str, &[&str])] = &[
    ("常用", &["📊", "📈", "📋", "🗂️", "📁", "📌", "✅", "🚀", "💡", "🎯", "⭐", "🔥"]),
    ("工作", &["💼", "🧾", "📝", "📅", "⏰", "📦", "🛒", "💰", "💳", "🏷️", "📣", "🤝"]),
    ("技术", &["💻", "📱", "🤖", "🧠", "🔧", "⚙️", "🧪", "🔗", "🌐", "🔍", "🛡️", "🐞"]),
    ("生活", &["🏠", "🍜", "☕", "🎵", "🎬", "📚", "✈️", "🏃", "🎮", "🎨", "📷", "🌱"]),
    ("符号", &["❤️", "💜", "💙", "💚", "💛", "🧡", "🟥", "🟦", "🟩", "🟨", "⚡", "🌙"]),
];

const ROW_H: f32 = 72.0;

thread_local! {
    static RENAME: std::cell::RefCell<Option<(Signal<Option<String>>, Signal<String>)>> = const { std::cell::RefCell::new(None) };
}

/// The base's icon: its emoji on a soft tile, or the table glyph.
pub fn base_icon(icon: &str, size: f32) -> AnyElement {
    if icon.is_empty() {
        view()
            .size(px(size))
            .rounded(px(size * 0.22))
            .bg(rgb(PRIMARY))
            .items_center()
            .justify_center()
            .child(icons::icon("table", size * 0.56, 0xFFFFFF))
            .into_element()
    } else {
        view()
            .size(px(size))
            .rounded(px(size * 0.22))
            .bg(rgb(BG_GREY))
            .items_center()
            .justify_center()
            .child(text(icon.to_string()).text_size(px(size * 0.62)))
            .into_element()
    }
}

fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn when_text(ms: i64) -> String {
    crate::model::value::format_date(ms, crate::model::DateFormat::YmdSlash, true)
}

fn account() -> Option<crate::api::Config> {
    st().account.with(|a| a.clone())
}

pub fn open(id: &str) {
    let Some(a) = account() else { return };
    let s = st();
    let entry = s.bases.get_untracked().into_iter().find(|b| b.id == id);
    s.connect_server(a.with_document(id));
    // Its name and icon show while it opens.
    if let Some(e) = entry {
        s.doc.with(|b| {
            b.name = e.name;
            b.icon = e.icon;
        });
        s.rev.update(|r| *r += 1);
    }
}

fn start_rename(id: &str, name: &str) {
    RENAME.with(|r| {
        if let Some((who, txt)) = *r.borrow() {
            txt.set(name.to_string());
            who.set(Some(id.to_string()));
        }
    });
}

fn finish_rename(who: Signal<Option<String>>, txt: Signal<String>) {
    let Some(id) = who.get_untracked() else { return };
    who.set(None);
    let name = txt.get_untracked().trim().to_string();
    let s = st();
    let unchanged = s.bases.get_untracked().iter().any(|b| b.id == id && b.name == name);
    if name.is_empty() || unchanged {
        return;
    }
    // Shown at once; the server's answer follows.
    s.bases.update(|l| l.iter_mut().filter(|b| b.id == id).for_each(|b| b.name = name.clone()));
    let Some(a) = account() else { return };
    s.background(move || a.update_base(&id, Some(&name), None), move |_| st().load_bases());
}

pub fn set_icon(id: &str, icon: &str) {
    let s = st();
    let (id, icon) = (id.to_string(), icon.to_string());
    s.bases.update(|l| l.iter_mut().filter(|b| b.id == id).for_each(|b| b.icon = icon.clone()));
    let Some(a) = account() else { return };
    s.background(move || a.update_base(&id, None, Some(&icon)), move |_| st().load_bases());
}

fn create() {
    let s = st();
    let Some(a) = account() else { return };
    let pool: Vec<&str> = EMOJI.iter().flat_map(|(_, e)| e.iter().copied()).collect();
    let icon = pool[(crate::model::now_ms() as usize / 7) % pool.len()].to_string();
    s.background(move || a.create_base("未命名多维表格", &icon), move |e: BaseEntry| {
        let s = st();
        let (id, name) = (e.id.clone(), e.name.clone());
        s.bases.update(|l| l.push(e));
        start_rename(&id, &name);
        s.load_bases();
    });
}

pub fn delete(id: &str) {
    let s = st();
    let id = id.to_string();
    s.bases.update(|l| l.retain(|b| b.id != id));
    let Some(a) = account() else { return };
    s.background(move || a.delete_base(&id), move |_| {
        st().toast("已移到「最近删除」，可随时恢复");
        st().load_bases();
    });
}

fn restore(id: &str) {
    let s = st();
    let id = id.to_string();
    let Some(a) = account() else { return };
    s.background(move || a.restore_base(&id), move |_| {
        st().toast("已恢复");
        st().load_bases();
    });
}

/// Moves base `from` to position `to` and saves the order.
fn reorder(from: &str, to: usize) {
    let s = st();
    let mut list = s.bases.get_untracked();
    let Some(i) = list.iter().position(|b| b.id == from) else { return };
    let item = list.remove(i);
    list.insert(to.min(list.len()), item);
    let ids: Vec<String> = list.iter().map(|b| b.id.clone()).collect();
    s.bases.set(list);
    let Some(a) = account() else { return };
    s.background(move || a.reorder_bases(&ids), move |l| st().bases.set(l));
}

/// Makes a key for an AI tool and copies ready-to-paste MCP settings.
pub fn copy_for_ai() {
    let s = st();
    let Some(a) = account() else { return };
    let name = format!("AI · {}", when_text(crate::model::now_ms()));
    let url = a.url.clone();
    s.toast("正在生成 AI 密钥…");
    s.background(move || a.create_ai_key(&name), move |key: String| {
        let mcp = format!("{url}/mcp");
        let text = format!(
            "Rutable 多维表格 MCP 服务器（AI 可直接读写表格，客户端实时显示变化）\n\
             地址：{mcp}\n\
             请求头：Authorization: Bearer {key}\n\n\
             Claude Code：\n\
             claude mcp add --transport http rutable {mcp} --header \"Authorization: Bearer {key}\"\n\n\
             通用 JSON 配置（Cursor、Claude Desktop 等）：\n\
             {{\"mcpServers\":{{\"rutable\":{{\"type\":\"http\",\"url\":\"{mcp}\",\"headers\":{{\"Authorization\":\"Bearer {key}\"}}}}}}}}\n\n\
             可用工具：list_bases、describe_table、list_records、create_records、update_records、delete_records、create_field、create_table。\n\
             此密钥可在 Rutable 客户端「AI 访问」中随时撤销。"
        );
        set_clipboard(text);
        st().toast("已复制 MCP 配置，粘贴给 AI 即可");
    });
}

pub fn home() -> impl IntoElement {
    let s = st();
    let who = signal(None::<String>);
    let txt = signal(String::new());
    RENAME.with(|r| *r.borrow_mut() = Some((who, txt)));
    let drag = signal(None::<(String, usize)>);
    let show_trash = signal(false);
    let account_at = element_ref();
    if s.bases.get_untracked().is_empty() {
        s.load_bases();
    }
    crate::win::set_title("Rutable");

    view()
        .flex_1()
        .bg(rgb(BG_GREY))
        .child(
            // Top bar.
            view()
                .h(px(TOPBAR_H))
                .flex_row()
                .items_center()
                .px(px(20.0))
                .gap(px(10.0))
                .bg(rgb(BG))
                .border_b(px(1.0))
                .border_color(rgb(BORDER))
                .child(base_icon("", 28.0))
                .child(text("Rutable").text_size(px(17.0)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(TEXT)))
                .child(view().flex_1())
                .child(widgets::button("复制给 AI", copy_for_ai))
                .child(
                    view()
                        .element_ref(&account_at)
                        .on_press(move || widgets::below(account_at, move |x, y| st().popup.set(Some(Popup::Account { x, y }))))
                        .child(widgets::avatar_el("我", 0, 30.0)),
                ),
        )
        .child(
            scroll().flex_1().child(
                view().items_center().py(px(32.0)).px(px(24.0)).child(
                    view()
                        .w_full()
                        .max_w(px(960.0))
                        .gap(px(16.0))
                        .child(
                            view()
                                .flex_row()
                                .items_center()
                                .child(
                                    view()
                                        .flex_1()
                                        .gap(px(2.0))
                                        .child(text("我的多维表格").text_size(px(22.0)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(TEXT)))
                                        .child(
                                            text(move || {
                                                let l = s.bases.get();
                                                if s.bases_loading.get() && l.is_empty() {
                                                    "正在读取…".to_string()
                                                } else {
                                                    format!("{} 个多维表格 · 拖动排序，点击图标更换 emoji", l.len())
                                                }
                                            })
                                            .text_size(px(13.0))
                                            .text_color(rgb(TEXT3)),
                                        ),
                                )
                                .child(widgets::primary_button("＋ 新建多维表格", create)),
                        )
                        .child(
                            view()
                                .bg(rgb(BG))
                                .rounded(px(10.0))
                                .border_1()
                                .border_color(rgb(BORDER))
                                .child(dynamic(move || {
                                    let list = s.bases.get();
                                    if list.is_empty() && !s.bases_loading.get() {
                                        return vec![empty_state()];
                                    }
                                    let n = list.len();
                                    list.into_iter().enumerate().map(|(i, b)| row(b, i, n, who, txt, drag)).collect::<Vec<_>>()
                                })),
                        )
                        .child(when(
                            move || !s.trash.get().is_empty(),
                            move || {
                                view()
                                    .gap(px(8.0))
                                    .child(
                                        view()
                                            .flex_row()
                                            .items_center()
                                            .gap(px(6.0))
                                            .py(px(4.0))
                                            .on_press(move || show_trash.update(|v| *v = !*v))
                                            .child(icons::icon_dyn(move || if show_trash.get() { "chevron-down" } else { "chevron-right" }, 14.0, move || TEXT2))
                                            .child(text(move || format!("最近删除（{}）", s.trash.get().len())).text_size(px(14.0)).text_color(rgb(TEXT2))),
                                    )
                                    .child(when(
                                        move || show_trash.get(),
                                        move || {
                                            view()
                                                .bg(rgb(BG))
                                                .rounded(px(10.0))
                                                .border_1()
                                                .border_color(rgb(BORDER))
                                                .child(dynamic(move || s.trash.get().into_iter().map(trash_row).collect::<Vec<_>>()))
                                        },
                                    ))
                            },
                        )),
                ),
            ),
        )
}

fn empty_state() -> AnyElement {
    view()
        .py(px(48.0))
        .items_center()
        .gap(px(12.0))
        .child(text("📊").text_size(px(40.0)))
        .child(text("还没有多维表格").text_size(px(15.0)).text_color(rgb(TEXT2)))
        .child(widgets::primary_button("新建多维表格", create))
        .into_element()
}

fn row(b: BaseEntry, index: usize, count: usize, who: Signal<Option<String>>, txt: Signal<String>, drag: Signal<Option<(String, usize)>>) -> AnyElement {
    let hovered = signal(false);
    let icon_at = element_ref();
    let more_at = element_ref();
    let id = b.id.clone();
    let renaming = {
        let id = id.clone();
        move || who.get().as_deref() == Some(id.as_str())
    };
    let dragging = {
        let id = id.clone();
        move || drag.get().is_some_and(|(d, _)| d == id)
    };
    let drop_line = move || drag.get().is_some_and(|(_, to)| to == index);
    let (id_open, id_icon, id_more, id_drag) = (id.clone(), id.clone(), id.clone(), id.clone());
    let (name_more, icon, title, name_menu) = (b.name.clone(), b.icon.clone(), b.name.clone(), b.name.clone());
    let detail = format!("{} 张数据表 · {} 条记录 · 更新于 {}", b.tables, thousands(b.records), when_text(b.updated_at));
    view()
        .h(px(ROW_H))
        .px(px(16.0))
        .flex_row()
        .items_center()
        .gap(px(14.0))
        .border_t(move || px(if drop_line() { 2.0 } else if index > 0 { 1.0 } else { 0.0 }))
        .border_color(move || rgb(if drop_line() { PRIMARY } else { LINE }))
        .opacity(move || if dragging() { 0.5 } else { 1.0 })
        .hover(|s| s.bg(rgb(HOVER)))
        .on_hover_in(move || hovered.set(true))
        .on_hover_out(move || hovered.set(false))
        .on_press(move || {
            if who.get_untracked().is_none() {
                open(&id_open);
            }
        })
        .on_context_menu(move |e: &ContextMenuEvent| {
            st().popup.set(Some(Popup::BaseMenu { base: id_more.clone(), name: name_more.clone(), x: e.page_position.x.0, y: e.page_position.y.0 }))
        })
        .claim_touches(TouchClaim::on_move())
        .on_responder_grant(move |_: &ResponderEvent| drag.set(Some((id_drag.clone(), index))))
        .on_responder_move(move |e: &ResponderEvent| {
            let to = (index as f32 + (e.gesture.delta.y.0 / ROW_H).round()).clamp(0.0, count.saturating_sub(1) as f32) as usize;
            if let Some((d, _)) = drag.get_untracked() {
                drag.set(Some((d, to)));
            }
        })
        .on_responder_release(move |_: &ResponderEvent| {
            if let Some((moving, to)) = drag.get_untracked() {
                drag.set(None);
                if to != index {
                    reorder(&moving, to);
                }
            }
        })
        .on_responder_terminate(move |_: &ResponderEvent| drag.set(None))
        .child(
            view()
                .w(px(14.0))
                .opacity(move || if hovered.get() { 1.0 } else { 0.0 })
                .child(icons::icon("grip", 14.0, TEXT3)),
        )
        .child(
            // The icon: press to pick an emoji.
            view()
                .element_ref(&icon_at)
                .rounded(px(10.0))
                .hover(|s| s.opacity(0.8))
                .on_press(move || {
                    let id = id_icon.clone();
                    widgets::below(icon_at, move |x, y| st().popup.set(Some(Popup::EmojiPicker { base: id.clone(), x, y })))
                })
                .child(base_icon(&icon, 40.0)),
        )
        .child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .gap(px(3.0))
                .child(dynamic(move || {
                    if renaming() {
                        nsh_core::element::input()
                            .h(px(30.0))
                            .max_w(px(420.0))
                            .px(px(8.0))
                            .rounded(px(6.0))
                            .bg(rgb(BG))
                            .border_1()
                            .border_color(rgb(PRIMARY))
                            .text_size(px(15.0))
                            .auto_focus(true)
                            .value(move || txt.get())
                            .on_change(move |t: &str| txt.set(t.to_string()))
                            .on_submit(move |_: &str| finish_rename(who, txt))
                            .on_blur(move || finish_rename(who, txt))
                            .into_element()
                    } else {
                        text(title.clone()).text_size(px(15.0)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT)).number_of_lines(1u32).into_element()
                    }
                }))
                .child(text(detail).text_size(px(12.0)).text_color(rgb(TEXT3)).number_of_lines(1u32)),
        )
        .child(
            view()
                .opacity(move || if hovered.get() { 1.0 } else { 0.0 })
                .child(text("打开 →").text_size(px(13.0)).text_color(rgb(PRIMARY))),
        )
        .child(
            view()
                .element_ref(&more_at)
                .size(px(30.0))
                .rounded(px(6.0))
                .items_center()
                .justify_center()
                .hover(|s| s.bg(rgb(0xDEE0E3)))
                .on_press(move || {
                    let (id, name) = (id.clone(), name_menu.clone());
                    widgets::below(more_at, move |x, y| st().popup.set(Some(Popup::BaseMenu { base: id.clone(), name: name.clone(), x, y })))
                })
                .child(icons::icon("more", 18.0, TEXT2)),
        )
        .into_element()
}

fn trash_row(b: BaseEntry) -> AnyElement {
    let id = b.id.clone();
    view()
        .h(px(56.0))
        .px(px(16.0))
        .flex_row()
        .items_center()
        .gap(px(12.0))
        .border_b(px(1.0))
        .border_color(rgb(LINE))
        .child(view().opacity(0.6).child(base_icon(&b.icon, 32.0)))
        .child(
            view()
                .flex_1()
                .gap(px(2.0))
                .child(text(b.name.clone()).text_size(px(14.0)).text_color(rgb(TEXT2)))
                .child(
                    text(format!("{} 条记录 · 删除于 {}", thousands(b.records), b.deleted_at.map(when_text).unwrap_or_default()))
                        .text_size(px(12.0))
                        .text_color(rgb(TEXT3)),
                ),
        )
        .child(widgets::button("恢复", move || restore(&id)))
        .into_element()
}

pub fn base_menu(base: String, name: String, x: f32, y: f32) -> impl IntoElement {
    let close = || st().popup.set(None);
    let (b1, b2, b3, b4) = (base.clone(), base.clone(), base.clone(), base.clone());
    let (n2, n4) = (name.clone(), name.clone());
    widgets::popover_at(
        x,
        y,
        200.0,
        close,
        view()
            .child(widgets::menu_item("table", "打开", move || {
                st().popup.set(None);
                open(&b1);
            }))
            .child(widgets::menu_item("edit", "重命名", move || {
                st().popup.set(None);
                start_rename(&b2, &n2);
            }))
            .child(widgets::menu_item("star", "更换图标", move || {
                st().popup.set(Some(Popup::EmojiPicker { base: b3.clone(), x, y }));
            }))
            .child(widgets::separator())
            .child(widgets::menu_item_danger("trash", "删除", move || {
                st().popup.set(Some(Popup::Confirm {
                    title: "删除多维表格".into(),
                    text: format!("「{n4}」将移到「最近删除」，其中的数据不会丢失，可随时恢复。"),
                    action: Confirm::DeleteBase(b4.clone(), n4.clone()),
                }))
            })),
    )
}

pub fn emoji_picker(base: String, x: f32, y: f32) -> impl IntoElement {
    let close = || st().popup.set(None);
    let b_clear = base.clone();
    widgets::popover_at(
        x,
        y,
        372.0,
        close,
        view()
            .px(px(10.0))
            .py(px(6.0))
            .gap(px(4.0))
            .children(EMOJI.iter().map(move |(group, list)| {
                let base = base.clone();
                view()
                    .gap(px(2.0))
                    .child(text(*group).text_size(px(12.0)).text_color(rgb(TEXT3)).py(px(4.0)))
                    .child(view().flex_row().flex_wrap().children(list.iter().map(move |e| {
                        let (base, e, shown) = (base.clone(), e.to_string(), e.to_string());
                        view()
                            .size(px(29.0))
                            .rounded(px(6.0))
                            .items_center()
                            .justify_center()
                            .hover(|s| s.bg(rgb(HOVER)))
                            .on_press(move || {
                                st().popup.set(None);
                                set_icon(&base, &e);
                            })
                            .child(text(shown).text_size(px(19.0)))
                            .into_element()
                    })))
                    .into_element()
            }))
            .child(widgets::separator())
            .child(widgets::menu_item("x", "使用默认图标", move || {
                st().popup.set(None);
                set_icon(&b_clear, "");
            })),
    )
}

pub fn account_menu(x: f32, y: f32) -> impl IntoElement {
    let close = || st().popup.set(None);
    let server = account().map(|a| a.url).unwrap_or_default();
    widgets::popover_at(
        x - 180.0,
        y,
        240.0,
        close,
        view()
            .child(widgets::menu_title(format!("已登录 {}", server.trim_start_matches("https://"))))
            .child(widgets::menu_item("robot", "复制给 AI（MCP 配置）", move || {
                st().popup.set(None);
                copy_for_ai();
            }))
            .child(widgets::menu_item("users", "管理 AI 访问", move || st().popup.set(Some(Popup::AiKeys))))
            .child(widgets::menu_item("edit", "修改密码", move || st().popup.set(Some(Popup::ChangePassword))))
            .child(widgets::separator())
            .child(widgets::menu_item("x", "退出登录", move || {
                let s = st();
                s.popup.set(None);
                if let Some(a) = account() {
                    // Ends this computer's login on the server too.
                    std::thread::spawn(move || {
                        let _ = a.logout();
                    });
                }
                s.signed_out();
            })),
    )
}

pub fn ai_keys() -> impl IntoElement {
    let keys = signal(Vec::<serde_json::Value>::new());
    let load = move || {
        if let Some(a) = account() {
            st().background(move || a.ai_keys(), move |k| keys.set(k));
        }
    };
    load();
    modal(
        560.0,
        "AI 访问",
        || st().popup.set(None),
        view()
            .p(px(20.0))
            .gap(px(12.0))
            .child(
                text("每次「复制给 AI」都会生成一个独立密钥。AI 用它通过 MCP 读写你的多维表格；不再使用的可随时撤销，撤销后立即失效。")
                    .text_size(px(14.0))
                    .text_color(rgb(TEXT2)),
            )
            .child(dynamic(move || {
                let list = keys.get();
                if list.is_empty() {
                    return vec![text("还没有 AI 密钥").text_size(px(13.0)).text_color(rgb(TEXT3)).into_element()];
                }
                list.into_iter()
                    .map(|k| {
                        let id = k["id"].as_str().unwrap_or("").to_string();
                        let used = k["last_used"].as_i64().unwrap_or(0);
                        let made = k["created_at"].as_i64().unwrap_or(0);
                        view()
                            .flex_row()
                            .items_center()
                            .gap(px(10.0))
                            .py(px(8.0))
                            .border_b(px(1.0))
                            .border_color(rgb(LINE))
                            .child(
                                view()
                                    .flex_1()
                                    .gap(px(2.0))
                                    .child(text(k["name"].as_str().unwrap_or("AI").to_string()).text_size(px(14.0)).text_color(rgb(TEXT)))
                                    .child(
                                        text(if used > made { format!("最近使用 {}", when_text(used)) } else { "尚未使用".into() })
                                            .text_size(px(12.0))
                                            .text_color(rgb(TEXT3)),
                                    ),
                            )
                            .child(widgets::button("撤销", move || {
                                let id = id.clone();
                                if let Some(a) = account() {
                                    st().background(move || a.delete_ai_key(&id), move |_| {
                                        st().toast("已撤销");
                                        load();
                                    });
                                }
                            }))
                            .into_element()
                    })
                    .collect::<Vec<_>>()
            }))
            .child(view().flex_row().child(widgets::primary_button("生成新密钥并复制配置", move || {
                copy_for_ai();
                st().popup.set(None);
            }))),
    )
}

pub fn change_password() -> impl IntoElement {
    let (cur, new, again, err) = (signal(String::new()), signal(String::new()), signal(String::new()), signal(String::new()));
    let field = |label: &'static str, v: Signal<String>| {
        view().gap(px(4.0)).child(text(label).text_size(px(12.0)).text_color(rgb(TEXT2))).child(
            nsh_core::element::input()
                .h(px(38.0))
                .px(px(10.0))
                .border_1()
                .border_color(rgb(BORDER))
                .rounded(px(6.0))
                .masked(true)
                .value(move || v.get())
                .on_change(move |t: &str| v.set(t.to_string())),
        )
    };
    let submit = move || {
        let (c, n) = (cur.get_untracked(), new.get_untracked());
        if n.chars().count() < 8 {
            return err.set("新密码至少 8 个字符".into());
        }
        if n != again.get_untracked() {
            return err.set("两次输入的新密码不一致".into());
        }
        let Some(a) = account() else { return };
        err.set("正在修改…".into());
        let (tx, rx) = async_channel::bounded(1);
        std::thread::spawn(move || {
            let _ = tx.send_blocking(a.change_password(&c, &n));
        });
        st().spawn(async move {
            match rx.recv().await {
                Ok(Ok(())) => {
                    st().popup.set(None);
                    st().toast("密码已修改，其他电脑需用新密码重新登录");
                }
                Ok(Err(e)) => err.set(e.to_string()),
                Err(_) => err.set("请求中断".into()),
            }
        });
    };
    modal(
        440.0,
        "修改密码",
        || st().popup.set(None),
        view()
            .p(px(20.0))
            .gap(px(12.0))
            .child(field("当前密码", cur))
            .child(field("新密码（至少 8 个字符）", new))
            .child(field("再次输入新密码", again))
            .child(text(move || err.get()).text_size(px(13.0)).text_color(rgb(DANGER)))
            .child(view().flex_row().child(widgets::primary_button("修改", submit))),
    )
}
