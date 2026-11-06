//! The top bar (the base's name, undo, share), the view tabs and the
//! toolbar of the open view.

use nsh_core::prelude::*;

use super::{
    grid, icons,
    state::{Popup, st},
    theme::*,
    widgets,
};
use crate::model::ViewKind;

fn icon_button(icon: &'static str, enabled: impl Fn() -> bool + Copy + 'static, on: impl Fn() + 'static) -> impl IntoElement {
    view()
        .size(px(32.0))
        .rounded(px(6.0))
        .items_center()
        .justify_center()
        .hover(|s| s.bg(rgb(HOVER)))
        .on_press(on)
        .child(icons::icon_dyn(move || icon, 18.0, move || if enabled() { TEXT2 } else { 0xC9CDD4 }))
}

pub fn top_bar() -> impl IntoElement {
    let s = st();
    let editing = signal(false);
    let name = signal(String::new());
    let finish = move || {
        if !editing.get_untracked() {
            return;
        }
        editing.set(false);
        let n = name.get_untracked().trim().to_string();
        if !n.is_empty() && n != s.peek(|b| b.name.clone()) {
            s.edit(|b| b.name = n);
        }
    };
    view()
        .h(px(TOPBAR_H))
        .flex_row()
        .items_center()
        .px(px(12.0))
        .gap(px(8.0))
        .border_b(px(1.0))
        .border_color(rgb(BORDER))
        .bg(rgb(BG))
        .child(icon_button("sidebar", || true, move || s.sidebar.update(|o| *o = !*o)))
        .child(
            view()
                .size(px(28.0))
                .rounded(px(6.0))
                .bg(rgb(PRIMARY))
                .items_center()
                .justify_center()
                .child(icons::icon("table", 16.0, 0xFFFFFF)),
        )
        .child(dynamic(move || {
            if editing.get() {
                nsh_core::element::input()
                    .w(px(320.0))
                    .h(px(32.0))
                    .px(px(8.0))
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(rgb(PRIMARY))
                    .text_size(px(16.0))
                    .auto_focus(true)
                    .value(move || name.get())
                    .on_change(move |t: &str| name.set(t.to_string()))
                    .on_submit(move |_: &str| finish())
                    .on_blur(finish)
                    .into_element()
            } else {
                view()
                    .px(px(6.0))
                    .py(px(4.0))
                    .rounded(px(6.0))
                    .hover(|s| s.bg(rgb(HOVER)))
                    .on_press(move || {
                        name.set(st().peek(|b| b.name.clone()));
                        editing.set(true);
                    })
                    .child(
                        text(move || s.read(|b| b.name.clone()))
                            .text_size(px(16.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(TEXT)),
                    )
                    .into_element()
            }
        }))
        .child(icons::icon("star", 16.0, TEXT3))
        .child(view().flex_1())
        .child(
            text(move || {
                let _ = s.rev.get();
                "已保存到本地".to_string()
            })
            .text_size(px(12.0))
            .text_color(rgb(TEXT3)),
        )
        .child(icon_button("undo", move || s.can_undo(), move || st().undo()))
        .child(icon_button("redo", move || s.can_redo(), move || st().redo()))
        .child(
            view()
                .child(icon_button("bell", || true, move || {
                    let n = st().notices.get_untracked();
                    if n.is_empty() {
                        st().toast("暂无通知");
                    } else {
                        st().toast(&n[0]);
                    }
                }))
                .child(when(move || !s.notices.get().is_empty(), move || {
                    view()
                        .absolute()
                        .top(px(2.0))
                        .right(px(2.0))
                        .min_w(px(16.0))
                        .h(px(16.0))
                        .px(px(4.0))
                        .rounded(px(8.0))
                        .bg(rgb(DANGER))
                        .items_center()
                        .justify_center()
                        .pointer_events(PointerEvents::None)
                        .child(text(move || s.notices.get().len().to_string()).text_size(px(10.0)).text_color(rgb(0xFFFFFF)))
                })),
        )
        .child(widgets::primary_button("分享", move || {
            st().toast("已复制多维表格链接（本地文档）");
            set_clipboard(super::state::data_path().to_string_lossy().to_string());
        }))
        .child(widgets::avatar_el("我", 0, 30.0))
}

pub fn view_tabs() -> impl IntoElement {
    let s = st();
    let renaming = signal(None::<String>);
    let name = signal(String::new());
    let finish = move || {
        let Some(id) = renaming.get_untracked() else { return };
        renaming.set(None);
        let n = name.get_untracked().trim().to_string();
        if !n.is_empty() {
            let cur = s.cur.get_untracked();
            s.edit(|b| {
                if let Some(v) = b.table_mut(&cur).and_then(|t| t.view_mut(&id)) {
                    v.name = n;
                }
            });
        }
    };
    let add_at = element_ref();
    view()
        .h(px(TABS_H))
        .flex_row()
        .items_end()
        .px(px(12.0))
        .gap(px(2.0))
        .border_b(px(1.0))
        .border_color(rgb(BORDER))
        .bg(rgb(BG))
        .child(dynamic(move || {
            let views: Vec<(String, String, ViewKind)> =
                s.with_table(|_, t| t.views.iter().map(|v| (v.id.clone(), v.name.clone(), v.kind)).collect()).unwrap_or_default();
            views
                .into_iter()
                .map(|(id, vname, kind)| {
                    let id2 = id.clone();
                    let id3 = id.clone();
                    let id4 = id.clone();
                    let active_m = computed(move || s.view.get() == id2);
                    let active = move || active_m.get();
                    let is_renaming = {
                        let id = id.clone();
                        move || renaming.get().as_deref() == Some(id.as_str())
                    };
                    let last = stored(0i64);
                    let more_at = element_ref();
                    view()
                        .h(px(36.0))
                        .px(px(10.0))
                        .flex_row()
                        .items_center()
                        .gap(px(6.0))
                        .rounded_t(px(6.0))
                        .bg(move || if active() { rgb(BG) } else { rgba(0xFFFFFF00) })
                        .hover(|s| s.bg(rgb(HOVER)))
                        .border_b(move || px(if active() { 2.0 } else { 0.0 }))
                        .border_color(rgb(PRIMARY))
                        .on_press(move || {
                            let now = crate::model::now_ms();
                            let dbl = now - last.with(|t| std::mem::replace(t, now)) < 450;
                            if dbl {
                                name.set(st().with_table(|_, t| t.view(&id3).map(|v| v.name.clone())).flatten().unwrap_or_default());
                                renaming.set(Some(id3.clone()));
                            } else if st().view.get_untracked() != id3 {
                                st().view.set(id3.clone());
                                st().checked.set(vec![]);
                            }
                        })
                        .on_context_menu(move |e: &ContextMenuEvent| {
                            st().popup.set(Some(Popup::ViewMenu { view: id4.clone(), x: e.page_position.x.0, y: e.page_position.y.0 }))
                        })
                        .child(icons::icon_dyn(move || icons::view_icon(kind), 16.0, move || if active() { PRIMARY } else { TEXT2 }))
                        .child(dynamic(move || {
                            if is_renaming() {
                                nsh_core::element::input()
                                    .w(px(120.0))
                                    .h(px(26.0))
                                    .px(px(6.0))
                                    .rounded(px(4.0))
                                    .border_1()
                                    .border_color(rgb(PRIMARY))
                                    .text_size(px(14.0))
                                    .auto_focus(true)
                                    .value(move || name.get())
                                    .on_change(move |t: &str| name.set(t.to_string()))
                                    .on_submit(move |_: &str| finish())
                                    .on_blur(finish)
                                    .into_element()
                            } else {
                                text(vname.clone())
                                    .text_size(px(14.0))
                                    .text_color(move || if active() { rgb(PRIMARY) } else { rgb(TEXT) })
                                    .font_weight(FontWeight::NORMAL)
                                    .into_element()
                            }
                        }))
                        .child(when(active, move || {
                            let id = id.clone();
                            view()
                                .element_ref(&more_at)
                                .size(px(20.0))
                                .rounded(px(4.0))
                                .items_center()
                                .justify_center()
                                .hover(|s| s.bg(rgb(0xDEE0E3)))
                                .on_press(move || {
                                    let id = id.clone();
                                    widgets::below(more_at, move |x, y| st().popup.set(Some(Popup::ViewMenu { view: id.clone(), x, y })))
                                })
                                .child(icons::icon("chevron-down", 14.0, TEXT2))
                        }))
                        .into_element()
                })
                .collect::<Vec<_>>()
        }))
        .child(
            view()
                .element_ref(&add_at)
                .h(px(32.0))
                .mb(px(2.0))
                .px(px(8.0))
                .rounded(px(6.0))
                .flex_row()
                .items_center()
                .gap(px(4.0))
                .hover(|s| s.bg(rgb(HOVER)))
                .on_press(move || widgets::below(add_at, move |x, y| st().popup.set(Some(Popup::AddView { x, y }))))
                .child(icons::icon("plus", 16.0, TEXT2))
                .child(text("新建视图").text_size(px(14.0)).text_color(rgb(TEXT2))),
        )
}

pub fn toolbar(kind: ViewKind) -> impl IntoElement {
    let s = st();
    let count = move |f: fn(&crate::model::View) -> usize| move || s.with_view(|_, _, v| f(v)).unwrap_or(0);
    let filters = count(|v| v.filter.conditions.len());
    let sorts = count(|v| v.sorts.len());
    let groups = count(|v| v.groups.len());
    let hidden = count(|v| v.hidden.len());
    let label = |base: &'static str, n: usize| if n == 0 { base.to_string() } else { format!("{n} 个{}", &base[base.len() - 6..]) };
    let _ = label;
    let search_text = s.search;
    view()
        .h(px(TOOLBAR_H))
        .flex_row()
        .items_center()
        .px(px(12.0))
        .gap(px(4.0))
        .border_b(px(1.0))
        .border_color(rgb(BORDER))
        .bg(rgb(BG))
        .child(
            view()
                .h(px(30.0))
                .px(px(10.0))
                .rounded(px(6.0))
                .bg(rgb(PRIMARY))
                .hover(|s| s.bg(rgb(PRIMARY_HOVER)))
                .flex_row()
                .items_center()
                .gap(px(4.0))
                .on_press(move || add_record_button(kind))
                .child(icons::icon("plus", 16.0, 0xFFFFFF))
                .child(text("添加记录").text_size(px(14.0)).text_color(rgb(0xFFFFFF))),
        )
        .child(view().w(px(1.0)).h(px(16.0)).mx(px(6.0)).bg(rgb(BORDER)))
        .child(widgets::tool_button(
            "fields",
            move || if hidden() > 0 { format!("{} 个隐藏字段", hidden()) } else { "字段配置".into() },
            move || hidden() > 0,
            move |x, y| st().popup.set(Some(Popup::Fields { x, y })),
        ))
        .child(widgets::tool_button(
            "filter",
            move || if filters() > 0 { format!("{} 个筛选条件", filters()) } else { "筛选".into() },
            move || filters() > 0,
            move |x, y| st().popup.set(Some(Popup::Filter { x, y })),
        ))
        .child(when(move || kind == ViewKind::Grid, move || {
            widgets::tool_button(
                "group",
                move || if groups() > 0 { format!("{} 个分组条件", groups()) } else { "分组".into() },
                move || groups() > 0,
                move |x, y| st().popup.set(Some(Popup::Group { x, y })),
            )
        }))
        .child(widgets::tool_button(
            "sort",
            move || if sorts() > 0 { format!("{} 个排序条件", sorts()) } else { "排序".into() },
            move || sorts() > 0,
            move |x, y| st().popup.set(Some(Popup::Sort { x, y })),
        ))
        .child(when(move || kind == ViewKind::Grid, move || {
            widgets::tool_button("rowheight", || "行高".into(), || false, move |x, y| st().popup.set(Some(Popup::RowHeight { x, y })))
        }))
        .child(when(move || kind == ViewKind::Grid, move || {
            let n = move || s.with_view(|_, _, v| v.row_colors.len()).unwrap_or(0);
            widgets::tool_button("palette", move || if n() > 0 { format!("{} 个填色条件", n()) } else { "填色".into() }, move || n() > 0, move |x, y| {
                st().popup.set(Some(Popup::Colors { x, y }))
            })
        }))
        .child(widgets::tool_button("more", || "".into(), || false, move |x, y| st().popup.set(Some(Popup::More { x, y }))))
        .child(view().flex_1())
        .child(dynamic(move || {
            if s.search_open.get() {
                view()
                    .w(px(240.0))
                    .h(px(30.0))
                    .px(px(8.0))
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(rgb(PRIMARY))
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .child(icons::icon("search", 14.0, TEXT3))
                    .child(
                        nsh_core::element::input()
                            .flex_1()
                            .text_size(px(13.0))
                            .placeholder("搜索记录")
                            .placeholder_color(rgb(TEXT3))
                            .auto_focus(true)
                            .value(move || search_text.get())
                            .on_change(move |t: &str| search_text.set(t.to_string())),
                    )
                    .child(
                        view()
                            .on_press(move || {
                                search_text.set(String::new());
                                st().search_open.set(false);
                            })
                            .child(icons::icon("x", 14.0, TEXT3)),
                    )
                    .into_element()
            } else {
                view()
                    .size(px(30.0))
                    .rounded(px(6.0))
                    .items_center()
                    .justify_center()
                    .hover(|s| s.bg(rgb(HOVER)))
                    .on_press(move || st().search_open.set(true))
                    .child(icons::icon("search", 16.0, TEXT2))
                    .into_element()
            }
        }))
}

/// "添加记录": a new record at the end; the grid selects it, other views
/// open it in the record card.
pub fn add_record_button(kind: ViewKind) {
    let s = st();
    if kind == ViewKind::Grid {
        if let Some(g) = grid::grid() {
            if let Some(id) = grid::edit::add_record(g, vec![], None) {
                let _ = id;
                g.input.focus();
            }
            return;
        }
    }
    let table = s.cur.get_untracked();
    let id = s.edit(|b| {
        let me = b.me.clone();
        let t = b.table_mut(&table)?;
        let r = t.new_record(&me);
        let id = r.id.clone();
        t.records.push(r);
        crate::ui::automations::on_add(b, &table, &id);
        Some(id)
    });
    if let Some(id) = id {
        s.expanded.set(Some((table, id)));
    }
}
