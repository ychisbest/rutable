//! The gantt view (甘特视图): records as bars from their start to their end
//! date; drag a bar to move it, its ends to change its dates.

use chrono::Datelike;
use nsh_core::{
    canvas::{CanvasTextAlign, CanvasTextBaseline},
    prelude::*,
};

use super::{
    cells::{self, ellipsize, rrect, text_at},
    state::st,
    theme::*,
    widgets,
};
use crate::model::{FieldKind, Id, Value, query, value};

const ROW: f32 = 40.0;
const HEAD: f32 = 56.0;
const LIST_W: f32 = 260.0;

#[derive(Clone, PartialEq)]
struct Bar {
    id: Id,
    title: String,
    start: Option<i64>,
    end: Option<i64>,
    progress: Option<f64>,
    color: u8,
}

#[derive(Clone, Copy, PartialEq)]
enum Grab {
    Move,
    Start,
    End,
}

fn bars() -> Vec<Bar> {
    st().with_view(|b, t, v| {
        let sf = v.start_field.as_ref().and_then(|f| t.field(f));
        let ef = v.end_field.as_ref().and_then(|f| t.field(f));
        let pf = t.fields.iter().find(|f| matches!(f.kind, FieldKind::Progress { .. }));
        let cf = t.fields.iter().find(|f| matches!(f.kind, FieldKind::SingleSelect { .. }));
        let search = st().search.get().to_lowercase();
        let all: Vec<&crate::model::Field> = t.fields.iter().collect();
        query::records(b, t, v)
            .into_iter()
            .filter(|&i| search.is_empty() || query::found(b, t, &t.records[i], &all, &search))
            .map(|i| {
                let r = &t.records[i];
                let date = |f: Option<&crate::model::Field>| f.and_then(|f| match query::cell(b, t, r, f) {
                    Some(Value::Date(d)) => Some(value::day_start(d)),
                    _ => None,
                });
                let start = date(sf);
                let end = date(ef).or(start);
                let color = cf
                    .and_then(|c| r.cells.get(&c.id).and_then(|v| v.ids().first().cloned()).and_then(|oid| c.kind.options().and_then(|o| o.iter().find(|x| x.id == oid)).map(|o| o.color)))
                    .unwrap_or(0);
                Bar {
                    id: r.id.clone(),
                    title: value::record_title(b, t, &r.id),
                    start,
                    end: end.map(|e| e.max(start.unwrap_or(e))),
                    progress: pf.and_then(|p| r.cells.get(&p.id).and_then(|v| v.as_number())),
                    color,
                }
            })
            .collect()
    })
    .unwrap_or_default()
}

pub fn gantt() -> impl IntoElement {
    let s = st();
    let day_w = signal(36.0f32);
    let sx = signal(0.0f32);
    let sy = signal(0.0f32);
    let size = signal((0.0f32, 0.0f32));
    let data = computed(bars);
    // The first day shown: two weeks before the earliest start, or before today.
    let origin_day = computed(move || {
        let bs = data.get();
        let min = bs.iter().filter_map(|b| b.start).min().unwrap_or_else(value::today_ms);
        value::day_start(min.min(value::today_ms()) - 7 * value::DAY_MS)
    });
    let drag = signal(None::<(usize, Grab, f32, i64, i64)>);
    let delta_days = signal(0i64);
    let hover = signal(None::<usize>);
    let date_fields = move || s.with_table(|_, t| t.fields.iter().filter(|f| matches!(f.kind, FieldKind::Date { .. })).map(|f| (f.id.clone(), f.name.clone())).collect::<Vec<_>>()).unwrap_or_default();

    let paint = move || {
        let (w, h) = size.get();
        let dw = day_w.get();
        let bs = data.get();
        let x0 = sx.get();
        let y0 = sy.get();
        let o = origin_day.get();
        let dd = delta_days.get();
        let dr = drag.get();
        let hv = hover.get();
        let mut p = Painter::new().fill_color(rgb(BG)).fill_rect(0.0, 0.0, w, h);
        let today = value::today_ms();
        let first = (x0 / dw).floor() as i64;
        let last = ((x0 + w - LIST_W) / dw).ceil() as i64 + 1;
        // Day columns and weekends.
        for d in first..last {
            let ms = o + d * value::DAY_MS;
            let x = LIST_W + d as f32 * dw - x0;
            let dt = value::local(ms + 3 * 3_600_000);
            if dt.weekday().number_from_monday() >= 6 {
                p = p.fill_color(rgb(0xFAFAFB)).fill_rect(x, HEAD, dw, h - HEAD);
            }
            p = p.fill_color(rgb(LINE)).fill_rect(x, HEAD - 24.0, 1.0, h);
        }
        // Rows.
        for (i, b) in bs.iter().enumerate() {
            let y = HEAD + i as f32 * ROW - y0;
            if y + ROW < HEAD || y > h {
                continue;
            }
            if hv == Some(i) {
                p = p.fill_color(rgb(0xF7F8FA)).fill_rect(0.0, y, w, ROW);
            }
            p = p.fill_color(rgb(LINE)).fill_rect(0.0, y + ROW - 1.0, w, 1.0);
            let (mut s0, mut e0) = (b.start, b.end);
            if let Some((di, grab, _, os, oe)) = dr {
                if di == i {
                    match grab {
                        Grab::Move => {
                            s0 = Some(os + dd * value::DAY_MS);
                            e0 = Some(oe + dd * value::DAY_MS);
                        }
                        Grab::Start => s0 = Some((os + dd * value::DAY_MS).min(oe)),
                        Grab::End => e0 = Some((oe + dd * value::DAY_MS).max(os)),
                    }
                }
            }
            if let (Some(s1), Some(e1)) = (s0, e0) {
                let bx = LIST_W + ((s1 - o) / value::DAY_MS) as f32 * dw - x0;
                let bw = (((e1 - s1) / value::DAY_MS) as f32 + 1.0) * dw;
                p = cells::clip(p, LIST_W, HEAD, w - LIST_W, h - HEAD);
                let strong = value::strong_color(b.color);
                let (light, _) = value::tag_color(b.color);
                p = rrect(p, bx + 2.0, y + 8.0, bw - 4.0, ROW - 16.0, 5.0, light);
                if let Some(pr) = b.progress {
                    p = rrect(p, bx + 2.0, y + 8.0, ((bw - 4.0) * pr.clamp(0.0, 1.0) as f32).max(0.0), ROW - 16.0, 5.0, strong);
                }
                p = p.fill_color(rgb(strong)).fill_rect(bx + 2.0, y + 8.0, 3.0, ROW - 16.0);
                p = text_at(p, &ellipsize(&b.title, (bw - 16.0).max(0.0), 12.0), bx + 10.0, y + ROW / 2.0, 12.0, TEXT);
                if hv == Some(i) {
                    p = rrect(p, bx + 2.0, y + 12.0, 4.0, ROW - 24.0, 2.0, strong);
                    p = rrect(p, bx + bw - 6.0, y + 12.0, 4.0, ROW - 24.0, 2.0, strong);
                }
                p = p.restore();
            }
            // The list column.
            p = p.fill_color(rgb(if hv == Some(i) { 0xF7F8FA } else { BG })).fill_rect(0.0, y, LIST_W, ROW - 1.0);
            p = text_at(p, &ellipsize(&b.title, LIST_W - 24.0, 14.0), 16.0, y + ROW / 2.0, 14.0, TEXT);
        }
        // Today.
        let tx = LIST_W + ((today - o) / value::DAY_MS) as f32 * dw - x0 + dw / 2.0;
        if tx > LIST_W {
            p = p.fill_color(rgb(PRIMARY)).fill_rect(tx, HEAD - 4.0, 1.5, h);
        }
        // Header: months and days.
        p = p.fill_color(rgb(BG_GREY)).fill_rect(LIST_W, 0.0, w, HEAD);
        p = cells::clip(p, LIST_W, 0.0, w - LIST_W, HEAD);
        for d in first..last {
            let ms = o + d * value::DAY_MS;
            let dt = value::local(ms + 3 * 3_600_000);
            let x = LIST_W + d as f32 * dw - x0;
            if dt.day() == 1 || d == first {
                p = text_at(p, &format!("{} 年 {} 月", dt.year(), dt.month()), x + 6.0, 16.0, 12.0, TEXT2);
                p = p.fill_color(rgb(BORDER)).fill_rect(x, 0.0, 1.0, 28.0);
            }
            let is_today = value::day_start(ms) == today;
            p = p
                .font(&font(11.0))
                .fill_color(rgb(if is_today { PRIMARY } else { TEXT3 }))
                .text_align(CanvasTextAlign::Center)
                .text_baseline(CanvasTextBaseline::Middle)
                .fill_text(&dt.day().to_string(), x + dw / 2.0, 42.0);
        }
        p = p.restore();
        p = p.fill_color(rgb(BG_GREY)).fill_rect(0.0, 0.0, LIST_W, HEAD);
        p = text_at(p, "记录", 16.0, HEAD / 2.0, 13.0, TEXT2);
        p = p.fill_color(rgb(BORDER)).fill_rect(0.0, HEAD - 1.0, w, 1.0);
        p.fill_color(rgb(BORDER)).fill_rect(LIST_W - 1.0, 0.0, 1.0, h)
    };

    let hit = move |x: f32, y: f32| -> Option<(usize, Option<Grab>)> {
        if y < HEAD {
            return None;
        }
        let i = ((y - HEAD + sy.get_untracked()) / ROW) as usize;
        let bs = data.get_untracked();
        let b = bs.get(i)?;
        if x < LIST_W {
            return Some((i, None));
        }
        let (Some(s0), Some(e0)) = (b.start, b.end) else { return Some((i, None)) };
        let dw = day_w.get_untracked();
        let o = origin_day.get_untracked();
        let bx = LIST_W + ((s0 - o) / value::DAY_MS) as f32 * dw - sx.get_untracked();
        let bw = (((e0 - s0) / value::DAY_MS) as f32 + 1.0) * dw;
        if x >= bx && x <= bx + 8.0 {
            Some((i, Some(Grab::Start)))
        } else if x >= bx + bw - 8.0 && x <= bx + bw {
            Some((i, Some(Grab::End)))
        } else if x > bx && x < bx + bw {
            Some((i, Some(Grab::Move)))
        } else {
            Some((i, None))
        }
    };
    let last_press = stored(0i64);
    super::wheel::take(move |dx, dy| {
        let (w, h) = size.get_untracked();
        let n = data.get_untracked().len() as f32;
        let max_y = (n * ROW + 40.0 - (h - HEAD)).max(0.0);
        let span = 400.0 * day_w.get_untracked();
        let max_x = (span - (w - LIST_W)).max(0.0);
        sx.set((sx.get_untracked() + dx).clamp(0.0, max_x));
        sy.set((sy.get_untracked() + dy).clamp(0.0, max_y));
    });
    effect(move || crate::win::set_grid_wheel(st().popup.get().is_none() && st().expanded.get().is_none()));

    view()
        .flex_1()
        .child(
            view()
                .h(px(40.0))
                .flex_row()
                .items_center()
                .px(px(16.0))
                .gap(px(8.0))
                .border_b(px(1.0))
                .border_color(rgb(BORDER))
                .child(text("开始日期").text_size(px(13.0)).text_color(rgb(TEXT2)))
                .child(widgets::dropdown(
                    move || s.with_view(|_, t, v| v.start_field.as_ref().and_then(|f| t.field(f)).map(|f| f.name.clone())).flatten().unwrap_or("选择".into()),
                    move || date_fields().into_iter().map(|f| f.1).collect(),
                    move |i| {
                        let id = date_fields().get(i).map(|f| f.0.clone());
                        st().edit_view(|v| v.start_field = id);
                    },
                    130.0,
                ))
                .child(text("结束日期").text_size(px(13.0)).text_color(rgb(TEXT2)))
                .child(widgets::dropdown(
                    move || s.with_view(|_, t, v| v.end_field.as_ref().and_then(|f| t.field(f)).map(|f| f.name.clone())).flatten().unwrap_or("选择".into()),
                    move || date_fields().into_iter().map(|f| f.1).collect(),
                    move |i| {
                        let id = date_fields().get(i).map(|f| f.0.clone());
                        st().edit_view(|v| v.end_field = id);
                    },
                    130.0,
                ))
                .child(view().flex_1())
                .child(widgets::dropdown(
                    move || match day_w.get() as i32 {
                        16 => "按月".into(),
                        36 => "按周".into(),
                        _ => "按日".into(),
                    },
                    || vec!["按日".into(), "按周".into(), "按月".into()],
                    move |i| day_w.set([64.0, 36.0, 16.0][i]),
                    90.0,
                ))
                .child(widgets::button("今天", move || {
                    let o = origin_day.get_untracked();
                    let x = ((value::today_ms() - o) / value::DAY_MS) as f32 * day_w.get_untracked() - 200.0;
                    sx.set(x.max(0.0));
                })),
        )
        .child(
            canvas(paint)
                .flex_1()
                .on_layout(move |b: &Bounds<Points>| size.set((b.size.width.0, b.size.height.0)))
                .claim_touches(TouchClaim::on_start())
                .on_hover_move(move |e: &HoverEvent| {
                    let (w, hh) = size.get_untracked();
                    crate::win::set_grid_rect(e.page_position.x.0 - e.position.x.0, e.page_position.y.0 - e.position.y.0, w, hh);
                    let h = hit(e.position.x.0, e.position.y.0).map(|x| x.0);
                    if hover.get_untracked() != h {
                        hover.set(h);
                    }
                })
                .on_hover_out(move || hover.set(None))
                .on_responder_grant(move |e: &ResponderEvent| {
                    let (x, y) = (e.location.x.0, e.location.y.0);
                    match hit(x, y) {
                        Some((i, Some(g))) => {
                            let b = data.get_untracked()[i].clone();
                            drag.set(Some((i, g, x, b.start.unwrap_or(0), b.end.unwrap_or(0))));
                            delta_days.set(0);
                        }
                        Some((i, None)) => {
                            let now = crate::model::now_ms();
                            let dbl = now - last_press.with(|t| std::mem::replace(t, now)) < 450;
                            let b = data.get_untracked()[i].clone();
                            if x < LIST_W || dbl {
                                st().expanded.set(Some((st().cur.get_untracked(), b.id.clone())));
                            } else if b.start.is_none() {
                                // A record without dates gets the day pressed.
                                let o = origin_day.get_untracked();
                                let d = ((x - LIST_W + sx.get_untracked()) / day_w.get_untracked()).floor() as i64;
                                let day = o + d * value::DAY_MS;
                                let (sf, ef) = st().with_view(|_, _, v| (v.start_field.clone(), v.end_field.clone())).unwrap_or((None, None));
                                let mut c = vec![];
                                if let Some(f) = sf {
                                    c.push((b.id.clone(), f, Some(Value::Date(day))));
                                }
                                if let Some(f) = ef {
                                    c.push((b.id.clone(), f, Some(Value::Date(day + 2 * value::DAY_MS))));
                                }
                                st().set_cells(&st().cur.get_untracked(), &c);
                            }
                        }
                        None => {}
                    }
                })
                .on_responder_move(move |e: &ResponderEvent| {
                    if let Some((_, _, x0, _, _)) = drag.get_untracked() {
                        let d = ((e.location.x.0 - x0) / day_w.get_untracked()).round() as i64;
                        if d != delta_days.get_untracked() {
                            delta_days.set(d);
                        }
                    }
                })
                .on_responder_release(move |_: &ResponderEvent| {
                    if let Some((i, grab, _, os, oe)) = drag.get_untracked() {
                        let d = delta_days.get_untracked();
                        drag.set(None);
                        delta_days.set(0);
                        if d != 0 {
                            let b = data.get_untracked()[i].clone();
                            let (sf, ef) = st().with_view(|_, _, v| (v.start_field.clone(), v.end_field.clone())).unwrap_or((None, None));
                            let mut c = vec![];
                            let (ns, ne) = match grab {
                                Grab::Move => (os + d * value::DAY_MS, oe + d * value::DAY_MS),
                                Grab::Start => ((os + d * value::DAY_MS).min(oe), oe),
                                Grab::End => (os, (oe + d * value::DAY_MS).max(os)),
                            };
                            if let Some(f) = sf {
                                c.push((b.id.clone(), f, Some(Value::Date(ns))));
                            }
                            if let Some(f) = ef {
                                c.push((b.id.clone(), f, Some(Value::Date(ne))));
                            }
                            st().set_cells(&st().cur.get_untracked(), &c);
                        }
                    }
                })
                .on_context_menu(move |_: &ContextMenuEvent| {}),
        )
}

