//! The app's popups: menus of fields, records, views and sidebar items,
//! and the panels of the toolbar (filter, sort, group, fields, row height).

use nsh_core::prelude::*;

use super::{
    field_editor, grid, icons, io, sidebar,
    state::{Confirm, Popup, st},
    theme::*,
    widgets::{self, menu_item, menu_item_danger, menu_title, popover_at, separator},
};
use crate::model::{
    Action, Condition, DateRef, Field, FieldKind, FilterValue, Id, Op, RowHeight, SortRule, Stat, Trigger, Value, View,
    ViewKind, Workflow, new_id, value,
};

fn close() {
    st().popup.set(None);
}

pub fn popup() -> Option<AnyElement> {
    let p = st().popup.get()?;
    untracked(move || build(p))
}

fn build(p: Popup) -> Option<AnyElement> {
    Some(match p {
        Popup::FieldMenu { field, x, y } => field_menu(field, x, y).into_element(),
        Popup::FieldEditor { field, at, x, y } => field_editor::field_editor(field, at, x, y).into_element(),
        Popup::RecordMenu { x, y } => record_menu(x, y).into_element(),
        Popup::ViewMenu { view: v, x, y } => view_menu(v, x, y).into_element(),
        Popup::AddView { x, y } => add_view(x, y).into_element(),
        Popup::Filter { x, y } => filter_panel(x, y).into_element(),
        Popup::Sort { x, y } => rules_panel(false, x, y).into_element(),
        Popup::Group { x, y } => rules_panel(true, x, y).into_element(),
        Popup::Fields { x, y } => fields_panel(x, y).into_element(),
        Popup::RowHeight { x, y } => row_height(x, y).into_element(),
        Popup::Stat { field, x, y } => stat_menu(field, x, y).into_element(),
        Popup::SidebarMenu { item, x, y } => sidebar_menu(item, x, y).into_element(),
        Popup::SidebarAdd { x, y } => sidebar_add(x, y).into_element(),
        Popup::More { x, y } => more_menu(x, y).into_element(),
        Popup::Shortcuts => shortcuts().into_element(),
        Popup::Workflows => workflows().into_element(),
        Popup::Confirm { title, text: body, action } => confirm(title, body, action).into_element(),
    })
}

/// A centered dialog over a dimmed window.
pub fn modal(width: f32, title: impl Into<String>, on_close: impl Fn() + Clone + 'static, content: impl IntoElement) -> impl IntoElement {
    let c1 = on_close.clone();
    let c2 = on_close.clone();
    overlay()
        .inset(px(0.0))
        .bg(rgba(0x1F232966))
        .items_center()
        .justify_center()
        .on_cancel(move || c1())
        .child(view().absolute().inset(px(0.0)).focusable(false).on_press(move || c2()))
        .child(
            view()
                .w(px(width))
                .max_h(px(720.0))
                .bg(rgb(BG))
                .rounded(px(10.0))
                .shadow_xl()
                .child(
                    view()
                        .h(px(56.0))
                        .px(px(20.0))
                        .flex_row()
                        .items_center()
                        .child(text(title.into()).flex_1().text_size(px(16.0)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT)))
                        .child(
                            view()
                                .size(px(28.0))
                                .rounded(px(6.0))
                                .items_center()
                                .justify_center()
                                .hover(|s| s.bg(rgb(HOVER)))
                                .on_press(on_close)
                                .child(icons::icon("x", 16.0, TEXT2)),
                        ),
                )
                .child(content),
        )
}

// ---------------------------------------------------------------------------
// Fields.

fn view_position(field: &str) -> Option<usize> {
    st().with_view(|_, t, v| t.view_fields(v).iter().position(|(f, _)| f.id == field)).flatten()
}

fn field_menu(field: Id, x: f32, y: f32) -> impl IntoElement {
    let s = st();
    let info = s.with_view(|_, t, v| {
        let f = t.field(&field)?.clone();
        let primary = t.primary().map(|p| p.id == field).unwrap_or(false);
        let pos = t.view_fields(v).iter().filter(|(_, h)| !h).position(|(f2, _)| f2.id == field).unwrap_or(0);
        Some((f, primary, pos, v.frozen))
    });
    let Some(Some((f, primary, pos, frozen))) = info else { return popover_at(x, y, 220.0, close, view()) };
    let fid = f.id.clone();
    let f1 = fid.clone();
    let f2 = fid.clone();
    let f3 = fid.clone();
    let f4 = fid.clone();
    let f5 = fid.clone();
    let f6 = fid.clone();
    let f7 = fid.clone();
    let f8 = fid.clone();
    let f9 = fid.clone();
    let f10 = fid.clone();
    let f11 = fid.clone();
    let date_or_num = f.kind.is_numeric() || f.kind.is_date();
    let (asc, desc) = if date_or_num { ("升序排列 1→9", "降序排列 9→1") } else { ("升序排列 A→Z", "降序排列 Z→A") };
    popover_at(
        x,
        y,
        230.0,
        close,
        view()
            .child(menu_item("edit", "修改字段", move || s.popup.set(Some(Popup::FieldEditor { field: Some(f1.clone()), at: None, x, y }))))
            .child(menu_item("info", "编辑字段描述", move || s.popup.set(Some(Popup::FieldEditor { field: Some(f2.clone()), at: None, x, y }))))
            .child(separator())
            .child(menu_item("insert-left", "向左插入字段", move || {
                let at = view_position(&f3);
                s.popup.set(Some(Popup::FieldEditor { field: None, at: at.map(|a| a.max(1)), x, y }))
            }))
            .child(menu_item("insert-right", "向右插入字段", move || {
                let at = view_position(&f4).map(|a| a + 1);
                s.popup.set(Some(Popup::FieldEditor { field: None, at, x, y }))
            }))
            .child(menu_item("copy", "复制字段", move || {
                duplicate_field(&f5);
                close();
            }))
            .child(separator())
            .child(menu_item("sort-asc", asc, move || {
                s.edit_view(|v| v.sorts = vec![SortRule { field: f6.clone(), desc: false }]);
                close();
            }))
            .child(menu_item("sort-desc", desc, move || {
                s.edit_view(|v| v.sorts = vec![SortRule { field: f7.clone(), desc: true }]);
                close();
            }))
            .child(menu_item("group", "按此字段分组", move || {
                s.edit_view(|v| {
                    if !v.groups.iter().any(|g| g.field == f8) {
                        v.groups.push(SortRule { field: f8.clone(), desc: false });
                        v.groups.truncate(3);
                    }
                });
                close();
            }))
            .child(menu_item("filter", "按此字段筛选", move || {
                let op = s.with_table(|_, t| t.field(&f9).map(|f| Op::for_kind(&f.kind)[0])).flatten().unwrap_or(Op::Is);
                s.edit_view(|v| v.filter.conditions.push(Condition { id: new_id("cnd"), field: f9.clone(), op, value: FilterValue::None }));
                s.popup.set(Some(Popup::Filter { x, y }));
            }))
            .child(separator())
            .child(if pos + 1 == frozen {
                menu_item("freeze", "取消冻结", move || {
                    s.edit_view(|v| v.frozen = 1);
                    close();
                })
                .into_element()
            } else {
                menu_item("freeze", "冻结至此字段", move || {
                    s.edit_view(|v| v.frozen = pos + 1);
                    close();
                })
                .into_element()
            })
            .child(if primary {
                view().into_element()
            } else {
                menu_item("eye-off", "隐藏字段", move || {
                    s.edit_view(|v| {
                        if !v.hidden.contains(&f10) {
                            v.hidden.push(f10.clone());
                        }
                    });
                    close();
                })
                .into_element()
            })
            .child(if primary {
                view().into_element()
            } else {
                let name = f.name.clone();
                view()
                    .child(separator())
                    .child(menu_item_danger("trash", "删除字段", move || {
                        s.popup.set(Some(Popup::Confirm {
                            title: "删除字段".into(),
                            text: format!("确定要删除字段「{name}」吗？字段中的数据将一并删除。"),
                            action: Confirm::DeleteField(f11.clone()),
                        }))
                    }))
                    .into_element()
            }),
    )
}

fn duplicate_field(fid: &str) {
    let s = st();
    let vid = s.view.get_untracked();
    s.edit_table(|t, _| {
        let Some(f) = t.field(fid).cloned() else { return };
        let mut nf = f.clone();
        nf.id = new_id("fld");
        nf.name = t.unique_field_name(&format!("{} 副本", f.name));
        for r in &mut t.records {
            if let Some(v) = r.cells.get(fid).cloned() {
                r.cells.insert(nf.id.clone(), v);
            }
        }
        let pos = t.fields.iter().position(|x| x.id == fid).unwrap_or(t.fields.len() - 1);
        t.fields.insert(pos + 1, nf.clone());
        // In every view the copy sits right of its source.
        let orders: Vec<(usize, Vec<Id>)> = t
            .views
            .iter()
            .enumerate()
            .map(|(i, v)| (i, t.view_fields(v).into_iter().map(|(f, _)| f.id.clone()).collect()))
            .collect();
        for (i, mut order) in orders {
            order.retain(|x| *x != nf.id);
            let at = order.iter().position(|x| x == fid).map(|p| p + 1).unwrap_or(order.len());
            order.insert(at, nf.id.clone());
            t.views[i].field_order = order;
        }
        let _ = &vid;
    });
}

pub fn delete_field(fid: &str) {
    let s = st();
    let cur = s.cur.get_untracked();
    s.edit(|b| {
        if let Some(t) = b.table_mut(&cur) {
            t.fields.retain(|f| f.id != fid);
            for r in &mut t.records {
                r.cells.remove(fid);
            }
            for v in &mut t.views {
                v.field_order.retain(|f| f != fid);
                v.hidden.retain(|f| f != fid);
                v.sorts.retain(|r| r.field != fid);
                v.groups.retain(|r| r.field != fid);
                v.filter.conditions.retain(|c| c.field != fid);
                v.widths.remove(fid);
                if v.group_field.as_deref() == Some(fid) {
                    v.group_field = None;
                }
                if let Some(form) = &mut v.form {
                    form.fields.retain(|(f, _)| f != fid);
                }
            }
        }
        // The other side of a duplex link goes too.
        for t in &mut b.tables {
            let pairs: Vec<Id> = t
                .fields
                .iter()
                .filter(|f| matches!(&f.kind, FieldKind::Link { pair: Some(p), .. } if p == fid))
                .map(|f| f.id.clone())
                .collect();
            for p in pairs {
                if let Some(f) = t.field_mut(&p) {
                    if let FieldKind::Link { pair, .. } = &mut f.kind {
                        *pair = None;
                    }
                }
            }
        }
    });
    s.toast("字段已删除");
}

// ---------------------------------------------------------------------------
// Records.

fn record_menu(x: f32, y: f32) -> impl IntoElement {
    let s = st();
    let Some(g) = grid::grid() else { return popover_at(x, y, 220.0, close, view()) };
    let ids = g.selected_records();
    let n = ids.len();
    let active = g.active();
    let first = ids.first().cloned();
    let ids2 = ids.clone();
    let ids3 = ids.clone();
    let a1 = active.clone();
    let a2 = active.clone();
    popover_at(
        x,
        y,
        230.0,
        close,
        view()
            .child(when(move || n <= 1, move || {
                let a1 = a1.clone();
                let a2 = a2.clone();
                view()
                    .child(menu_item("insert-above", "向上插入记录", move || {
                        close();
                        if let Some((rid, ..)) = a1.clone() {
                            grid::edit::add_record(g, vec![], Some((rid, false)));
                        }
                    }))
                    .child(menu_item("insert-below", "向下插入记录", move || {
                        close();
                        if let Some((rid, ..)) = a2.clone() {
                            grid::edit::add_record(g, vec![], Some((rid, true)));
                        }
                    }))
                    .child(separator())
            }))
            .child(menu_item("expand", "展开记录", move || {
                close();
                if let Some(r) = first.clone() {
                    s.expanded.set(Some((s.cur.get_untracked(), r)));
                }
            }))
            .child(menu_item_full_hint("copy", "复制", "Ctrl+C", move || {
                close();
                grid::keys::copy(g, false);
            }))
            .child(menu_item_full_hint("copy", "剪切", "Ctrl+X", move || {
                close();
                grid::keys::copy(g, true);
            }))
            .child(menu_item_full_hint("copy", "粘贴", "Ctrl+V", move || {
                close();
                grid::keys::paste(g);
            }))
            .child(menu_item("copy", "创建副本", move || {
                close();
                duplicate_records(&ids2);
            }))
            .child(separator())
            .child(menu_item_danger("trash", if n > 1 { format!("删除 {n} 条记录") } else { "删除记录".to_string() }, move || {
                close();
                delete_records(&ids3);
            })),
    )
}

fn menu_item_full_hint(icon: &'static str, label: &'static str, hint: &'static str, on: impl Fn() + 'static) -> impl IntoElement {
    widgets::menu_item_full(icon, label.into(), hint.into(), false, on)
}

pub fn duplicate_records(ids: &[Id]) {
    let s = st();
    let cur = s.cur.get_untracked();
    s.edit(|b| {
        let me = b.me.clone();
        let Some(t) = b.table_mut(&cur) else { return };
        for id in ids {
            let Some(pos) = t.records.iter().position(|r| &r.id == id) else { continue };
            let mut copy = t.new_record(&me);
            copy.cells = t.records[pos].cells.clone();
            let nid = copy.id.clone();
            t.records.insert(pos + 1, copy);
            for v in &mut t.views {
                if let Some(p) = v.row_order.iter().position(|x| x == id) {
                    v.row_order.insert(p + 1, nid.clone());
                }
            }
        }
    });
    s.toast(&format!("已创建 {} 条记录副本", ids.len()));
}

pub fn delete_records(ids: &[Id]) {
    let s = st();
    let cur = s.cur.get_untracked();
    let n = ids.len();
    s.edit(|b| {
        if let Some(t) = b.table_mut(&cur) {
            t.records.retain(|r| !ids.contains(&r.id));
            for v in &mut t.views {
                v.row_order.retain(|x| !ids.contains(x));
            }
        }
        b.sync_links();
    });
    s.checked.set(vec![]);
    if s.expanded.get_untracked().is_some_and(|(_, r)| ids.contains(&r)) {
        s.expanded.set(None);
    }
    s.toast(&format!("已删除 {n} 条记录"));
}

// ---------------------------------------------------------------------------
// Views.

fn view_menu(vid: Id, x: f32, y: f32) -> impl IntoElement {
    let s = st();
    let count = s.with_table(|_, t| t.views.len()).unwrap_or(0);
    let v1 = vid.clone();
    let v2 = vid.clone();
    let v3 = vid.clone();
    popover_at(
        x,
        y,
        200.0,
        close,
        view()
            .child(menu_item("edit", "重命名视图", move || {
                close();
                s.view.set(v1.clone());
                st().toast("双击视图名称即可重命名");
            }))
            .child(menu_item("copy", "创建副本", move || {
                close();
                let cur = s.cur.get_untracked();
                let nid = s.edit(|b| {
                    let t = b.table_mut(&cur)?;
                    let pos = t.views.iter().position(|v| v.id == v2)?;
                    let mut v = t.views[pos].clone();
                    v.id = new_id("vew");
                    v.name = format!("{} 副本", v.name);
                    let id = v.id.clone();
                    t.views.insert(pos + 1, v);
                    Some(id)
                });
                if let Some(id) = nid {
                    s.view.set(id);
                }
            }))
            .child(when(move || count > 1, move || {
                let v3 = v3.clone();
                view().child(separator()).child(menu_item_danger("trash", "删除视图", move || {
                    let name = st().with_table(|_, t| t.view(&v3).map(|v| v.name.clone())).flatten().unwrap_or_default();
                    st().popup.set(Some(Popup::Confirm {
                        title: "删除视图".into(),
                        text: format!("确定要删除视图「{name}」吗？视图中的数据不会被删除。"),
                        action: Confirm::DeleteView(v3.clone()),
                    }))
                }))
            })),
    )
}

pub fn new_view(kind: ViewKind) {
    let s = st();
    let cur = s.cur.get_untracked();
    let id = s.edit(|b| {
        let t = b.table_mut(&cur)?;
        let n = t.views.iter().filter(|v| v.kind == kind).count();
        let name = if n == 0 { kind.short().to_string() } else { format!("{}{}", kind.short(), n + 1) };
        let mut v = View::new(&format!("{name}视图").replace("视图视图", "视图"), kind);
        let first_of = |pred: &dyn Fn(&Field) -> bool| t.fields.iter().find(|f| pred(f)).map(|f| f.id.clone());
        match kind {
            ViewKind::Kanban => {
                v.group_field = first_of(&|f| matches!(f.kind, FieldKind::SingleSelect { .. }))
                    .or_else(|| first_of(&|f| matches!(f.kind, FieldKind::User { .. })));
                v.cover_field = first_of(&|f| matches!(f.kind, FieldKind::Attachment));
            }
            ViewKind::Gallery => v.cover_field = first_of(&|f| matches!(f.kind, FieldKind::Attachment)),
            ViewKind::Gantt | ViewKind::Calendar => {
                let dates: Vec<Id> = t.fields.iter().filter(|f| f.kind.is_date()).map(|f| f.id.clone()).collect();
                v.start_field = dates.first().cloned();
                v.end_field = dates.get(1).cloned().or(dates.first().cloned());
            }
            ViewKind::Form => {
                v.form = Some(crate::model::FormConfig {
                    title: t.name.clone(),
                    description: String::new(),
                    fields: t.fields.iter().filter(|f| !f.kind.is_computed()).map(|f| (f.id.clone(), false)).collect(),
                    submit_text: "提交".into(),
                });
            }
            ViewKind::Grid => {}
        }
        let id = v.id.clone();
        t.views.push(v);
        Some(id)
    });
    if let Some(id) = id {
        s.view.set(id);
    }
}

fn add_view(x: f32, y: f32) -> impl IntoElement {
    popover_at(
        x,
        y,
        220.0,
        close,
        view().child(menu_title("新建视图")).child(
            ViewKind::ALL
                .iter()
                .map(|k| {
                    let k = *k;
                    menu_item(super::icons::view_icon(k), k.label(), move || {
                        close();
                        new_view(k);
                    })
                    .into_element()
                })
                .collect::<Vec<_>>(),
        ),
    )
}

// ---------------------------------------------------------------------------
// Filter.

fn field_names() -> Vec<(Id, String, FieldKind)> {
    st().with_table(|_, t| t.fields.iter().map(|f| (f.id.clone(), f.name.clone(), f.kind.clone())).collect()).unwrap_or_default()
}

fn filter_panel(x: f32, y: f32) -> impl IntoElement {
    let s = st();
    popover_at(
        x,
        y,
        620.0,
        close,
        view()
            .px(px(16.0))
            .py(px(12.0))
            .gap(px(10.0))
            .child(
                view()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .child(text("设置筛选条件").text_size(px(14.0)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT)))
                    .child(view().flex_1())
                    .child(when(move || s.with_view(|_, _, v| v.filter.conditions.len() > 1).unwrap_or(false), move || {
                        view()
                            .flex_row()
                            .items_center()
                            .gap(px(6.0))
                            .child(text("符合以下").text_size(px(13.0)).text_color(rgb(TEXT2)))
                            .child(widgets::dropdown(
                                move || if s.with_view(|_, _, v| v.filter.any).unwrap_or(false) { "任一条件".into() } else { "所有条件".into() },
                                || vec!["所有条件".into(), "任一条件".into()],
                                move |i| s.edit_view(|v| v.filter.any = i == 1),
                                110.0,
                            ))
                    })),
            )
            .child(dynamic(move || {
                let conds = s.with_view(|_, _, v| v.filter.conditions.clone()).unwrap_or_default();
                if conds.is_empty() {
                    return vec![view().py(px(8.0)).child(text("暂无筛选条件").text_size(px(13.0)).text_color(rgb(TEXT3))).into_element()];
                }
                conds.into_iter().map(condition_row).collect::<Vec<_>>()
            }))
            .child(
                view()
                    .flex_row()
                    .child(widgets::link_button("+ 添加条件", move || {
                        let fields = field_names();
                        if let Some((id, _, kind)) = fields.first() {
                            let op = Op::for_kind(kind)[0];
                            let id = id.clone();
                            s.edit_view(|v| v.filter.conditions.push(Condition { id: new_id("cnd"), field: id, op, value: FilterValue::None }));
                        }
                    })),
            ),
    )
}

fn set_condition(id: &str, f: impl FnOnce(&mut Condition)) {
    st().edit_view(|v| {
        if let Some(c) = v.filter.conditions.iter_mut().find(|c| c.id == id) {
            f(c)
        }
    });
}

fn condition_row(c: Condition) -> AnyElement {
    let s = st();
    let fields = field_names();
    let kind = fields.iter().find(|f| f.0 == c.field).map(|f| f.2.clone()).unwrap_or(FieldKind::Text);
    let ops = Op::for_kind(&kind);
    let is_date = kind.is_date();
    let cid = c.id.clone();
    let cid2 = c.id.clone();
    let cid3 = c.id.clone();
    let fname = fields.iter().find(|f| f.0 == c.field).map(|f| f.1.clone()).unwrap_or_default();
    let fields2 = fields.clone();
    let ops2 = ops.clone();
    let op = c.op;
    view()
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .child(widgets::dropdown(
            move || fname.clone(),
            move || fields.iter().map(|f| f.1.clone()).collect(),
            move |i| {
                let (fid, _, k) = fields2[i].clone();
                set_condition(&cid, |c| {
                    c.field = fid;
                    c.op = Op::for_kind(&k)[0];
                    c.value = FilterValue::None;
                });
            },
            150.0,
        ))
        .child(widgets::dropdown(
            move || op.label(is_date).to_string(),
            move || ops.iter().map(|o| o.label(is_date).to_string()).collect(),
            move |i| {
                let o = ops2[i];
                set_condition(&cid2, |c| c.op = o);
            },
            110.0,
        ))
        .child(view().flex_1().child(if op.needs_value() || matches!(kind, FieldKind::Checkbox) {
            value_editor(c.clone(), kind.clone()).into_element()
        } else {
            view().into_element()
        }))
        .child(
            view()
                .size(px(28.0))
                .rounded(px(6.0))
                .items_center()
                .justify_center()
                .hover(|s| s.bg(rgb(HOVER)))
                .on_press(move || s.edit_view(|v| v.filter.conditions.retain(|x| x.id != cid3)))
                .child(icons::icon("trash", 16.0, TEXT3)),
        )
        .into_element()
}

/// The value of a condition: text, a number, options, people, a date.
fn value_editor(c: Condition, kind: FieldKind) -> impl IntoElement {
    let s = st();
    let cid = c.id.clone();
    match &kind {
        FieldKind::Checkbox => {
            let on = matches!(c.value, FilterValue::Bool(true));
            view()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .on_press(move || set_condition(&cid, |c| c.value = FilterValue::Bool(!on)))
                .child(widgets::check_box(on))
                .child(text(if on { "已勾选" } else { "未勾选" }).text_size(px(13.0)).text_color(rgb(TEXT2)))
                .into_element()
        }
        FieldKind::SingleSelect { options } | FieldKind::MultiSelect { options } => {
            let chosen: Vec<Id> = match &c.value {
                FilterValue::Ids(ids) => ids.clone(),
                _ => vec![],
            };
            view()
                .flex_row()
                .flex_wrap()
                .gap(px(4.0))
                .child(
                    options
                        .iter()
                        .map(|o| {
                            let on = chosen.contains(&o.id);
                            let oid = o.id.clone();
                            let cid = cid.clone();
                            let (bg, fg) = value::tag_color(o.color);
                            view()
                                .rounded(px(12.0))
                                .border(px(2.0))
                                .border_color(if on { rgb(PRIMARY) } else { rgba(0xFFFFFF00) })
                                .on_press(move || {
                                    let oid = oid.clone();
                                    set_condition(&cid, |c| {
                                        let mut ids = match &c.value {
                                            FilterValue::Ids(i) => i.clone(),
                                            _ => vec![],
                                        };
                                        if let Some(p) = ids.iter().position(|x| *x == oid) {
                                            ids.remove(p);
                                        } else {
                                            ids.push(oid);
                                        }
                                        c.value = FilterValue::Ids(ids);
                                    })
                                })
                                .child(widgets::tag_el(&o.name, bg, fg))
                                .into_element()
                        })
                        .collect::<Vec<_>>(),
                )
                .into_element()
        }
        FieldKind::User { .. } | FieldKind::CreatedBy | FieldKind::ModifiedBy => {
            let chosen: Vec<Id> = match &c.value {
                FilterValue::Ids(ids) => ids.clone(),
                _ => vec![],
            };
            let members = s.peek(|b| b.members.clone());
            view()
                .flex_row()
                .flex_wrap()
                .gap(px(4.0))
                .child(
                    members
                        .into_iter()
                        .map(|m| {
                            let on = chosen.contains(&m.id);
                            let cid = cid.clone();
                            let mid = m.id.clone();
                            view()
                                .flex_row()
                                .items_center()
                                .gap(px(4.0))
                                .px(px(6.0))
                                .py(px(3.0))
                                .rounded(px(12.0))
                                .bg(if on { rgb(PRIMARY_LIGHT) } else { rgb(BG_GREY) })
                                .on_press(move || {
                                    let mid = mid.clone();
                                    set_condition(&cid, |c| {
                                        let mut ids = match &c.value {
                                            FilterValue::Ids(i) => i.clone(),
                                            _ => vec![],
                                        };
                                        if let Some(p) = ids.iter().position(|x| *x == mid) {
                                            ids.remove(p);
                                        } else {
                                            ids.push(mid);
                                        }
                                        c.value = FilterValue::Ids(ids);
                                    })
                                })
                                .child(widgets::avatar_el(&m.name, m.color, 18.0))
                                .child(text(m.name.clone()).text_size(px(12.0)).text_color(rgb(TEXT)))
                                .into_element()
                        })
                        .collect::<Vec<_>>(),
                )
                .into_element()
        }
        k if k.is_date() => {
            let label = match &c.value {
                FilterValue::Date(d) => d.label(),
                _ => "选择日期".into(),
            };
            let exact = signal(match &c.value {
                FilterValue::Date(DateRef::Exact(ms)) => value::format_date(*ms, crate::model::DateFormat::YmdSlash, false),
                _ => String::new(),
            });
            let cid2 = cid.clone();
            view()
                .flex_row()
                .gap(px(6.0))
                .child(widgets::dropdown(
                    move || label.clone(),
                    || DateRef::RELATIVE.iter().map(|d| d.label()).chain(std::iter::once("指定日期".to_string())).collect(),
                    move |i| {
                        let v = DateRef::RELATIVE.get(i).cloned().unwrap_or(DateRef::Exact(value::today_ms()));
                        set_condition(&cid, |c| c.value = FilterValue::Date(v));
                    },
                    130.0,
                ))
                .child(
                    widgets::text_field(exact, "2024/01/30").w(px(120.0)).on_submit(move |t: &str| {
                        if let Some(ms) = value::parse_date(t) {
                            set_condition(&cid2, |c| c.value = FilterValue::Date(DateRef::Exact(ms)));
                        }
                    }),
                )
                .into_element()
        }
        _ => {
            let initial = match &c.value {
                FilterValue::Text(t) => t.clone(),
                FilterValue::Number(n) => value::format_plain(*n),
                _ => String::new(),
            };
            let t = signal(initial);
            let numeric = kind.is_numeric();
            widgets::text_field(t, "请输入")
                .w_full()
                .on_change(move |txt: &str| {
                    t.set(txt.to_string());
                    let v = if numeric {
                        txt.trim().parse::<f64>().map(FilterValue::Number).unwrap_or(FilterValue::Text(txt.to_string()))
                    } else {
                        FilterValue::Text(txt.to_string())
                    };
                    set_condition(&cid, |c| c.value = v);
                })
                .into_element()
        }
    }
}

// ---------------------------------------------------------------------------
// Sort and group.

fn rules_panel(group: bool, x: f32, y: f32) -> impl IntoElement {
    let s = st();
    let rules = move || s.with_view(|_, _, v| if group { v.groups.clone() } else { v.sorts.clone() }).unwrap_or_default();
    let edit_rules = move |f: Box<dyn FnOnce(&mut Vec<SortRule>)>| {
        s.edit_view(|v| f(if group { &mut v.groups } else { &mut v.sorts }));
    };
    popover_at(
        x,
        y,
        420.0,
        close,
        view()
            .px(px(16.0))
            .py(px(12.0))
            .gap(px(10.0))
            .child(text(if group { "设置分组条件" } else { "设置排序条件" }).text_size(px(14.0)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT)))
            .child(dynamic(move || {
                let rs = rules();
                if rs.is_empty() {
                    return vec![view().py(px(4.0)).child(text(if group { "暂无分组条件" } else { "暂无排序条件" }).text_size(px(13.0)).text_color(rgb(TEXT3))).into_element()];
                }
                let fields = field_names();
                rs.into_iter()
                    .enumerate()
                    .map(|(i, r)| {
                        let fields2 = fields.clone();
                        let fname = fields.iter().find(|f| f.0 == r.field).map(|f| f.1.clone()).unwrap_or_default();
                        let kind = fields.iter().find(|f| f.0 == r.field).map(|f| f.2.clone()).unwrap_or(FieldKind::Text);
                        let numeric = kind.is_numeric() || kind.is_date();
                        let (a, d) = if numeric { ("1 → 9", "9 → 1") } else { ("A → Z", "Z → A") };
                        let desc = r.desc;
                        view()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .child(widgets::dropdown(
                                move || fname.clone(),
                                {
                                    let fields = fields.clone();
                                    move || fields.iter().map(|f| f.1.clone()).collect()
                                },
                                move |k| {
                                    let fid = fields2[k].0.clone();
                                    edit_rules(Box::new(move |rs| rs[i].field = fid));
                                },
                                170.0,
                            ))
                            .child(widgets::dropdown(
                                move || if desc { d.to_string() } else { a.to_string() },
                                move || vec![a.to_string(), d.to_string()],
                                move |k| edit_rules(Box::new(move |rs| rs[i].desc = k == 1)),
                                110.0,
                            ))
                            .child(view().flex_1())
                            .child(
                                view()
                                    .size(px(28.0))
                                    .rounded(px(6.0))
                                    .items_center()
                                    .justify_center()
                                    .hover(|s| s.bg(rgb(HOVER)))
                                    .on_press(move || edit_rules(Box::new(move |rs| {
                                        rs.remove(i);
                                    })))
                                    .child(icons::icon("trash", 16.0, TEXT3)),
                            )
                            .into_element()
                    })
                    .collect::<Vec<_>>()
            }))
            .child(
                view()
                    .flex_row()
                    .items_center()
                    .child(when(move || !group || rules().len() < 3, move || {
                        widgets::link_button(if group { "+ 添加分组" } else { "+ 添加排序" }, move || {
                            let fields = field_names();
                            let used: Vec<Id> = rules().iter().map(|r| r.field.clone()).collect();
                            if let Some((id, _, _)) = fields.iter().find(|f| !used.contains(&f.0)) {
                                let id = id.clone();
                                edit_rules(Box::new(move |rs| rs.push(SortRule { field: id, desc: false })));
                            }
                        })
                    }))
                    .child(view().flex_1())
                    .child(when(move || !group, move || {
                        view()
                            .flex_row()
                            .items_center()
                            .gap(px(6.0))
                            .child(text("自动排序").text_size(px(13.0)).text_color(rgb(TEXT2)))
                            .child(widgets::switch(
                                move || s.with_view(|_, _, v| v.auto_sort).unwrap_or(true),
                                move || s.edit_view(|v| v.auto_sort = !v.auto_sort),
                            ))
                    })),
            ),
    )
}

// ---------------------------------------------------------------------------
// Fields shown, row height, statistics.

fn fields_panel(x: f32, y: f32) -> impl IntoElement {
    let s = st();
    let q = signal(String::new());
    let kind = s.view_kind();
    popover_at(
        x,
        y,
        300.0,
        close,
        view()
            .px(px(8.0))
            .py(px(6.0))
            .gap(px(4.0))
            .child(view().px(px(4.0)).child(widgets::text_field(q, "搜索字段")))
            .child(when(move || matches!(kind, Some(ViewKind::Gallery | ViewKind::Kanban)), move || {
                view().px(px(4.0)).py(px(4.0)).gap(px(6.0)).child(text("封面").text_size(px(12.0)).text_color(rgb(TEXT3))).child(widgets::dropdown(
                    move || {
                        s.with_view(|_, t, v| v.cover_field.as_ref().and_then(|f| t.field(f)).map(|f| f.name.clone()))
                            .flatten()
                            .unwrap_or("无封面".into())
                    },
                    move || {
                        let mut o = vec!["无封面".to_string()];
                        o.extend(s.with_table(|_, t| t.fields.iter().filter(|f| matches!(f.kind, FieldKind::Attachment)).map(|f| f.name.clone()).collect::<Vec<_>>()).unwrap_or_default());
                        o
                    },
                    move |i| {
                        let ids: Vec<Id> = s.with_table(|_, t| t.fields.iter().filter(|f| matches!(f.kind, FieldKind::Attachment)).map(|f| f.id.clone()).collect()).unwrap_or_default();
                        s.edit_view(|v| v.cover_field = if i == 0 { None } else { ids.get(i - 1).cloned() });
                    },
                    270.0,
                ))
            }))
            .child(
                scroll().max_h(px(420.0)).child(dynamic(move || {
                    let ql = q.get().to_lowercase();
                    let fields = s.with_view(|_, t, v| t.view_fields(v).into_iter().map(|(f, h)| (f.id.clone(), f.name.clone(), f.kind.clone(), h)).collect::<Vec<_>>()).unwrap_or_default();
                    let n = fields.len();
                    fields
                        .into_iter()
                        .enumerate()
                        .filter(|(_, f)| ql.is_empty() || f.1.to_lowercase().contains(&ql))
                        .map(|(i, (id, name, kind, hidden))| {
                            let id2 = id.clone();
                            let primary = i == 0;
                            view()
                                .h(px(34.0))
                                .px(px(8.0))
                                .rounded(px(6.0))
                                .flex_row()
                                .items_center()
                                .gap(px(8.0))
                                .hover(|s| s.bg(rgb(HOVER)))
                                .child(icons::icon(icons::field_icon(&kind), 16.0, TEXT2))
                                .child(text(name).flex_1().text_size(px(14.0)).text_color(rgb(if hidden { TEXT3 } else { TEXT })).number_of_lines(1u32))
                                .child(when(move || i > 1, {
                                    let id = id.clone();
                                    move || {
                                        let id = id.clone();
                                        view()
                                            .size(px(22.0))
                                            .items_center()
                                            .justify_center()
                                            .rounded(px(4.0))
                                            .hover(|s| s.bg(rgb(0xDEE0E3)))
                                            .on_press(move || move_field(&id, -1))
                                            .child(icons::icon("chevron-up", 14.0, TEXT3))
                                    }
                                }))
                                .child(when(move || i > 0 && i + 1 < n, {
                                    let id = id.clone();
                                    move || {
                                        let id = id.clone();
                                        view()
                                            .size(px(22.0))
                                            .items_center()
                                            .justify_center()
                                            .rounded(px(4.0))
                                            .hover(|s| s.bg(rgb(0xDEE0E3)))
                                            .on_press(move || move_field(&id, 1))
                                            .child(icons::icon("chevron-down", 14.0, TEXT3))
                                    }
                                }))
                                .child(if primary {
                                    view().w(px(28.0)).into_element()
                                } else {
                                    widgets::switch(move || !hidden, move || {
                                        let id = id2.clone();
                                        st().edit_view(|v| {
                                            if let Some(p) = v.hidden.iter().position(|x| *x == id) {
                                                v.hidden.remove(p);
                                            } else {
                                                v.hidden.push(id);
                                            }
                                        })
                                    })
                                    .into_element()
                                })
                                .into_element()
                        })
                        .collect::<Vec<_>>()
                })),
            )
            .child(separator())
            .child(
                view()
                    .flex_row()
                    .justify_between()
                    .px(px(4.0))
                    .child(widgets::link_button("全部显示", move || s.edit_view(|v| v.hidden.clear())))
                    .child(widgets::link_button("全部隐藏", move || {
                        let ids: Vec<Id> = s.with_table(|_, t| t.fields.iter().skip(1).map(|f| f.id.clone()).collect()).unwrap_or_default();
                        s.edit_view(|v| v.hidden = ids);
                    }))
                    .child(widgets::link_button("+ 新建字段", move || st().popup.set(Some(Popup::FieldEditor { field: None, at: None, x, y })))),
            ),
    )
}

fn move_field(id: &str, dir: isize) {
    let s = st();
    let cur = s.cur.get_untracked();
    let vid = s.view.get_untracked();
    s.edit(|b| {
        let Some(t) = b.table_mut(&cur) else { return };
        let Some(v) = t.view(&vid).cloned() else { return };
        let mut order: Vec<Id> = t.view_fields(&v).into_iter().map(|(f, _)| f.id.clone()).collect();
        let Some(p) = order.iter().position(|x| x == id) else { return };
        let np = p as isize + dir;
        if np < 1 || np as usize >= order.len() {
            return;
        }
        order.swap(p, np as usize);
        if let Some(v) = t.view_mut(&vid) {
            v.field_order = order;
        }
    });
}

fn row_height(x: f32, y: f32) -> impl IntoElement {
    let s = st();
    let cur = move || s.with_view(|_, _, v| v.row_height).unwrap_or(RowHeight::Short);
    popover_at(
        x,
        y,
        200.0,
        close,
        view()
            .child(menu_title("行高"))
            .child(
                RowHeight::ALL
                    .iter()
                    .map(|h| {
                        let h = *h;
                        view()
                            .h(px(34.0))
                            .mx(px(4.0))
                            .px(px(10.0))
                            .rounded(px(6.0))
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .hover(|s| s.bg(rgb(HOVER)))
                            .on_press(move || {
                                s.edit_view(|v| v.row_height = h);
                                close();
                            })
                            .child(text(h.label()).flex_1().text_size(px(14.0)).text_color(rgb(TEXT)))
                            .child(icons::icon_dyn(|| "check", 16.0, move || if cur() == h { PRIMARY } else { BG }))
                            .into_element()
                    })
                    .collect::<Vec<_>>(),
            ),
    )
}

fn stat_menu(field: Id, x: f32, y: f32) -> impl IntoElement {
    let s = st();
    let kind = s.with_table(|_, t| t.field(&field).map(|f| f.kind.clone())).flatten().unwrap_or(FieldKind::Text);
    let cur = s.with_view(|_, _, v| v.stats.get(&field).copied().unwrap_or(Stat::None)).unwrap_or(Stat::None);
    let stats = Stat::for_kind(&kind);
    let h = stats.len() as f32 * 34.0 + 16.0;
    popover_at(
        x,
        (y - h - 8.0).max(60.0),
        180.0,
        close,
        view().child(
            stats
                .into_iter()
                .map(|st_| {
                    let f = field.clone();
                    view()
                        .h(px(34.0))
                        .mx(px(4.0))
                        .px(px(10.0))
                        .rounded(px(6.0))
                        .flex_row()
                        .items_center()
                        .hover(|s| s.bg(rgb(HOVER)))
                        .on_press(move || {
                            let f = f.clone();
                            s.edit_view(|v| {
                                v.stats.insert(f, st_);
                            });
                            close();
                        })
                        .child(text(st_.label()).flex_1().text_size(px(14.0)).text_color(rgb(TEXT)))
                        .child(if cur == st_ { icons::icon("check", 16.0, PRIMARY).into_element() } else { view().into_element() })
                        .into_element()
                })
                .collect::<Vec<_>>(),
        ),
    )
}

// ---------------------------------------------------------------------------
// Sidebar.

fn sidebar_menu(item: Id, x: f32, y: f32) -> impl IntoElement {
    let s = st();
    let is_table = s.peek(|b| b.table(&item).is_some());
    let i1 = item.clone();
    let i2 = item.clone();
    let i3 = item.clone();
    let i4 = item.clone();
    let count = s.peek(|b| b.tables.len());
    popover_at(
        x,
        y,
        200.0,
        close,
        view()
            .child(menu_item("edit", "重命名", move || {
                close();
                sidebar::rename(&i1);
            }))
            .child(menu_item("copy", if is_table { "复制数据表" } else { "复制仪表盘" }, move || {
                close();
                let id = i2.clone();
                let nid = s.edit(|b| {
                    if let Some(t) = b.table(&id).cloned() {
                        let mut t = t;
                        t.id = new_id("tbl");
                        t.name = b.unique_table_name(&format!("{} 副本", t.name));
                        for v in &mut t.views {
                            v.id = new_id("vew");
                        }
                        let nid = t.id.clone();
                        let pos = b.sidebar().iter().position(|x| *x == id).map(|p| p + 1).unwrap_or(b.order.len());
                        let mut order = b.sidebar();
                        order.insert(pos, nid.clone());
                        b.order = order;
                        b.tables.push(t);
                        Some(nid)
                    } else if let Some(d) = b.dashboard(&id).cloned() {
                        let mut d = d;
                        d.id = new_id("dsh");
                        d.name = b.unique_table_name(&format!("{} 副本", d.name));
                        let nid = d.id.clone();
                        b.dashboards.push(d);
                        b.order = b.sidebar();
                        Some(nid)
                    } else {
                        None
                    }
                });
                if let Some(n) = nid {
                    st().open(&n);
                }
            }))
            .child(when(move || is_table, move || {
                let i3 = i3.clone();
                menu_item("download", "导出为 CSV", move || {
                    close();
                    io::export_csv(i3.clone());
                })
            }))
            .child(when(move || !is_table || count > 1, move || {
                let i4 = i4.clone();
                view().child(separator()).child(menu_item_danger("trash", "删除", move || {
                    let name = st().peek(|b| b.table(&i4).map(|t| t.name.clone()).or_else(|| b.dashboard(&i4).map(|d| d.name.clone()))).unwrap_or_default();
                    st().popup.set(Some(Popup::Confirm {
                        title: "删除".into(),
                        text: format!("确定要删除「{name}」吗？"),
                        action: Confirm::DeleteItem(i4.clone()),
                    }))
                }))
            })),
    )
}

fn sidebar_add(x: f32, y: f32) -> impl IntoElement {
    let s = st();
    popover_at(
        x,
        y,
        220.0,
        close,
        view()
            .child(menu_title("新建"))
            .child(menu_item("table", "数据表", move || {
                close();
                let id = s.edit(|b| b.new_table("数据表"));
                st().open(&id);
            }))
            .child(menu_item("dashboard", "仪表盘", move || {
                close();
                let id = s.edit(|b| {
                    let d = crate::model::Dashboard { id: new_id("dsh"), name: b.unique_table_name("仪表盘"), charts: vec![] };
                    let id = d.id.clone();
                    b.dashboards.push(d);
                    b.order.push(id.clone());
                    id
                });
                st().open(&id);
            }))
            .child(separator())
            .child(menu_item("upload", "导入 CSV 为新数据表", move || {
                close();
                io::import_csv_new();
            })),
    )
}

fn more_menu(x: f32, y: f32) -> impl IntoElement {
    let s = st();
    let wrap = move || s.with_view(|_, _, v| v.wrap).unwrap_or(false);
    popover_at(
        x,
        y,
        220.0,
        close,
        view()
            .child(menu_item("download", "导出为 CSV", move || {
                close();
                io::export_csv(st().cur.get_untracked());
            }))
            .child(menu_item("upload", "从 CSV 导入记录", move || {
                close();
                io::import_csv_append();
            }))
            .child(separator())
            .child(menu_item("copy", "复制视图", move || {
                close();
                let vid = st().view.get_untracked();
                let cur = st().cur.get_untracked();
                let nid = st().edit(|b| {
                    let t = b.table_mut(&cur)?;
                    let mut v = t.view(&vid)?.clone();
                    v.id = new_id("vew");
                    v.name = format!("{} 副本", v.name);
                    let id = v.id.clone();
                    t.views.push(v);
                    Some(id)
                });
                if let Some(n) = nid {
                    st().view.set(n);
                }
            }))
            .child(
                view()
                    .h(px(34.0))
                    .mx(px(4.0))
                    .px(px(10.0))
                    .rounded(px(6.0))
                    .flex_row()
                    .items_center()
                    .gap(px(10.0))
                    .hover(|s| s.bg(rgb(HOVER)))
                    .on_press(move || s.edit_view(|v| v.wrap = !v.wrap))
                    .child(icons::icon("wrap", 16.0, TEXT2))
                    .child(text("自动换行").flex_1().text_size(px(14.0)).text_color(rgb(TEXT)))
                    .child(widgets::switch(wrap, move || s.edit_view(|v| v.wrap = !v.wrap))),
            ),
    )
}

// ---------------------------------------------------------------------------
// Dialogs.

fn confirm(title: String, body: String, action: Confirm) -> impl IntoElement {
    let action2 = action;
    modal(
        420.0,
        title,
        close,
        view()
            .px(px(20.0))
            .pb(px(20.0))
            .gap(px(20.0))
            .child(text(body).text_size(px(14.0)).text_color(rgb(TEXT2)))
            .child(
                view()
                    .flex_row()
                    .justify_end()
                    .gap(px(10.0))
                    .child(widgets::button("取消", close))
                    .child(
                        view()
                            .h(px(32.0))
                            .px(px(14.0))
                            .rounded(px(6.0))
                            .bg(rgb(DANGER))
                            .hover(|s| s.opacity(0.9))
                            .justify_center()
                            .on_press(move || {
                                close();
                                run_confirm(action2.clone());
                            })
                            .child(text("删除").text_size(px(14.0)).text_color(rgb(0xFFFFFF))),
                    ),
            ),
    )
}

fn run_confirm(action: Confirm) {
    let s = st();
    match action {
        Confirm::DeleteField(f) => delete_field(&f),
        Confirm::DeleteRecords(ids) => delete_records(&ids),
        Confirm::DeleteView(v) => {
            let cur = s.cur.get_untracked();
            s.edit(|b| {
                if let Some(t) = b.table_mut(&cur) {
                    if t.views.len() > 1 {
                        t.views.retain(|x| x.id != v);
                    }
                }
            });
            s.fix_selection();
        }
        Confirm::DeleteItem(id) => {
            s.edit(|b| {
                if b.tables.len() > 1 || b.table(&id).is_none() {
                    b.tables.retain(|t| t.id != id);
                }
                b.dashboards.retain(|d| d.id != id);
                b.order.retain(|x| *x != id);
                // Links to a deleted table go.
                for t in &mut b.tables {
                    for f in &mut t.fields {
                        if matches!(&f.kind, FieldKind::Link { table, .. } if *table == id) {
                            f.kind = FieldKind::Text;
                        }
                    }
                }
            });
            s.fix_selection();
            s.toast("已删除");
        }
    }
}

fn shortcuts() -> impl IntoElement {
    let rows: &[(&str, &str)] = &[
        ("移动选中单元格", "↑ ↓ ← →"),
        ("扩展选区", "Shift + 方向键"),
        ("移动到边缘", "Ctrl + 方向键"),
        ("编辑单元格", "Enter / F2 / 直接输入"),
        ("确认编辑", "Enter"),
        ("确认并移到右侧", "Tab"),
        ("退出编辑", "Esc"),
        ("单元格内换行", "Shift + Enter"),
        ("在下方插入记录", "Shift + Enter"),
        ("展开记录", "Space"),
        ("清空单元格", "Delete / Backspace"),
        ("复制 / 剪切 / 粘贴", "Ctrl + C / X / V"),
        ("向下填充", "Ctrl + D"),
        ("全选", "Ctrl + A"),
        ("撤销 / 重做", "Ctrl + Z / Ctrl + Y"),
        ("搜索", "Ctrl + F"),
        ("保存", "Ctrl + S"),
        ("快捷键", "Ctrl + /"),
    ];
    modal(
        460.0,
        "快捷键",
        close,
        scroll().max_h(px(560.0)).px(px(20.0)).pb(px(20.0)).child(
            rows.iter()
                .map(|(a, b)| {
                    view()
                        .h(px(36.0))
                        .flex_row()
                        .items_center()
                        .border_b(px(1.0))
                        .border_color(rgb(LINE))
                        .child(text(*a).flex_1().text_size(px(14.0)).text_color(rgb(TEXT)))
                        .child(text(*b).text_size(px(13.0)).text_color(rgb(TEXT2)))
                        .into_element()
                })
                .collect::<Vec<_>>(),
        ),
    )
}

fn workflows() -> impl IntoElement {
    let s = st();
    let adding = signal(false);
    let trig_field = signal(None::<Id>);
    let act_notify = signal(true);
    let act_field = signal(None::<Id>);
    let act_text = signal(String::new());
    let wf_name = signal("新建流程".to_string());
    let fields = move || s.with_table(|_, t| t.fields.iter().map(|f| (f.id.clone(), f.name.clone(), f.kind.clone())).collect::<Vec<_>>()).unwrap_or_default();
    modal(
        620.0,
        "自动化流程",
        close,
        scroll().max_h(px(600.0)).px(px(20.0)).pb(px(20.0)).gap(px(12.0)).child(dynamic(move || {
            let list = s.read(|b| b.workflows.iter().map(|w| (w.id.clone(), w.name.clone(), w.enabled, super::automations::describe(b, w), w.runs, b.table(&w.table).map(|t| t.name.clone()).unwrap_or_default())).collect::<Vec<_>>());
            let mut out: Vec<AnyElement> = vec![];
            if list.is_empty() {
                out.push(text("当前多维表格还没有自动化流程。流程会在新增记录或字段修改时自动执行操作。").text_size(px(13.0)).text_color(rgb(TEXT3)).into_element());
            }
            for (id, name, enabled, desc, runs, tname) in list {
                let id2 = id.clone();
                out.push(
                    view()
                        .p(px(12.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(rgb(BORDER))
                        .flex_row()
                        .items_center()
                        .gap(px(12.0))
                        .child(icons::icon("robot", 20.0, PRIMARY))
                        .child(
                            view()
                                .flex_1()
                                .gap(px(4.0))
                                .child(text(format!("{name} · {tname}")).text_size(px(14.0)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT)))
                                .child(text(desc).text_size(px(12.0)).text_color(rgb(TEXT2)))
                                .child(text(format!("已执行 {runs} 次")).text_size(px(12.0)).text_color(rgb(TEXT3))),
                        )
                        .child(widgets::switch(move || enabled, move || {
                            let id = id.clone();
                            st().edit(|b| {
                                if let Some(w) = b.workflows.iter_mut().find(|w| w.id == id) {
                                    w.enabled = !w.enabled;
                                }
                            })
                        }))
                        .child(
                            view()
                                .size(px(28.0))
                                .rounded(px(6.0))
                                .items_center()
                                .justify_center()
                                .hover(|s| s.bg(rgb(HOVER)))
                                .on_press(move || {
                                    let id = id2.clone();
                                    st().edit(|b| b.workflows.retain(|w| w.id != id))
                                })
                                .child(icons::icon("trash", 16.0, TEXT3)),
                        )
                        .into_element(),
                );
            }
            if adding.get() {
                let fs = fields();
                let fs2 = fs.clone();
                let fs3 = fs.clone();
                let fs4 = fs.clone();
                out.push(
                    view()
                        .p(px(12.0))
                        .rounded(px(8.0))
                        .bg(rgb(BG_GREY))
                        .gap(px(10.0))
                        .child(view().flex_row().items_center().gap(px(8.0)).child(text("名称").w(px(64.0)).text_size(px(13.0)).text_color(rgb(TEXT2))).child(widgets::text_field(wf_name, "流程名称").flex_1()))
                        .child(
                            view().flex_row().items_center().gap(px(8.0)).child(text("触发条件").w(px(64.0)).text_size(px(13.0)).text_color(rgb(TEXT2))).child(widgets::dropdown(
                                move || match trig_field.get() {
                                    None => "新增记录时".into(),
                                    Some(f) => format!("「{}」修改时", fs.iter().find(|x| x.0 == f).map(|x| x.1.clone()).unwrap_or_default()),
                                },
                                move || std::iter::once("新增记录时".to_string()).chain(fs2.iter().map(|f| format!("「{}」修改时", f.1))).collect(),
                                move |i| trig_field.set(if i == 0 { None } else { fs3.get(i - 1).map(|f| f.0.clone()) }),
                                260.0,
                            )),
                        )
                        .child(
                            view().flex_row().items_center().gap(px(8.0)).child(text("执行操作").w(px(64.0)).text_size(px(13.0)).text_color(rgb(TEXT2))).child(widgets::dropdown(
                                move || if act_notify.get() { "发送通知".into() } else { "修改记录".into() },
                                || vec!["发送通知".into(), "修改记录".into()],
                                move |i| act_notify.set(i == 0),
                                120.0,
                            ))
                            .child(when(move || !act_notify.get(), move || {
                                let fs4 = fs4.clone();
                                let fs5 = fs4.clone();
                                widgets::dropdown(
                                    move || act_field.get().and_then(|f| fs4.iter().find(|x| x.0 == f).map(|x| x.1.clone())).unwrap_or("选择字段".into()),
                                    {
                                        let fs = fs5.clone();
                                        move || fs.iter().filter(|f| !f.2.is_computed()).map(|f| f.1.clone()).collect()
                                    },
                                    move |i| act_field.set(fs5.iter().filter(|f| !f.2.is_computed()).nth(i).map(|f| f.0.clone())),
                                    140.0,
                                )
                            }))
                            .child(widgets::text_field(act_text, "内容 / 值").flex_1()),
                        )
                        .child(
                            view().flex_row().justify_end().gap(px(8.0)).child(widgets::button("取消", move || adding.set(false))).child(widgets::primary_button("创建", move || {
                                let s = st();
                                let table = s.cur.get_untracked();
                                if s.peek(|b| b.table(&table).is_none()) {
                                    s.toast("请先打开一个数据表");
                                    return;
                                }
                                let trigger = match trig_field.get_untracked() {
                                    None => Trigger::RecordAdded,
                                    Some(f) => Trigger::FieldChanged { field: f },
                                };
                                let text_v = act_text.get_untracked();
                                let action = if act_notify.get_untracked() {
                                    Action::Notify { text: if text_v.is_empty() { "记录有更新".into() } else { text_v } }
                                } else {
                                    let Some(f) = act_field.get_untracked() else {
                                        s.toast("请选择要修改的字段");
                                        return;
                                    };
                                    let v = s.peek(|b| {
                                        let t = b.table(&table)?;
                                        let mut k = t.field(&f)?.kind.clone();
                                        value::parse(b, &mut k, &text_v)
                                    });
                                    Action::SetField { field: f, value: v.unwrap_or(Value::Text(String::new())) }
                                };
                                let name = wf_name.get_untracked();
                                s.edit(|b| b.workflows.push(Workflow { id: new_id("wfl"), name, enabled: true, table, trigger, action, runs: 0 }));
                                adding.set(false);
                            })),
                        )
                        .into_element(),
                );
            } else {
                out.push(view().flex_row().child(widgets::primary_button("+ 新建流程", move || adding.set(true))).into_element());
            }
            out
        })),
    )
}
