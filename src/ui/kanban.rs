//! The kanban view (看板视图): a column per option of the grouping field,
//! cards dragged between them.

use nsh_core::prelude::*;

use super::{
    cells, icons,
    state::st,
    theme::*,
    widgets,
};
use crate::model::{Field, FieldKind, Id, Value, query, value};

pub const CARD_W: f32 = 272.0;
const GAP: f32 = 12.0;
const PAD: f32 = 16.0;

/// A record's card: its cover, title and the fields shown.
pub fn card(table: Id, record: Id, width: f32, cover_h: f32) -> AnyElement {
    let s = st();
    let (t1, r1) = (table.clone(), record.clone());
    let info = s.with_view(|b, t, v| {
        let r = t.record(&record)?;
        let fields: Vec<Field> = t.view_fields(v).into_iter().filter(|(_, h)| !h).map(|(f, _)| f.clone()).collect();
        let cover = v.cover_field.as_ref().and_then(|cf| match r.cells.get(cf) {
            Some(Value::Files(f)) => f.iter().find(|a| a.is_image()).cloned(),
            _ => None,
        });
        let title = value::record_title(b, t, &r.id);
        let rows: Vec<(Field, Option<Value>)> = fields.iter().skip(1).filter(|f| v.group_field.as_ref() != Some(&f.id)).take(5).map(|f| (f.clone(), query::cell(b, t, r, f))).collect();
        Some((title, cover, rows, v.cover_field.is_some(), v.compact))
    }).flatten();
    let Some((title, cover, rows, has_cover, compact)) = info else { return view().into_element() };
    let inner = width - 24.0;
    view()
        .w(px(width))
        .bg(rgb(BG))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgb(BORDER))
        .overflow_hidden()
        .hover(|s| s.shadow_md())
        .on_press(move || st().expanded.set(Some((t1.clone(), r1.clone()))))
        .child(if has_cover && cover_h > 0.0 {
            match cover {
                Some(a) => view()
                    .h(px(cover_h))
                    .bg(rgb(BG_GREY))
                    .child(image(cells::file_url(&a.path)).w_full().h(px(cover_h)).resize_mode(ResizeMode::Cover))
                    .into_element(),
                None => view().h(px(cover_h)).bg(rgb(BG_GREY)).items_center().justify_center().child(icons::icon("image", 32.0, 0xC9CDD4)).into_element(),
            }
        } else {
            view().into_element()
        })
        .child(
            view()
                .px(px(12.0))
                .py(px(10.0))
                .gap(px(6.0))
                .child(text(title).text_size(px(14.0)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT)).number_of_lines(2u32))
                .child(
                    rows.into_iter()
                        .filter(|(_, v)| v.as_ref().is_some_and(|v| !v.is_empty()))
                        .map(|(f, v)| {
                            let (tid, k) = (table.clone(), f.kind.clone());
                            view()
                                .gap(px(2.0))
                                .child(if compact { view().into_element() } else { text(f.name.clone()).text_size(px(11.0)).text_color(rgb(TEXT3)).into_element() })
                                .child(
                                    canvas(move || {
                                        st().peek(|b| {
                                            let t = b.table(&tid).unwrap();
                                            cells::draw_value(Painter::new(), b, t, &k, v.as_ref(), -8.0, 0.0, inner + 16.0, 24.0, 1)
                                        })
                                    })
                                    .w(px(inner))
                                    .h(px(24.0)),
                                )
                                .into_element()
                        })
                        .collect::<Vec<_>>(),
                ),
        )
        .into_element()
}

pub fn kanban() -> impl IntoElement {
    let s = st();
    let scroll_x = signal(0.0f32);
    let origin = signal((0.0f32, 0.0f32));
    let board = element_ref();
    let drag = signal(None::<(Id, usize)>);
    let over = signal(None::<usize>);
    // The columns: (option id or None, label, color).
    let columns = move || -> Vec<(Option<Id>, String, u8, Vec<Id>)> {
        s.with_view(|b, t, v| {
            let Some(gf) = v.group_field.as_ref().and_then(|g| t.field(g)) else { return vec![] };
            let idx = query::records(b, t, v);
            let mut cols: Vec<(Option<Id>, String, u8, Vec<Id>)> = match &gf.kind {
                FieldKind::SingleSelect { options } => options.iter().map(|o| (Some(o.id.clone()), o.name.clone(), o.color, vec![])).collect(),
                FieldKind::User { .. } => b.members.iter().map(|m| (Some(m.id.clone()), m.name.clone(), m.color, vec![])).collect(),
                _ => vec![],
            };
            cols.insert(0, (None, "未分组".into(), 10, vec![]));
            let search = st().search.get().to_lowercase();
            for i in idx {
                let r = &t.records[i];
                if !search.is_empty() {
                    let fields: Vec<&Field> = t.fields.iter().collect();
                    if !query::found(b, t, r, &fields, &search) {
                        continue;
                    }
                }
                let key = r.cells.get(&gf.id).and_then(|v| v.ids().first().cloned());
                match cols.iter_mut().find(|c| c.0 == key) {
                    Some(c) => c.3.push(r.id.clone()),
                    None => cols[0].3.push(r.id.clone()),
                }
            }
            // An empty "未分组" column is not shown.
            if cols[0].3.is_empty() {
                cols.remove(0);
            }
            cols.retain(|c| !v.hidden_groups.contains(c.0.as_ref().unwrap_or(&String::new())));
            cols
        })
        .unwrap_or_default()
    };
    let group_fields = move || {
        s.with_table(|_, t| {
            t.fields
                .iter()
                .filter(|f| matches!(f.kind, FieldKind::SingleSelect { .. } | FieldKind::User { multiple: false }))
                .map(|f| (f.id.clone(), f.name.clone()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
    };
    let col_at = move |page_x: f32| -> usize {
        let (ox, _) = origin.get_untracked();
        ((page_x - ox + scroll_x.get_untracked() - PAD) / (CARD_W + 8.0 * 2.0 + GAP)).floor().max(0.0) as usize
    };
    let drop_to = move |rid: Id, col: usize| {
        let cols = untracked(columns);
        let Some((key, _, _, _)) = cols.get(col) else { return };
        let s = st();
        let gf = s.with_view(|_, t, v| v.group_field.as_ref().and_then(|g| t.field(g)).map(|f| (f.id.clone(), f.kind.clone()))).flatten();
        if let Some((fid, kind)) = gf {
            let v = key.clone().map(|k| if matches!(kind, FieldKind::User { .. }) { Value::Users(vec![k]) } else { Value::Options(vec![k]) });
            s.set_cell(&rid, &fid, v);
        }
    };

    view()
        .flex_1()
        .bg(rgb(BG_GREY))
        .child(
            view()
                .h(px(40.0))
                .flex_row()
                .items_center()
                .px(px(PAD))
                .gap(px(8.0))
                .child(text("分组依据").text_size(px(13.0)).text_color(rgb(TEXT2)))
                .child(widgets::dropdown(
                    move || {
                        s.with_view(|_, t, v| v.group_field.as_ref().and_then(|g| t.field(g)).map(|f| f.name.clone()))
                            .flatten()
                            .unwrap_or("选择单选或人员字段".into())
                    },
                    move || group_fields().into_iter().map(|f| f.1).collect(),
                    move |i| {
                        if let Some((id, _)) = group_fields().get(i).cloned() {
                            st().edit_view(|v| v.group_field = Some(id));
                        }
                    },
                    200.0,
                ))
                .child(view().w(px(16.0)))
                .child(text("紧凑模式").text_size(px(13.0)).text_color(rgb(TEXT2)))
                .child(widgets::switch(move || s.with_view(|_, _, v| v.compact).unwrap_or(false), move || st().edit_view(|v| v.compact = !v.compact))),
        )
        .child(dynamic(move || {
            if s.with_view(|_, t, v| v.group_field.as_ref().and_then(|g| t.field(g)).is_none()).unwrap_or(true) {
                return view()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .child(text("请选择一个单选或人员字段作为看板分组依据").text_size(px(14.0)).text_color(rgb(TEXT3)))
                    .into_element();
            }
            scroll()
                .element_ref(&board)
                .horizontal()
                .flex_1()
                .on_scroll(move |e: &ScrollEvent| scroll_x.set(e.offset.x.0))
                .child(
                    view()
                        .flex_row()
                        .px(px(PAD))
                        .pb(px(PAD))
                        .gap(px(GAP))
                        .items_start()
                        .child(dynamic(move || {
                            let tid = st().cur.get();
                            columns()
                                .into_iter()
                                .enumerate()
                                .map(|(ci, (key, label, color, ids))| {
                                    let n = ids.len();
                                    let key2 = key.clone();
                                    let tid2 = tid.clone();
                                    let (bg, fg) = value::tag_color(color);
                                    view()
                                        .w(px(CARD_W + 16.0))
                                        .max_h(px(2000.0))
                                        .rounded(px(10.0))
                                        .bg(move || if over.get() == Some(ci) && drag.get().is_some() { rgb(0xE1EAFF) } else { rgb(0xEFF0F1) })
                                        .p(px(8.0))
                                        .gap(px(8.0))
                                        .child(
                                            view()
                                                .h(px(32.0))
                                                .flex_row()
                                                .items_center()
                                                .gap(px(8.0))
                                                .px(px(4.0))
                                                .child(if key.is_some() { widgets::tag_el(&label, bg, fg).into_element() } else { text(label.clone()).text_size(px(14.0)).text_color(rgb(TEXT2)).into_element() })
                                                .child(text(n.to_string()).text_size(px(13.0)).text_color(rgb(TEXT3)))
                                                .child(view().flex_1())
                                                .child(
                                                    view()
                                                        .size(px(24.0))
                                                        .rounded(px(4.0))
                                                        .items_center()
                                                        .justify_center()
                                                        .hover(|s| s.bg(rgb(0xDEE0E3)))
                                                        .on_press({
                                                            let key = key2.clone();
                                                            let tid = tid2.clone();
                                                            move || add_in(&tid, key.clone())
                                                        })
                                                        .child(icons::icon("plus", 16.0, TEXT2)),
                                                ),
                                        )
                                        .child(
                                            ids.into_iter()
                                                .map(|rid| {
                                                    let r2 = rid.clone();
                                                    let dragging = {
                                                        let rid = rid.clone();
                                                        move || drag.get().is_some_and(|(d, _)| d == rid)
                                                    };
                                                    view()
                                                        .opacity(move || if dragging() { 0.4 } else { 1.0 })
                                                        .claim_touches(TouchClaim::on_move())
                                                        .on_responder_grant(move |e: &ResponderEvent| {
                                                            drag.set(Some((r2.clone(), ci)));
                                                            over.set(Some(ci));
                                                            let _ = e;
                                                            spawn(async move {
                                                                if let Some(m) = board.measure().await {
                                                                    origin.set((m.page.x.0, m.page.y.0));
                                                                }
                                                            });
                                                        })
                                                        .on_responder_move(move |e: &ResponderEvent| {
                                                            let c = col_at(e.page.x.0);
                                                            if over.get_untracked() != Some(c) {
                                                                over.set(Some(c));
                                                            }
                                                        })
                                                        .on_responder_release(move |e: &ResponderEvent| {
                                                            if let Some((rid, from)) = drag.get_untracked() {
                                                                let to = col_at(e.page.x.0);
                                                                drag.set(None);
                                                                over.set(None);
                                                                if to != from {
                                                                    drop_to(rid, to);
                                                                }
                                                            }
                                                        })
                                                        .on_responder_terminate(move |_: &ResponderEvent| {
                                                            drag.set(None);
                                                            over.set(None);
                                                        })
                                                        .child(card(tid.clone(), rid, CARD_W, 140.0))
                                                        .into_element()
                                                })
                                                .collect::<Vec<_>>(),
                                        )
                                        .child(
                                            view()
                                                .h(px(34.0))
                                                .rounded(px(6.0))
                                                .flex_row()
                                                .items_center()
                                                .gap(px(6.0))
                                                .px(px(8.0))
                                                .hover(|s| s.bg(rgb(0xE4E5E7)))
                                                .on_press(move || add_in(&tid2, key2.clone()))
                                                .child(icons::icon("plus", 14.0, TEXT3))
                                                .child(text("添加记录").text_size(px(13.0)).text_color(rgb(TEXT3))),
                                        )
                                        .into_element()
                                })
                                .collect::<Vec<_>>()
                        })),
                )
                .into_element()
        }))
}

/// A new record in a column, opened in the record card.
fn add_in(table: &str, key: Option<Id>) {
    let s = st();
    let gf = s.with_view(|_, t, v| v.group_field.as_ref().and_then(|g| t.field(g)).map(|f| (f.id.clone(), f.kind.clone()))).flatten();
    let table = table.to_string();
    let id = s.edit(|b| {
        let me = b.me.clone();
        let t = b.table_mut(&table)?;
        let mut r = t.new_record(&me);
        if let (Some((fid, kind)), Some(k)) = (gf, key) {
            r.cells.insert(fid, if matches!(kind, FieldKind::User { .. }) { Value::Users(vec![k]) } else { Value::Options(vec![k]) });
        }
        let id = r.id.clone();
        t.records.push(r);
        crate::ui::automations::on_add(b, &table, &id);
        Some(id)
    });
    if let Some(id) = id {
        s.expanded.set(Some((table, id)));
    }
}
