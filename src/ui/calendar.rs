//! The calendar view (日历视图): records on the days of a month, from their
//! start to their end date; drag one to another day.

use chrono::Datelike;
use nsh_core::prelude::*;

use super::{icons, state::st, theme::*, widgets};
use crate::model::{FieldKind, Id, Value, query, value};

struct Item {
    id: Id,
    title: String,
    start: i64,
    end: i64,
    color: u8,
}

fn dates_of(start_f: &Option<Id>, end_f: &Option<Id>) -> Vec<Item> {
    st().with_view(|b, t, v| {
        let Some(sf) = start_f.as_ref().and_then(|f| t.field(f)) else { return vec![] };
        let ef = end_f.as_ref().and_then(|f| t.field(f));
        // A single select's color colors the events.
        let color_field = t.fields.iter().find(|f| matches!(f.kind, FieldKind::SingleSelect { .. }));
        query::records(b, t, v)
            .into_iter()
            .filter_map(|i| {
                let r = &t.records[i];
                let s = match query::cell(b, t, r, sf) {
                    Some(Value::Date(d)) => d,
                    _ => return None,
                };
                let e = ef.and_then(|f| match query::cell(b, t, r, f) {
                    Some(Value::Date(d)) => Some(d),
                    _ => None,
                }).filter(|e| *e >= s).unwrap_or(s);
                let color = color_field
                    .and_then(|cf| r.cells.get(&cf.id).and_then(|v| v.ids().first().cloned()).and_then(|oid| cf.kind.options().and_then(|o| o.iter().find(|x| x.id == oid)).map(|o| o.color)))
                    .unwrap_or(0);
                Some(Item { id: r.id.clone(), title: value::record_title(b, t, &r.id), start: value::day_start(s), end: value::day_start(e), color })
            })
            .collect()
    })
    .unwrap_or_default()
}

pub fn calendar_view() -> impl IntoElement {
    let s = st();
    let now = value::local(value::today_ms());
    let month = signal(now.year() * 12 + now.month() as i32 - 1);
    let drag = signal(None::<(Id, i64)>);
    let over = signal(None::<i64>);
    let date_fields = move || s.with_table(|_, t| t.fields.iter().filter(|f| matches!(f.kind, FieldKind::Date { .. })).map(|f| (f.id.clone(), f.name.clone())).collect::<Vec<_>>()).unwrap_or_default();
    view()
        .flex_1()
        .bg(rgb(BG))
        .child(
            view()
                .h(px(48.0))
                .flex_row()
                .items_center()
                .px(px(16.0))
                .gap(px(8.0))
                .child(text(move || {
                    let m = month.get();
                    format!("{} 年 {} 月", m.div_euclid(12), m.rem_euclid(12) + 1)
                }).text_size(px(18.0)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT)).w(px(140.0)))
                .child(view().size(px(28.0)).rounded(px(6.0)).items_center().justify_center().hover(|s| s.bg(rgb(HOVER))).on_press(move || month.update(|m| *m -= 1)).child(icons::icon("chevron-left", 16.0, TEXT2)))
                .child(widgets::button("今天", move || month.set(now.year() * 12 + now.month() as i32 - 1)))
                .child(view().size(px(28.0)).rounded(px(6.0)).items_center().justify_center().hover(|s| s.bg(rgb(HOVER))).on_press(move || month.update(|m| *m += 1)).child(icons::icon("chevron-right", 16.0, TEXT2)))
                .child(view().flex_1())
                .child(text("开始日期").text_size(px(13.0)).text_color(rgb(TEXT2)))
                .child(widgets::dropdown(
                    move || s.with_view(|_, t, v| v.start_field.as_ref().and_then(|f| t.field(f)).map(|f| f.name.clone())).flatten().unwrap_or("选择日期字段".into()),
                    move || date_fields().into_iter().map(|f| f.1).collect(),
                    move |i| {
                        let id = date_fields().get(i).map(|f| f.0.clone());
                        st().edit_view(|v| v.start_field = id);
                    },
                    140.0,
                ))
                .child(text("结束日期").text_size(px(13.0)).text_color(rgb(TEXT2)))
                .child(widgets::dropdown(
                    move || s.with_view(|_, t, v| v.end_field.as_ref().and_then(|f| t.field(f)).map(|f| f.name.clone())).flatten().unwrap_or("无".into()),
                    move || std::iter::once("无".to_string()).chain(date_fields().into_iter().map(|f| f.1)).collect(),
                    move |i| {
                        let id = if i == 0 { None } else { date_fields().get(i - 1).map(|f| f.0.clone()) };
                        st().edit_view(|v| v.end_field = id);
                    },
                    140.0,
                )),
        )
        .child(
            view().flex_row().border_t(px(1.0)).border_b(px(1.0)).border_color(rgb(BORDER)).child(
                ["周一", "周二", "周三", "周四", "周五", "周六", "周日"]
                    .iter()
                    .map(|d| view().flex_1().h(px(32.0)).items_center().justify_center().child(text(*d).text_size(px(12.0)).text_color(rgb(TEXT2))).into_element())
                    .collect::<Vec<_>>(),
            ),
        )
        .child(dynamic(move || {
            let m = month.get();
            let (sf, ef) = s.with_view(|_, _, v| (v.start_field.clone(), v.end_field.clone())).unwrap_or((None, None));
            let items = dates_of(&sf, &ef);
            let (y, mo) = (m.div_euclid(12), m.rem_euclid(12) as u32 + 1);
            let first = value::date_ms(y, mo, 1);
            let lead = value::local(first).weekday().num_days_from_monday() as i64;
            let start = first - lead * value::DAY_MS;
            let today = value::today_ms();
            let weeks = (0..6)
                .map(|w| {
                    view()
                        .flex_1()
                        .flex_row()
                        .border_b(px(1.0))
                        .border_color(rgb(LINE))
                        .child(
                            (0..7)
                                .map(|d| {
                                    let day = value::day_start(start + (w * 7 + d) * value::DAY_MS + 3 * 3_600_000);
                                    let dt = value::local(day);
                                    let in_month = dt.month() == mo;
                                    let on_day: Vec<&Item> = items.iter().filter(|it| it.start <= day && it.end >= day).collect();
                                    let more = on_day.len().saturating_sub(3);
                                    let sf2 = sf.clone();
                                    view()
                                        .flex_1()
                                        .border_r(px(1.0))
                                        .border_color(rgb(LINE))
                                        .bg(move || if over.get() == Some(day) { rgb(0xE1EAFF) } else if in_month { rgb(BG) } else { rgb(0xFAFAFB) })
                                        .p(px(4.0))
                                        .gap(px(2.0))
                                        .on_hover_in(move || {
                                            if drag.get_untracked().is_some() {
                                                over.set(Some(day));
                                            }
                                        })
                                        .on_press(move || {
                                            if let Some((rid, from)) = drag.get_untracked() {
                                                drag.set(None);
                                                over.set(None);
                                                move_dates(&rid, day - from);
                                                return;
                                            }
                                            add_on(day, sf2.clone());
                                        })
                                        .child(
                                            view().flex_row().justify_end().child(
                                                view()
                                                    .size(px(22.0))
                                                    .rounded(px(11.0))
                                                    .items_center()
                                                    .justify_center()
                                                    .bg(if day == today { rgb(PRIMARY) } else { rgba(0xFFFFFF00) })
                                                    .child(text(dt.day().to_string()).text_size(px(12.0)).text_color(rgb(if day == today { 0xFFFFFF } else if in_month { TEXT } else { 0xC9CDD4 }))),
                                            ),
                                        )
                                        .child(
                                            on_day
                                                .iter()
                                                .take(3)
                                                .map(|it| {
                                                    let (bg, fg) = value::tag_color(it.color);
                                                    let rid = it.id.clone();
                                                    let rid2 = it.id.clone();
                                                    let tid = st().cur.get_untracked();
                                                    view()
                                                        .h(px(22.0))
                                                        .px(px(6.0))
                                                        .rounded(px(4.0))
                                                        .bg(rgb(bg))
                                                        .border_l(px(3.0))
                                                        .border_color(rgb(value::strong_color(it.color)))
                                                        .justify_center()
                                                        .on_press(move || {
                                                            if drag.get_untracked().is_some() {
                                                                return;
                                                            }
                                                            st().expanded.set(Some((tid.clone(), rid.clone())))
                                                        })
                                                        .on_long_press(move || {
                                                            drag.set(Some((rid2.clone(), day)));
                                                            st().toast("点击目标日期以移动记录");
                                                        })
                                                        .child(text(it.title.clone()).text_size(px(12.0)).text_color(rgb(fg)).number_of_lines(1u32))
                                                        .into_element()
                                                })
                                                .collect::<Vec<_>>(),
                                        )
                                        .child(if more > 0 { text(format!("还有 {more} 条")).text_size(px(11.0)).text_color(rgb(TEXT3)).into_element() } else { view().into_element() })
                                        .into_element()
                                })
                                .collect::<Vec<_>>(),
                        )
                        .into_element()
                })
                .collect::<Vec<_>>();
            view().flex_1().child(weeks).into_element()
        }))
}

fn add_on(day: i64, start_field: Option<Id>) {
    let s = st();
    let Some(sf) = start_field else {
        s.toast("请先选择开始日期字段");
        return;
    };
    let ef = s.with_view(|_, _, v| v.end_field.clone()).flatten();
    let table = s.cur.get_untracked();
    let id = s.edit(|b| {
        let me = b.me.clone();
        let t = b.table_mut(&table)?;
        let mut r = t.new_record(&me);
        r.cells.insert(sf.clone(), Value::Date(day));
        if let Some(e) = ef {
            r.cells.insert(e, Value::Date(day));
        }
        let id = r.id.clone();
        t.records.push(r);
        Some(id)
    });
    if let Some(id) = id {
        s.expanded.set(Some((table, id)));
    }
}

/// Moves a record's start and end by `delta` milliseconds.
pub fn move_dates(rid: &str, delta: i64) {
    let s = st();
    if delta == 0 {
        return;
    }
    let (sf, ef) = s.with_view(|_, _, v| (v.start_field.clone(), v.end_field.clone())).unwrap_or((None, None));
    let table = s.cur.get_untracked();
    let cells = s.peek(|b| {
        let r = b.table(&table)?.record(rid)?;
        let mut out = vec![];
        for f in [sf.clone(), ef.clone()].into_iter().flatten().collect::<std::collections::BTreeSet<_>>() {
            if let Some(Value::Date(d)) = r.cells.get(&f) {
                out.push((rid.to_string(), f.clone(), Some(Value::Date(d + delta))));
            }
        }
        Some(out)
    });
    if let Some(c) = cells {
        s.set_cells(&table, &c);
    }
}
