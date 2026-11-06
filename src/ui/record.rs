//! The record card (展开记录): every field of a record with its editor,
//! the previous / next record, comments and the change history.

use std::rc::Rc;

use nsh_core::prelude::*;

use super::{
    cells, grid, icons, popups,
    state::st,
    theme::*,
    widgets::{self, avatar_el, tag_el},
};
use crate::model::{
    Comment, FieldKind, Id, SelectOption, Value, new_id, now_ms, query,
    value::{self, tag_color},
};

/// Reads and writes one value (a record's cell, a form's draft).
#[derive(Clone)]
pub struct Slot {
    pub get: Rc<dyn Fn() -> Option<Value>>,
    pub set: Rc<dyn Fn(Option<Value>)>,
    /// Text is written as it is typed (a form's draft), not on blur.
    pub live: bool,
}

pub fn record_slot(table: Id, record: Id, field: Id) -> Slot {
    let (t1, r1, f1) = (table.clone(), record.clone(), field.clone());
    Slot {
        get: Rc::new(move || {
            st().read(|b| {
                let t = b.table(&t1)?;
                let r = t.record(&r1)?;
                let f = t.field(&f1)?;
                query::cell(b, t, r, f)
            })
        }),
        set: Rc::new(move |v| st().set_cells(&table, &[(record.clone(), field.clone(), v)])),
        live: false,
    }
}

pub fn record_card() -> Option<AnyElement> {
    let (table, record) = st().expanded.get()?;
    untracked(move || card_body(table, record))
}

fn card_body(table: Id, record: Id) -> Option<AnyElement> {
    let s = st();
    let exists = s.read(|b| b.table(&table).and_then(|t| t.record(&record)).is_some());
    if !exists {
        return None;
    }
    let tab = signal(0usize);
    let comment = signal(String::new());
    let close = move || st().expanded.set(None);
    // The records of the view around this one (for previous / next).
    let neighbors = {
        let record = record.clone();
        move || -> (Option<Id>, Option<Id>, usize, usize) {
            let ids: Vec<Id> = match grid::grid() {
                Some(g) if st().view_kind() == Some(crate::model::ViewKind::Grid) => g.layout.get_untracked().record_ids,
                _ => st().with_view(|b, t, v| query::records(b, t, v).iter().map(|&i| t.records[i].id.clone()).collect()).unwrap_or_default(),
            };
            let p = ids.iter().position(|x| *x == record);
            match p {
                Some(p) => (p.checked_sub(1).map(|i| ids[i].clone()), ids.get(p + 1).cloned(), p + 1, ids.len()),
                None => (None, None, 0, ids.len()),
            }
        }
    };
    let (prev, next, pos, total) = untracked(neighbors);
    let t2 = table.clone();
    let r2 = record.clone();
    let title = move || st().read(|b| b.table(&t2).map(|t| value::record_title(b, t, &r2)).unwrap_or_default());
    let fields: Vec<(Id, String, FieldKind, String)> = s.with_view(|_, t, v| {
        t.view_fields(v).into_iter().map(|(f, _)| (f.id.clone(), f.name.clone(), f.kind.clone(), f.description.clone())).collect()
    })
    .or_else(|| s.read(|b| b.table(&table).map(|t| t.fields.iter().map(|f| (f.id.clone(), f.name.clone(), f.kind.clone(), f.description.clone())).collect())))
    .unwrap_or_default();
    let hidden: Vec<Id> = s.with_view(|_, _, v| v.hidden.clone()).unwrap_or_default();
    let show_hidden = signal(false);
    let (t3, r3) = (table.clone(), record.clone());
    let (t4, r4) = (table.clone(), record.clone());
    let r5 = record.clone();
    let nav = move |to: Option<Id>| {
        if let Some(r) = to {
            st().expanded.set(Some((st().cur.get_untracked(), r)));
        }
    };
    let n_hidden = hidden.len();
    Some(
        overlay()
            .inset(px(0.0))
            .bg(rgba(0x1F232966))
            .items_center()
            .justify_center()
            .on_cancel(close)
            .child(view().absolute().inset(px(0.0)).focusable(false).on_press(close))
            .child(
                view()
                    .w(px(1000.0))
                    .h(px(720.0))
                    .bg(rgb(BG))
                    .rounded(px(10.0))
                    .shadow_xl()
                    .overflow_hidden()
                    .child(
                        view()
                            .h(px(56.0))
                            .px(px(20.0))
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .border_b(px(1.0))
                            .border_color(rgb(BORDER))
                            .child(text(title).flex_1().text_size(px(18.0)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT)).number_of_lines(1u32))
                            .child(text(format!("{pos} / {total}")).text_size(px(13.0)).text_color(rgb(TEXT3)))
                            .child(nav_button("chevron-up", prev.is_some(), {
                                let prev = prev.clone();
                                move || nav(prev.clone())
                            }))
                            .child(nav_button("chevron-down", next.is_some(), {
                                let next = next.clone();
                                move || nav(next.clone())
                            }))
                            .child(nav_button("copy", true, move || {
                                popups::duplicate_records(&[r4.clone()]);
                                let _ = &t4;
                            }))
                            .child(nav_button("trash", true, move || popups::delete_records(&[r5.clone()])))
                            .child(nav_button("x", true, close)),
                    )
                    .child(
                        view()
                            .flex_1()
                            .flex_row()
                            .child(
                                scroll().flex_1().px(px(28.0)).py(px(16.0)).child(
                                    view()
                                        .gap(px(4.0))
                                        .child(
                                            fields
                                                .iter()
                                                .filter(|f| !hidden.contains(&f.0))
                                                .map(|(fid, name, kind, desc)| field_row(&table, &record, fid, name, kind, desc))
                                                .collect::<Vec<_>>(),
                                        )
                                        .child(when(move || n_hidden > 0, move || {
                                            widgets::link_button(if show_hidden.get() { "收起隐藏字段" } else { "显示隐藏字段" }, move || show_hidden.update(|x| *x = !*x))
                                        }))
                                        .child({
                                            let (table, record) = (table.clone(), record.clone());
                                            let fields = fields.clone();
                                            let hidden = hidden.clone();
                                            dynamic(move || {
                                                if !show_hidden.get() {
                                                    return vec![];
                                                }
                                                fields
                                                    .iter()
                                                    .filter(|f| hidden.contains(&f.0))
                                                    .map(|(fid, name, kind, desc)| field_row(&table, &record, fid, name, kind, desc))
                                                    .collect::<Vec<_>>()
                                            })
                                        }),
                                ),
                            )
                            .child(
                                view()
                                    .w(px(320.0))
                                    .border_l(px(1.0))
                                    .border_color(rgb(BORDER))
                                    .bg(rgb(BG_SIDEBAR))
                                    .child(
                                        view()
                                            .h(px(44.0))
                                            .flex_row()
                                            .items_end()
                                            .px(px(16.0))
                                            .gap(px(16.0))
                                            .border_b(px(1.0))
                                            .border_color(rgb(BORDER))
                                            .child(side_tab("评论", 0, tab))
                                            .child(side_tab("修改记录", 1, tab)),
                                    )
                                    .child(dynamic(move || {
                                        if tab.get() == 0 {
                                            comments(t3.clone(), r3.clone(), comment).into_element()
                                        } else {
                                            history(t3.clone(), r3.clone()).into_element()
                                        }
                                    })),
                            ),
                    ),
            )
            .into_element(),
    )
}

fn side_tab(label: &'static str, i: usize, tab: Signal<usize>) -> impl IntoElement {
    view()
        .h(px(44.0))
        .justify_center()
        .border_b(move || px(if tab.get() == i { 2.0 } else { 0.0 }))
        .border_color(rgb(PRIMARY))
        .on_press(move || tab.set(i))
        .child(text(label).text_size(px(14.0)).text_color(move || if tab.get() == i { rgb(PRIMARY) } else { rgb(TEXT2) }))
}

fn nav_button(icon: &'static str, enabled: bool, on: impl Fn() + 'static) -> impl IntoElement {
    view()
        .size(px(32.0))
        .rounded(px(6.0))
        .items_center()
        .justify_center()
        .hover(|s| s.bg(rgb(HOVER)))
        .on_press(move || {
            if enabled {
                on()
            }
        })
        .child(icons::icon(icon, 18.0, if enabled { TEXT2 } else { 0xC9CDD4 }))
}

fn field_row(table: &str, record: &str, fid: &str, name: &str, kind: &FieldKind, desc: &str) -> AnyElement {
    let slot = record_slot(table.to_string(), record.to_string(), fid.to_string());
    view()
        .flex_row()
        .gap(px(12.0))
        .py(px(6.0))
        .child(
            view()
                .w(px(160.0))
                .pt(px(8.0))
                .flex_row()
                .gap(px(6.0))
                .child(icons::icon(icons::field_icon(kind), 14.0, TEXT3))
                .child(
                    view()
                        .flex_1()
                        .child(text(name.to_string()).text_size(px(13.0)).text_color(rgb(TEXT2)).number_of_lines(2u32))
                        .child(if desc.is_empty() { view().into_element() } else { text(desc.to_string()).text_size(px(11.0)).text_color(rgb(TEXT3)).into_element() }),
                ),
        )
        .child(view().flex_1().min_w(px(0.0)).child(value_input(table.to_string(), fid.to_string(), kind.clone(), slot)))
        .into_element()
}

/// The editor of a value of a field of `kind`.
pub fn value_input(table: Id, field: Id, kind: FieldKind, slot: Slot) -> AnyElement {
    let s = st();
    let get = slot.get.clone();
    let set = slot.set.clone();
    let box_style = |v: View| v.min_h(px(36.0)).px(px(10.0)).rounded(px(6.0)).border_1().border_color(rgb(BORDER)).bg(rgb(BG));
    if let FieldKind::Button { label, color, .. } = &kind {
        let (t, f) = (table.clone(), field.clone());
        return view()
            .flex_row()
            .child(
                view()
                    .h(px(32.0))
                    .px(px(14.0))
                    .rounded(px(6.0))
                    .bg(rgb(crate::model::value::strong_color(*color)))
                    .hover(|s| s.opacity(0.85))
                    .justify_center()
                    .on_press(move || {
                        if let Some((_, r)) = st().expanded.get_untracked() {
                            crate::ui::automations::press_button(&t, &f, &r);
                        }
                    })
                    .child(text(label.clone()).text_size(px(14.0)).text_color(rgb(0xFFFFFF))),
            )
            .into_element();
    }
    if kind.is_computed() {
        let k = kind.clone();
        let t = table.clone();
        return box_style(view())
            .bg(rgb(BG_GREY))
            .justify_center()
            .child(
                text(move || {
                    let v = get();
                    st().read(|b| b.table(&t).map(|tt| v.map(|v| value::display(b, tt, &k, &v)).unwrap_or_default()).unwrap_or_default())
                })
                .text_size(px(14.0))
                .text_color(rgb(TEXT2)),
            )
            .into_element();
    }
    match kind.clone() {
        FieldKind::Checkbox => view()
            .h(px(36.0))
            .justify_center()
            .child(view().on_press(move || {
                let on = matches!(get(), Some(Value::Bool(true)));
                set(Some(Value::Bool(!on)));
            }).child(widgets::check_box_dyn({
                let g = slot.get.clone();
                move || matches!(g(), Some(Value::Bool(true)))
            })))
            .into_element(),
        FieldKind::Rating { max, .. } => view()
            .h(px(36.0))
            .flex_row()
            .items_center()
            .gap(px(4.0))
            .child(
                (1..=max)
                    .map(|i| {
                        let get = get.clone();
                        let set = set.clone();
                        let g2 = get.clone();
                        view()
                            .on_press(move || {
                                let cur = get().and_then(|v| v.as_number()).unwrap_or(0.0) as u8;
                                set(if cur == i { None } else { Some(Value::Number(i as f64)) })
                            })
                            .child(canvas(move || {
                                let n = g2().and_then(|v| v.as_number()).unwrap_or(0.0) as u8;
                                cells::rating(Painter::new(), 0.0, 10.0, if n >= i { 1 } else { 0 }, 1, None)
                            }).size(px(20.0)))
                            .into_element()
                    })
                    .collect::<Vec<_>>(),
            )
            .into_element(),
        FieldKind::SingleSelect { .. } | FieldKind::MultiSelect { .. } | FieldKind::User { .. } | FieldKind::Link { .. } => picker(table, field, kind, slot),
        FieldKind::Date { time, .. } => {
            let open = signal(false);
            let at = element_ref();
            let k = kind.clone();
            let g2 = get.clone();
            view()
                .child(
                    box_style(view().element_ref(&at))
                        .flex_row()
                        .items_center()
                        .gap(px(8.0))
                        .on_press(move || open.set(true))
                        .child(icons::icon("date", 16.0, TEXT3))
                        .child(
                            text(move || match g2() {
                                Some(Value::Date(ms)) => match &k {
                                    FieldKind::Date { format, time } => value::format_date(ms, *format, *time),
                                    _ => String::new(),
                                },
                                _ => "选择日期".into(),
                            })
                            .text_size(px(14.0))
                            .text_color(rgb(TEXT)),
                        ),
                )
                .child(when(move || open.get(), move || {
                    let set = set.clone();
                    let set2 = set.clone();
                    let cur = match get() {
                        Some(Value::Date(d)) => Some(d),
                        _ => None,
                    };
                    overlay()
                        .anchor(&at)
                        .placement(Placement::Bottom)
                        .align(Align::Start)
                        .offset(px(4.0))
                        .margin(px(8.0))
                        .on_press_outside(move |_: &Point<Points>| open.set(false))
                        .on_cancel(move || open.set(false))
                        .w(px(280.0))
                        .bg(rgb(BG))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(rgb(BORDER))
                        .shadow_lg()
                        .child(widgets::calendar(cur.unwrap_or_else(value::today_ms), cur, move |day| {
                            let t = if time { cur.map(|c| c - value::day_start(c)).unwrap_or(9 * 3_600_000) } else { 0 };
                            set(Some(Value::Date(day + t)));
                            open.set(false);
                        }))
                        .child(view().flex_row().justify_between().px(px(12.0)).py(px(8.0)).child(widgets::link_button("今天", move || {
                            set2(Some(Value::Date(value::today_ms())));
                            open.set(false);
                        })).child(widgets::link_button("清除", {
                            let set = slot.set.clone();
                            move || {
                                set(None);
                                open.set(false);
                            }
                        })))
                }))
                .into_element()
        }
        FieldKind::Attachment => {
            let g2 = get.clone();
            let t = table.clone();
            let f = field.clone();
            view()
                .gap(px(6.0))
                .child(dynamic(move || {
                    let files = match g2() {
                        Some(Value::Files(f)) => f,
                        _ => vec![],
                    };
                    let set = set.clone();
                    view()
                        .flex_row()
                        .flex_wrap()
                        .gap(px(8.0))
                        .child(
                            files
                                .iter()
                                .enumerate()
                                .map(|(i, a)| {
                                    let a2 = a.clone();
                                    let path = a.path.clone();
                                    let files = files.clone();
                                    let set = set.clone();
                                    view()
                                        .w(px(96.0))
                                        .gap(px(4.0))
                                        .child(
                                            view()
                                                .size(px(96.0))
                                                .rounded(px(6.0))
                                                .border_1()
                                                .border_color(rgb(BORDER))
                                                .overflow_hidden()
                                                .on_press(move || widgets::reveal_file(&path))
                                                .child(canvas(move || cells::file_thumb(Painter::new(), &a2, 0.0, 0.0, 96.0)).size(px(96.0)))
                                                .child(
                                                    view()
                                                        .absolute()
                                                        .top(px(4.0))
                                                        .right(px(4.0))
                                                        .size(px(20.0))
                                                        .rounded(px(10.0))
                                                        .bg(rgba(0x1F232999))
                                                        .items_center()
                                                        .justify_center()
                                                        .on_press(move || {
                                                            let mut f = files.clone();
                                                            f.remove(i);
                                                            set(if f.is_empty() { None } else { Some(Value::Files(f)) })
                                                        })
                                                        .child(icons::icon("x", 12.0, 0xFFFFFF)),
                                                ),
                                        )
                                        .child(text(a.name.clone()).text_size(px(12.0)).text_color(rgb(TEXT2)).number_of_lines(1u32))
                                        .child(text(value::human_size(a.size)).text_size(px(11.0)).text_color(rgb(TEXT3)))
                                        .into_element()
                                })
                                .collect::<Vec<_>>(),
                        )
                        .into_element()
                }))
                .child(view().flex_row().child(widgets::button("上传附件", move || {
                    let (t, f) = (t.clone(), f.clone());
                    let rec = st().expanded.get_untracked().map(|x| x.1);
                    spawn(async move {
                        let paths = widgets::open_files_dialog().await;
                        if let (false, Some(r)) = (paths.is_empty(), rec) {
                            grid::edit::attach(&t, &r, &f, paths);
                        }
                    });
                })))
                .into_element()
        }
        _ => {
            // Text-like values: an input saved when it loses focus or Enter.
            let initial = grid::edit::edit_text_of(&kind, get().as_ref());
            let draft = signal(initial);
            let k2 = kind.clone();
            let save = {
                let set = set.clone();
                let get = get.clone();
                move || {
                    let txt = draft.get_untracked();
                    let mut k = k2.clone();
                    let v = st().peek(|b| value::parse(b, &mut k, &txt));
                    if !txt.trim().is_empty() && v.is_none() && k.is_numeric() {
                        st().toast("请输入数字");
                        return;
                    }
                    if v != get() {
                        set(v);
                    }
                }
            };
            let multiline = matches!(kind, FieldKind::Text);
            let g3 = get.clone();
            let k3 = kind.clone();
            let live = slot.live;
            // Follows changes made elsewhere (undo, the grid; a form's reset).
            effect(move || {
                let v = g3();
                if live && v.is_some() {
                    return;
                }
                let t = grid::edit::edit_text_of(&k3, v.as_ref());
                if untracked(|| draft.get()) != t {
                    draft.set(t);
                }
            });
            let set_live = set.clone();
            let k4 = kind.clone();
            let save2 = save.clone();
            let _ = s;
            let is_url = matches!(kind, FieldKind::Url);
            let input = nsh_core::element::input()
                .min_h(px(36.0))
                .px(px(10.0))
                .py(px(8.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(rgb(BORDER))
                .text_size(px(14.0))
                .text_color(rgb(if matches!(kind, FieldKind::Url | FieldKind::Email | FieldKind::Phone) { PRIMARY } else { TEXT }))
                .multiline(multiline)
                .placeholder(match kind {
                    FieldKind::Number { .. } | FieldKind::Currency { .. } => "请输入数字",
                    FieldKind::Progress { .. } => "请输入 0-100",
                    FieldKind::Url => "请输入链接",
                    FieldKind::Email => "请输入邮箱",
                    FieldKind::Phone => "请输入电话号码",
                    _ => "请输入",
                })
                .placeholder_color(rgb(0xC9CDD4))
                .value(move || draft.get())
                .on_change(move |t: &str| {
                    draft.set(t.to_string());
                    if live {
                        let mut k = k4.clone();
                        let v = st().peek(|b| value::parse(b, &mut k, t));
                        set_live(v);
                    }
                })
                .on_blur(move || {
                    if !live {
                        save()
                    }
                })
                .on_submit(move |_: &str| save2());
            if is_url {
                view().flex_row().items_center().gap(px(8.0))
                    .child(input.flex_1())
                    .child(widgets::link_button("打开链接", move || {
                        let link = draft.get_untracked();
                        if !link.trim().is_empty() {
                            widgets::shell_open(link.trim());
                        }
                    }))
                    .into_element()
            } else {
                input.into_element()
            }
        }
    }
}

/// Tags of the chosen options / people / records, and a list to change them.
fn picker(table: Id, field: Id, kind: FieldKind, slot: Slot) -> AnyElement {
    let open = signal(false);
    let q = signal(String::new());
    let at = element_ref();
    let get = slot.get.clone();
    let set = slot.set.clone();
    let k_show = kind.clone();
    let t_show = table.clone();
    let f_items = field.clone();
    let multiple = match &kind {
        FieldKind::SingleSelect { .. } => false,
        FieldKind::MultiSelect { .. } => true,
        FieldKind::User { multiple } => *multiple,
        FieldKind::Link { multiple, .. } => *multiple,
        _ => false,
    };
    // (id, label, color) of every choice.
    let items = {
        let table = table.clone();
        let kind = kind.clone();
        move || -> Vec<(Id, String, u8)> {
            st().read(|b| {
                let t = b.table(&table)?;
                let k = t.field(&f_items).map(|f| f.kind.clone()).unwrap_or(kind.clone());
                Some(match &k {
                    FieldKind::SingleSelect { options } | FieldKind::MultiSelect { options } => options.iter().map(|o| (o.id.clone(), o.name.clone(), o.color)).collect(),
                    FieldKind::User { .. } => b.members.iter().map(|m| (m.id.clone(), m.name.clone(), m.color)).collect(),
                    FieldKind::Link { table: ot, .. } => b
                        .table(ot)
                        .map(|o| o.records.iter().map(|r| (r.id.clone(), value::record_title(b, o, &r.id), 0)).collect())
                        .unwrap_or_default(),
                    _ => vec![],
                })
            })
            .unwrap_or_default()
        }
    };
    let make = {
        let kind = kind.clone();
        move |ids: Vec<Id>| -> Option<Value> {
            if ids.is_empty() {
                return None;
            }
            Some(match kind {
                FieldKind::User { .. } => Value::Users(ids),
                FieldKind::Link { .. } => Value::Links(ids),
                _ => Value::Options(ids),
            })
        }
    };
    let is_select = kind.options().is_some();
    let g_tags = get.clone();
    let items_tags = items.clone();
    let k_tags = kind.clone();
    view()
        .child(
            view()
                .element_ref(&at)
                .min_h(px(36.0))
                .px(px(8.0))
                .py(px(6.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(move || if open.get() { rgb(PRIMARY) } else { rgb(BORDER) })
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(px(6.0))
                .on_press(move || open.set(true))
                .child(dynamic(move || {
                    let ids: Vec<Id> = g_tags().map(|v| v.ids().to_vec()).unwrap_or_default();
                    let all = items_tags();
                    let _ = (&k_show, &t_show);
                    if ids.is_empty() {
                        return vec![text("请选择").text_size(px(14.0)).text_color(rgb(0xC9CDD4)).into_element()];
                    }
                    ids.iter()
                        .filter_map(|id| all.iter().find(|x| &x.0 == id))
                        .map(|(id, label, color)| match &k_tags {
                            FieldKind::User { .. } => view()
                                .flex_row()
                                .items_center()
                                .gap(px(4.0))
                                .child(avatar_el(label, *color, 22.0))
                                .child(text(label.clone()).text_size(px(14.0)).text_color(rgb(TEXT)))
                                .into_element(),
                            FieldKind::Link { table: ot, .. } => {
                                let (ot, id) = (ot.clone(), id.clone());
                                view()
                                    .on_press(move || {
                                        // Opens the linked record.
                                        st().open(&ot);
                                        st().expanded.set(Some((ot.clone(), id.clone())));
                                    })
                                    .child(tag_el(label, 0xE8F0FE, 0x245BDB))
                                    .into_element()
                            }
                            _ => {
                                let (bg, fg) = tag_color(*color);
                                tag_el(label, bg, fg).into_element()
                            }
                        })
                        .collect::<Vec<_>>()
                })),
        )
        .child(when(move || open.get(), move || {
            let items = items.clone();
            let get = get.clone();
            let set = set.clone();
            let make = make.clone();
            let table = table.clone();
            let field = field.clone();
            overlay()
                .anchor(&at)
                .placement(Placement::Bottom)
                .align(Align::Start)
                .offset(px(4.0))
                .margin(px(8.0))
                .on_press_outside(move |_: &Point<Points>| {
                    open.set(false);
                    q.set(String::new());
                })
                .on_cancel(move || open.set(false))
                .w(px(300.0))
                .bg(rgb(BG))
                .rounded(px(8.0))
                .border_1()
                .border_color(rgb(BORDER))
                .shadow_lg()
                .child(view().p(px(8.0)).child(widgets::text_field(q, if is_select { "查找或创建选项" } else { "搜索" }).auto_focus(true)))
                .child(scroll().max_h(px(260.0)).py(px(4.0)).child(dynamic(move || {
                    let ql = q.get().to_lowercase();
                    let chosen: Vec<Id> = get().map(|v| v.ids().to_vec()).unwrap_or_default();
                    let all = items();
                    let mut out: Vec<AnyElement> = all
                        .iter()
                        .filter(|x| ql.is_empty() || x.1.to_lowercase().contains(&ql))
                        .map(|(id, label, color)| {
                            let on = chosen.contains(id);
                            let id = id.clone();
                            let chosen = chosen.clone();
                            let set = set.clone();
                            let make = make.clone();
                            let (bg, fg) = tag_color(*color);
                            let label = label.clone();
                            view()
                                .h(px(34.0))
                                .mx(px(4.0))
                                .px(px(8.0))
                                .rounded(px(6.0))
                                .flex_row()
                                .items_center()
                                .gap(px(8.0))
                                .hover(|s| s.bg(rgb(HOVER)))
                                .on_press(move || {
                                    let mut c = chosen.clone();
                                    if let Some(p) = c.iter().position(|x| *x == id) {
                                        c.remove(p);
                                    } else if multiple {
                                        c.push(id.clone());
                                    } else {
                                        c = vec![id.clone()];
                                    }
                                    set(make(c));
                                    if !multiple {
                                        open.set(false);
                                    }
                                })
                                .child(widgets::check_box(on))
                                .child(if is_select { tag_el(&label, bg, fg).into_element() } else { text(label.clone()).text_size(px(14.0)).text_color(rgb(TEXT)).into_element() })
                                .into_element()
                        })
                        .collect();
                    if is_select && !ql.is_empty() && !all.iter().any(|x| x.1.to_lowercase() == ql) {
                        let name = q.get();
                        let (table, field) = (table.clone(), field.clone());
                        let set = set.clone();
                        let make = make.clone();
                        let chosen = chosen.clone();
                        out.push(
                            view()
                                .h(px(34.0))
                                .mx(px(4.0))
                                .px(px(8.0))
                                .rounded(px(6.0))
                                .flex_row()
                                .items_center()
                                .gap(px(8.0))
                                .hover(|s| s.bg(rgb(HOVER)))
                                .on_press(move || {
                                    let id = new_id("opt");
                                    let (tid, fid, oid, nm) = (table.clone(), field.clone(), id.clone(), name.clone());
                                    st().edit(|b| {
                                        if let Some(o) = b.table_mut(&tid).and_then(|t| t.field_mut(&fid)).and_then(|f| f.kind.options_mut()) {
                                            let n = o.len();
                                            o.push(SelectOption { id: oid, name: nm, color: (n % 11) as u8 });
                                        }
                                    });
                                    let mut c = if multiple { chosen.clone() } else { vec![] };
                                    c.push(id);
                                    set(make(c));
                                    q.set(String::new());
                                })
                                .child(text(format!("创建选项「{}」", q.get_untracked())).text_size(px(14.0)).text_color(rgb(PRIMARY)))
                                .into_element(),
                        );
                    }
                    if out.is_empty() {
                        out.push(view().px(px(12.0)).py(px(8.0)).child(text("无匹配项").text_size(px(13.0)).text_color(rgb(TEXT3))).into_element());
                    }
                    out
                })))
        }))
        .into_element()
}

fn comments(table: Id, record: Id, draft: Signal<String>) -> impl IntoElement {
    let (t2, r2) = (table.clone(), record.clone());
    let send = move || {
        let t = draft.get_untracked().trim().to_string();
        if t.is_empty() {
            return;
        }
        let (table, record) = (table.clone(), record.clone());
        st().edit(|b| {
            let me = b.me.clone();
            if let Some(r) = b.table_mut(&table).and_then(|t| t.record_mut(&record)) {
                r.comments.push(Comment { id: new_id("cmt"), author: me, text: t, at: now_ms() });
            }
        });
        draft.set(String::new());
    };
    let send2 = send.clone();
    view()
        .flex_1()
        .child(scroll().flex_1().p(px(16.0)).child(dynamic(move || {
            let list = st().read(|b| {
                b.table(&t2)
                    .and_then(|t| t.record(&r2))
                    .map(|r| r.comments.iter().map(|c| (b.member(&c.author).cloned(), c.text.clone(), c.at)).collect::<Vec<_>>())
                    .unwrap_or_default()
            });
            if list.is_empty() {
                return vec![text("暂无评论").text_size(px(13.0)).text_color(rgb(TEXT3)).into_element()];
            }
            list.into_iter()
                .map(|(m, t, at)| {
                    let name = m.as_ref().map(|m| m.name.clone()).unwrap_or("?".into());
                    let color = m.map(|m| m.color).unwrap_or(0);
                    view()
                        .flex_row()
                        .gap(px(8.0))
                        .mb(px(12.0))
                        .child(avatar_el(&name, color, 28.0))
                        .child(
                            view()
                                .flex_1()
                                .gap(px(2.0))
                                .child(view().flex_row().gap(px(8.0)).child(text(name).text_size(px(13.0)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT))).child(text(value::format_date(at, crate::model::DateFormat::YmdSlash, true)).text_size(px(12.0)).text_color(rgb(TEXT3))))
                                .child(text(t).text_size(px(14.0)).text_color(rgb(TEXT))),
                        )
                        .into_element()
                })
                .collect::<Vec<_>>()
        })))
        .child(
            view()
                .p(px(12.0))
                .border_t(px(1.0))
                .border_color(rgb(BORDER))
                .gap(px(8.0))
                .child(widgets::text_field(draft, "输入评论，Enter 发送").on_submit(move |_: &str| send()))
                .child(view().flex_row().justify_end().child(widgets::primary_button("发送", send2))),
        )
}

fn history(table: Id, record: Id) -> impl IntoElement {
    scroll().flex_1().p(px(16.0)).child(dynamic(move || {
        let (rows, created) = st().read(|b| {
            b.table(&table)
                .and_then(|t| t.record(&record))
                .map(|r| (r.history.iter().rev().map(|h| (b.member_name(&h.1), h.0, h.2.clone(), h.3.clone(), h.4.clone())).collect::<Vec<_>>(), (b.member_name(&r.created_by), r.created_at)))
                .unwrap_or_default()
        });
        let mut out: Vec<AnyElement> = rows
            .into_iter()
            .map(|(who, at, field, old, new)| {
                view()
                    .mb(px(14.0))
                    .gap(px(4.0))
                    .child(view().flex_row().gap(px(8.0)).child(text(who).text_size(px(13.0)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT))).child(text(value::format_date(at, crate::model::DateFormat::YmdSlash, true)).text_size(px(12.0)).text_color(rgb(TEXT3))))
                    .child(text(format!("修改了「{field}」")).text_size(px(13.0)).text_color(rgb(TEXT2)))
                    .child(
                        view()
                            .flex_row()
                            .flex_wrap()
                            .gap(px(6.0))
                            .items_center()
                            .child(text(if old.is_empty() { "空".into() } else { old }).text_size(px(13.0)).text_color(rgb(TEXT3)).line_through())
                            .child(text("→").text_size(px(13.0)).text_color(rgb(TEXT3)))
                            .child(text(if new.is_empty() { "空".into() } else { new }).text_size(px(13.0)).text_color(rgb(TEXT))),
                    )
                    .into_element()
            })
            .collect();
        out.push(
            view()
                .gap(px(4.0))
                .child(text(format!("{} 创建了记录", created.0)).text_size(px(13.0)).text_color(rgb(TEXT2)))
                .child(text(value::format_date(created.1, crate::model::DateFormat::YmdSlash, true)).text_size(px(12.0)).text_color(rgb(TEXT3)))
                .into_element(),
        );
        out
    }))
}
