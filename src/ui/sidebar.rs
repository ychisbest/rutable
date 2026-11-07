//! The sidebar: the base's tables and dashboards.

use nsh_core::prelude::*;

use super::{
    icons,
    state::{Popup, st},
    theme::*,
    widgets,
};

thread_local! {
    static RENAME: std::cell::RefCell<Option<(Signal<Option<String>>, Signal<String>)>> = const { std::cell::RefCell::new(None) };
}

/// Starts renaming sidebar item `id` in place.
pub fn rename(id: &str) {
    let name = st().peek(|b| b.table(id).map(|t| t.name.clone()).or_else(|| b.dashboard(id).map(|d| d.name.clone())));
    RENAME.with(|r| {
        if let (Some((who, text)), Some(name)) = (*r.borrow(), name) {
            text.set(name);
            who.set(Some(id.to_string()));
        }
    });
}

fn finish(who: Signal<Option<String>>, text: Signal<String>) {
    let Some(id) = who.get_untracked() else { return };
    let name = text.get_untracked().trim().to_string();
    who.set(None);
    if name.is_empty() {
        return;
    }
    let s = st();
    let taken = s.peek(|b| b.tables.iter().any(|t| t.name == name && t.id != id) || b.dashboards.iter().any(|d| d.name == name && d.id != id));
    if taken {
        s.toast("名称已存在");
        return;
    }
    s.edit(|b| {
        if let Some(t) = b.table_mut(&id) {
            t.name = name.clone();
        } else if let Some(d) = b.dashboard_mut(&id) {
            d.name = name.clone();
        }
    });
}

pub fn sidebar() -> impl IntoElement {
    let s = st();
    let who = signal(None::<String>);
    let text_sig = signal(String::new());
    RENAME.with(|r| *r.borrow_mut() = Some((who, text_sig)));
    let filter = signal(String::new());
    let drag = signal(None::<(String, usize)>);
    let add_at = element_ref();

    view()
        .w(px(SIDEBAR_W))
        .bg(rgb(BG_SIDEBAR))
        .border_r(px(1.0))
        .border_color(rgb(BORDER))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .px(px(12.0))
                .pt(px(12.0))
                .pb(px(8.0))
                .child(
                    view()
                        .flex_1()
                        .h(px(32.0))
                        .rounded(px(6.0))
                        .bg(rgb(BG))
                        .border_1()
                        .border_color(rgb(BORDER))
                        .flex_row()
                        .items_center()
                        .px(px(8.0))
                        .gap(px(6.0))
                        .child(icons::icon("search", 14.0, TEXT3))
                        .child(
                            nsh_core::element::input()
                                .flex_1()
                                .text_size(px(13.0))
                                .text_color(rgb(TEXT))
                                .placeholder("搜索")
                                .placeholder_color(rgb(TEXT3))
                                .value(move || filter.get())
                                .on_change(move |t: &str| filter.set(t.to_string())),
                        ),
                )
                .child(
                    view()
                        .element_ref(&add_at)
                        .size(px(32.0))
                        .rounded(px(6.0))
                        .bg(rgb(PRIMARY))
                        .hover(|s| s.bg(rgb(PRIMARY_HOVER)))
                        .items_center()
                        .justify_center()
                        .on_press(move || widgets::below(add_at, move |x, y| st().popup.set(Some(Popup::SidebarAdd { x, y }))))
                        .child(icons::icon("plus", 16.0, 0xFFFFFF)),
                ),
        )
        .child(
            scroll().flex_1().py(px(4.0)).child(dynamic(move || {
                let q = filter.get().to_lowercase();
                let items: Vec<(String, String, bool, usize)> = s.read(|b| {
                    b.sidebar()
                        .into_iter()
                        .filter_map(|id| {
                            if let Some(t) = b.table(&id) {
                                Some((id, t.name.clone(), false, t.records.len()))
                            } else {
                                b.dashboard(&id).map(|d| (id, d.name.clone(), true, d.charts.len()))
                            }
                        })
                        .filter(|(_, n, _, _)| q.is_empty() || n.to_lowercase().contains(&q))
                        .collect()
                });
                items
                    .into_iter()
                    .enumerate()
                    .map(|(i, (id, name, dash, _n))| item(id, name, dash, i, who, text_sig, drag))
                    .collect::<Vec<_>>()
            })),
        )
        .child(
            view()
                .border_t(px(1.0))
                .border_color(rgb(BORDER))
                .p(px(8.0))
                .gap(px(2.0))
                .child(bottom_item("robot", "自动化流程", move || st().popup.set(Some(Popup::Workflows))))
                .child(bottom_item("keyboard", "快捷键", move || st().popup.set(Some(Popup::Shortcuts)))),
        )
}

fn bottom_item(icon: &'static str, label: &'static str, on: impl Fn() + 'static) -> impl IntoElement {
    view()
        .h(px(34.0))
        .px(px(10.0))
        .rounded(px(6.0))
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .hover(|s| s.bg(rgb(HOVER)))
        .on_press(on)
        .child(icons::icon(icon, 16.0, TEXT2))
        .child(text(label).text_size(px(14.0)).text_color(rgb(TEXT)))
}

fn item(
    id: String,
    name: String,
    dash: bool,
    index: usize,
    who: Signal<Option<String>>,
    text_sig: Signal<String>,
    drag: Signal<Option<(String, usize)>>,
) -> AnyElement {
    let s = st();
    let id2 = id.clone();
    let active_m = computed(move || s.cur.get() == id2);
    let active = move || active_m.get();
    let renaming = {
        let id = id.clone();
        move || who.get().as_deref() == Some(id.as_str())
    };
    let more_at = element_ref();
    let last_press = stored(0i64);
    let id_press = id.clone();
    let id_more = id.clone();
    let id_ctx = id.clone();
    let id_drag = id.clone();
    let hovered = signal(false);
    let drop_here = move || drag.get().is_some_and(|(_, to)| to == index);
    view()
        .h(px(36.0))
        .mx(px(8.0))
        .px(px(10.0))
        .rounded(px(6.0))
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .bg(move || if active() { rgb(ACTIVE) } else { rgba(0xFFFFFF00) })
        .hover(|s| s.bg(rgb(HOVER)))
        .border_t(move || px(if drop_here() { 2.0 } else { 0.0 }))
        .border_color(rgb(PRIMARY))
        .on_hover_in(move || hovered.set(true))
        .on_hover_out(move || hovered.set(false))
        .on_press(move || {
            let now = crate::model::now_ms();
            let dbl = now - last_press.with(|t| std::mem::replace(t, now)) < 450;
            if dbl {
                rename(&id_press);
            } else if st().cur.get_untracked() != id_press {
                st().open(&id_press);
            }
        })
        .on_context_menu(move |e: &ContextMenuEvent| {
            st().popup.set(Some(Popup::SidebarMenu { item: id_ctx.clone(), x: e.page_position.x.0, y: e.page_position.y.0 }))
        })
        .claim_touches(TouchClaim::on_move())
        .on_responder_grant(move |_: &ResponderEvent| drag.set(Some((id_drag.clone(), index))))
        .on_responder_move(move |e: &ResponderEvent| {
            let to = (index as f32 + (e.gesture.delta.y.0 / 36.0).round()).max(0.0) as usize;
            if let Some((d, _)) = drag.get_untracked() {
                drag.set(Some((d, to)));
            }
        })
        .on_responder_release(move |_: &ResponderEvent| {
            if let Some((moving, to)) = drag.get_untracked() {
                drag.set(None);
                let s = st();
                s.edit(|b| {
                    let mut order = b.sidebar();
                    if let Some(from) = order.iter().position(|x| *x == moving) {
                        let item = order.remove(from);
                        order.insert(to.min(order.len()), item);
                        b.order = order;
                    }
                });
            }
        })
        .on_responder_terminate(move |_: &ResponderEvent| drag.set(None))
        .child(icons::icon(if dash { "dashboard" } else { "table" }, 16.0, if dash { 0xFF8800 } else { PRIMARY }))
        .child(dynamic(move || {
            if renaming() {
                nsh_core::element::input()
                    .flex_1()
                    .h(px(28.0))
                    .px(px(6.0))
                    .rounded(px(4.0))
                    .bg(rgb(BG))
                    .border_1()
                    .border_color(rgb(PRIMARY))
                    .text_size(px(14.0))
                    .auto_focus(true)
                    .value(move || text_sig.get())
                    .on_change(move |t: &str| text_sig.set(t.to_string()))
                    .on_submit(move |_: &str| finish(who, text_sig))
                    .on_blur(move || finish(who, text_sig))
                    .into_element()
            } else {
                text(name.clone())
                    .flex_1()
                    .text_size(px(14.0))
                    .text_color(move || if active() { rgb(PRIMARY) } else { rgb(TEXT) })
                    .number_of_lines(1u32)
                    .into_element()
            }
        }))
        .child(
            view()
                .element_ref(&more_at)
                .size(px(22.0))
                .rounded(px(4.0))
                .items_center()
                .justify_center()
                .opacity(move || if hovered.get() || active() { 1.0 } else { 0.0 })
                .hover(|s| s.bg(rgb(0xDEE0E3)))
                .on_press(move || {
                    let id = id_more.clone();
                    widgets::below(more_at, move |x, y| st().popup.set(Some(Popup::SidebarMenu { item: id.clone(), x, y })))
                })
                .child(icons::icon("more", 16.0, TEXT2)),
        )
        .into_element()
}
