//! Dashboards (仪表盘): metric cards and charts of the tables' records.

use std::f32::consts::{FRAC_PI_2, TAU};

use nsh_core::{
    canvas::{CanvasTextAlign, CanvasTextBaseline},
    prelude::*,
};

use super::{
    cells::{ellipsize, text_at, text_w},
    icons, popups,
    state::st,
    theme::*,
    widgets,
};
use crate::model::{Chart, ChartKind, FieldKind, Filter, Id, Measure, Value, new_id, query, value};

/// The chart's data: (label, number, color) per group.
pub fn chart_data(c: &Chart) -> Vec<(String, f64, u32)> {
    st().read(|b| {
        let Some(t) = b.table(&c.table) else { return vec![] };
        let recs: Vec<&crate::model::Record> = t.records.iter().filter(|r| query::passes(b, t, r, &c.filter)).collect();
        let vf = c.value_field.as_ref().and_then(|f| t.field(f));
        let num = |r: &crate::model::Record| -> Option<f64> {
            vf.and_then(|f| query::cell(b, t, r, f)).and_then(|v| match v {
                Value::List(l) => l.iter().filter_map(|x| x.as_number()).reduce(|a, b| a + b),
                x => x.as_number(),
            })
        };
        let measure = |rs: &[&crate::model::Record]| -> f64 {
            let nums: Vec<f64> = rs.iter().filter_map(|r| num(r)).collect();
            match c.measure {
                Measure::Count => rs.len() as f64,
                Measure::Sum => nums.iter().sum(),
                Measure::Average => if nums.is_empty() { 0.0 } else { nums.iter().sum::<f64>() / nums.len() as f64 },
                Measure::Max => nums.iter().copied().reduce(f64::max).unwrap_or(0.0),
                Measure::Min => nums.iter().copied().reduce(f64::min).unwrap_or(0.0),
            }
        };
        let Some(gf) = c.group_by.as_ref().and_then(|g| t.field(g)) else {
            return vec![(c.title.clone(), measure(&recs), value::strong_color(0))];
        };
        // Groups: a record with several options or people counts in each.
        let mut groups: Vec<(String, Vec<&crate::model::Record>, u32)> = vec![];
        if let Some(opts) = gf.kind.options() {
            for (i, o) in opts.iter().enumerate() {
                let _ = i;
                groups.push((o.name.clone(), vec![], value::strong_color(o.color)));
            }
        }
        for r in &recs {
            let v = query::cell(b, t, r, gf);
            let labels: Vec<(String, u32)> = match (&v, &gf.kind) {
                (Some(Value::Options(ids)), k) => ids
                    .iter()
                    .filter_map(|id| k.options().and_then(|o| o.iter().find(|x| &x.id == id)).map(|o| (o.name.clone(), value::strong_color(o.color))))
                    .collect(),
                (Some(Value::Users(ids)), _) => ids.iter().filter_map(|id| b.member(id).map(|m| (m.name.clone(), value::strong_color(m.color)))).collect(),
                (Some(x), k) => vec![(value::display(b, t, k, x), 0)],
                (None, _) => vec![("空".into(), 0x8F959E)],
            };
            for (l, color) in labels {
                match groups.iter_mut().find(|g| g.0 == l) {
                    Some(g) => g.1.push(r),
                    None => {
                        let n = groups.len() as u8;
                        groups.push((l, vec![r], if color == 0 { value::strong_color(n) } else { color }));
                    }
                }
            }
        }
        groups.retain(|g| !g.1.is_empty());
        groups.into_iter().map(|(l, rs, color)| (l, measure(&rs), color)).collect()
    })
}

fn fmt(n: f64) -> String {
    if n.abs() >= 10000.0 {
        format!("{}", value::format_number(n, &FieldKind::Number { precision: 0, thousands: true, percent: false }))
    } else {
        value::format_plain((n * 100.0).round() / 100.0)
    }
}

pub fn paint_chart(c: &Chart, w: f32, h: f32) -> Painter {
    let data = chart_data(c);
    let mut p = Painter::new();
    if w <= 0.0 || h <= 0.0 {
        return p;
    }
    let total: f64 = data.iter().map(|d| d.1).sum();
    let max = data.iter().map(|d| d.1).fold(0.0f64, f64::max).max(1e-9);
    match c.kind {
        ChartKind::Metric => {
            let v = data.first().map(|d| d.1).unwrap_or(0.0);
            p = p
                .font(&bold(36.0))
                .fill_color(rgb(PRIMARY))
                .text_align(CanvasTextAlign::Left)
                .text_baseline(CanvasTextBaseline::Middle)
                .fill_text(&fmt(v), 4.0, h / 2.0);
        }
        ChartKind::Bar | ChartKind::Line => {
            let left = 44.0;
            let bottom = 28.0;
            let ch = h - bottom - 10.0;
            let cw = w - left - 8.0;
            for i in 0..=4 {
                let y = 10.0 + ch * (1.0 - i as f32 / 4.0);
                p = p.fill_color(rgb(LINE)).fill_rect(left, y, cw, 1.0);
                p = super::cells::text_right(p, &fmt(max * i as f64 / 4.0), left - 6.0, y, 11.0, TEXT3);
            }
            let n = data.len().max(1) as f32;
            let slot = cw / n;
            let mut pts = vec![];
            for (i, (label, v, color)) in data.iter().enumerate() {
                let x = left + slot * i as f32;
                let bh = ch * (*v / max) as f32;
                if c.kind == ChartKind::Bar {
                    let bw = (slot * 0.5).min(48.0);
                    p = super::cells::rrect(p, x + (slot - bw) / 2.0, 10.0 + ch - bh, bw, bh, 3.0, *color);
                    p = p
                        .font(&font(11.0))
                        .fill_color(rgb(TEXT2))
                        .text_align(CanvasTextAlign::Center)
                        .text_baseline(CanvasTextBaseline::Bottom)
                        .fill_text(&fmt(*v), x + slot / 2.0, 8.0 + ch - bh);
                } else {
                    pts.push((x + slot / 2.0, 10.0 + ch - bh));
                }
                p = p
                    .font(&font(11.0))
                    .fill_color(rgb(TEXT2))
                    .text_align(CanvasTextAlign::Center)
                    .text_baseline(CanvasTextBaseline::Middle)
                    .fill_text(&ellipsize(label, slot - 4.0, 11.0), x + slot / 2.0, h - bottom / 2.0);
            }
            if c.kind == ChartKind::Line && !pts.is_empty() {
                let mut q = p.stroke_color(rgb(PRIMARY)).line_width(2.0).begin_path().move_to(pts[0].0, pts[0].1);
                for (x, y) in &pts[1..] {
                    q = q.line_to(*x, *y);
                }
                p = q.stroke();
                for (x, y) in &pts {
                    p = super::cells::circle(p, *x, *y, 4.0, PRIMARY);
                    p = super::cells::circle(p, *x, *y, 2.0, 0xFFFFFF);
                }
            }
        }
        ChartKind::HBar => {
            let left = 90.0;
            let n = data.len().max(1) as f32;
            let slot = (h - 8.0) / n;
            let cw = w - left - 50.0;
            for (i, (label, v, color)) in data.iter().enumerate() {
                let y = 4.0 + slot * i as f32;
                let bh = (slot * 0.6).min(24.0);
                let bw = cw * (*v / max) as f32;
                p = super::cells::text_right(p, &ellipsize(label, left - 10.0, 12.0), left - 8.0, y + slot / 2.0, 12.0, TEXT2);
                p = super::cells::rrect(p, left, y + (slot - bh) / 2.0, bw.max(2.0), bh, 3.0, *color);
                p = text_at(p, &fmt(*v), left + bw + 6.0, y + slot / 2.0, 12.0, TEXT2);
            }
        }
        ChartKind::Pie | ChartKind::Donut => {
            let r = (h / 2.0 - 12.0).min(w * 0.3);
            let (cx, cy) = (r + 16.0, h / 2.0);
            let mut a = -FRAC_PI_2;
            if total <= 0.0 {
                p = super::cells::circle(p, cx, cy, r, 0xEFF0F1);
            }
            for (_, v, color) in &data {
                let sweep = (*v / total.max(1e-9)) as f32 * TAU;
                p = p.fill_color(rgb(*color)).begin_path().move_to(cx, cy).arc(cx, cy, r, a, a + sweep).close_path().fill();
                a += sweep;
            }
            if c.kind == ChartKind::Donut {
                p = super::cells::circle(p, cx, cy, r * 0.6, 0xFFFFFF);
                p = p
                    .font(&bold(18.0))
                    .fill_color(rgb(TEXT))
                    .text_align(CanvasTextAlign::Center)
                    .text_baseline(CanvasTextBaseline::Middle)
                    .fill_text(&fmt(total), cx, cy - 6.0)
                    .font(&font(11.0))
                    .fill_color(rgb(TEXT3))
                    .fill_text("总计", cx, cy + 12.0);
            }
            // The legend.
            let lx = cx + r + 24.0;
            for (i, (label, v, color)) in data.iter().enumerate() {
                let y = 16.0 + i as f32 * 22.0;
                if y > h - 8.0 {
                    break;
                }
                p = super::cells::rrect(p, lx, y - 5.0, 10.0, 10.0, 2.0, *color);
                let pct = if total > 0.0 { format!("{:.1}%", v / total * 100.0) } else { "0%".into() };
                let s = format!("{}  {}  {}", label, fmt(*v), pct);
                p = text_at(p, &ellipsize(&s, w - lx - 20.0, 12.0), lx + 16.0, y, 12.0, TEXT2);
            }
        }
    }
    if data.is_empty() && c.kind != ChartKind::Metric {
        p = p
            .font(&font(13.0))
            .fill_color(rgb(TEXT3))
            .text_align(CanvasTextAlign::Center)
            .text_baseline(CanvasTextBaseline::Middle)
            .fill_text("暂无数据", w / 2.0, h / 2.0);
    }
    let _ = text_w("", 1.0);
    p
}

pub fn dashboard(id: Id) -> impl IntoElement {
    let s = st();
    let editing = signal(None::<Option<Id>>);
    let width = signal(1000.0f32);
    let id2 = id.clone();
    let id3 = id.clone();
    view()
        .flex_1()
        .bg(rgb(BG_GREY))
        .child(
            view()
                .h(px(52.0))
                .flex_row()
                .items_center()
                .px(px(20.0))
                .gap(px(10.0))
                .bg(rgb(BG))
                .border_b(px(1.0))
                .border_color(rgb(BORDER))
                .child(icons::icon("dashboard", 18.0, 0xFF8800))
                .child(text(move || s.read(|b| b.dashboard(&id2).map(|d| d.name.clone()).unwrap_or_default())).text_size(px(16.0)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT)))
                .child(view().flex_1())
                .child(widgets::primary_button("+ 添加组件", move || editing.set(Some(None)))),
        )
        .child(
            scroll().flex_1().on_layout(move |b: &Bounds<Points>| width.set(b.size.width.0)).child(dynamic(move || {
                let charts = s.read(|b| b.dashboard(&id3).map(|d| d.charts.clone()).unwrap_or_default());
                let w = width.get();
                let unit = ((w - 40.0 - 3.0 * 16.0) / 4.0).floor().max(160.0);
                if charts.is_empty() {
                    return view()
                        .p(px(80.0))
                        .items_center()
                        .gap(px(12.0))
                        .child(icons::icon("dashboard", 48.0, 0xC9CDD4))
                        .child(text("仪表盘还没有组件").text_size(px(15.0)).text_color(rgb(TEXT2)))
                        .child(widgets::primary_button("添加组件", move || editing.set(Some(None))))
                        .into_element();
                }
                let dash = id3.clone();
                view()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(16.0))
                    .p(px(20.0))
                    .items_start()
                    .child(
                        charts
                            .into_iter()
                            .map(|c| {
                                let cw = unit * c.span as f32 + 16.0 * (c.span as f32 - 1.0);
                                let ch = 120.0 * c.rows as f32 + 16.0 * (c.rows as f32 - 1.0);
                                let c2 = c.clone();
                                let cid = c.id.clone();
                                let cid2 = c.id.clone();
                                let dash2 = dash.clone();
                                let tname = s.read(|b| b.table(&c.table).map(|t| t.name.clone()).unwrap_or_default());
                                view()
                                    .w(px(cw))
                                    .h(px(ch))
                                    .bg(rgb(BG))
                                    .rounded(px(10.0))
                                    .border_1()
                                    .border_color(rgb(BORDER))
                                    .p(px(16.0))
                                    .gap(px(8.0))
                                    .child(
                                        view()
                                            .flex_row()
                                            .items_center()
                                            .gap(px(6.0))
                                            .child(text(c.title.clone()).flex_1().text_size(px(14.0)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT)).number_of_lines(1u32))
                                            .child(text(tname).text_size(px(11.0)).text_color(rgb(TEXT3)))
                                            .child(view().size(px(24.0)).rounded(px(4.0)).items_center().justify_center().hover(|s| s.bg(rgb(HOVER))).on_press(move || editing.set(Some(Some(cid.clone())))).child(icons::icon("edit", 14.0, TEXT3)))
                                            .child(view().size(px(24.0)).rounded(px(4.0)).items_center().justify_center().hover(|s| s.bg(rgb(HOVER))).on_press(move || {
                                                let (d, c) = (dash2.clone(), cid2.clone());
                                                st().edit(|b| {
                                                    if let Some(db) = b.dashboard_mut(&d) {
                                                        db.charts.retain(|x| x.id != c);
                                                    }
                                                });
                                            }).child(icons::icon("trash", 14.0, TEXT3))),
                                    )
                                    .child(canvas(move || {
                                        let _ = st().rev.get();
                                        paint_chart(&c2, cw - 32.0, ch - 56.0)
                                    }).w(px(cw - 32.0)).h(px(ch - 56.0)))
                                    .into_element()
                            })
                            .collect::<Vec<_>>(),
                    )
                    .into_element()
            })),
        )
        .child(dynamic(move || editing.get().map(|which| chart_editor(id.clone(), which, editing).into_element())))
}

fn chart_editor(dash: Id, which: Option<Id>, editing: Signal<Option<Option<Id>>>) -> impl IntoElement {
    let s = st();
    let existing = which.as_ref().and_then(|w| s.peek(|b| b.dashboard(&dash).and_then(|d| d.charts.iter().find(|c| &c.id == w).cloned())));
    let first_table = s.peek(|b| b.tables.first().map(|t| t.id.clone()).unwrap_or_default());
    let draft = signal(existing.clone().unwrap_or(Chart {
        id: new_id("cht"),
        title: "新图表".into(),
        kind: ChartKind::Bar,
        table: first_table,
        group_by: None,
        measure: Measure::Count,
        value_field: None,
        filter: Filter::default(),
        span: 2,
        rows: 3,
    }));
    let title = signal(draft.get_untracked().title.clone());
    let close = move || editing.set(None);
    let tables = move || s.read(|b| b.tables.iter().map(|t| (t.id.clone(), t.name.clone())).collect::<Vec<_>>());
    let fields = move || {
        let tid = draft.get().table;
        s.read(|b| b.table(&tid).map(|t| t.fields.iter().map(|f| (f.id.clone(), f.name.clone(), f.kind.clone())).collect::<Vec<_>>()).unwrap_or_default())
    };
    let row = |label: &'static str, el: AnyElement| view().flex_row().items_center().gap(px(12.0)).child(text(label).w(px(80.0)).text_size(px(13.0)).text_color(rgb(TEXT2))).child(el).into_element();
    popups::modal(
        820.0,
        if existing.is_some() { "编辑组件" } else { "添加组件" },
        close,
        view()
            .flex_row()
            .px(px(20.0))
            .pb(px(20.0))
            .gap(px(20.0))
            .child(
                view()
                    .w(px(340.0))
                    .gap(px(12.0))
                    .child(row("标题", widgets::text_field(title, "图表标题").flex_1().on_change(move |t: &str| {
                        title.set(t.to_string());
                        let t = t.to_string();
                        draft.update(|d| d.title = t);
                    }).into_element()))
                    .child(row("图表类型", view().flex_row().flex_wrap().gap(px(6.0)).flex_1().child(
                        ChartKind::ALL
                            .iter()
                            .map(|k| {
                                let k = *k;
                                view()
                                    .px(px(10.0))
                                    .py(px(5.0))
                                    .rounded(px(6.0))
                                    .border_1()
                                    .border_color(move || if draft.get().kind == k { rgb(PRIMARY) } else { rgb(BORDER) })
                                    .bg(move || if draft.get().kind == k { rgb(ACTIVE) } else { rgb(BG) })
                                    .on_press(move || draft.update(|d| {
                                        d.kind = k;
                                        if k == ChartKind::Metric {
                                            d.group_by = None;
                                            d.span = 1;
                                            d.rows = 1;
                                        } else if d.rows < 2 {
                                            d.span = 2;
                                            d.rows = 3;
                                        }
                                    }))
                                    .child(text(k.label()).text_size(px(13.0)).text_color(rgb(TEXT)))
                                    .into_element()
                            })
                            .collect::<Vec<_>>(),
                    ).into_element()))
                    .child(row("数据源", widgets::dropdown(
                        move || {
                            let tid = draft.get().table;
                            tables().into_iter().find(|t| t.0 == tid).map(|t| t.1).unwrap_or_default()
                        },
                        move || tables().into_iter().map(|t| t.1).collect(),
                        move |i| {
                            if let Some((id, _)) = tables().get(i).cloned() {
                                draft.update(|d| {
                                    d.table = id;
                                    d.group_by = None;
                                    d.value_field = None;
                                });
                            }
                        },
                        240.0,
                    ).into_element()))
                    .child(when(move || draft.get().kind != ChartKind::Metric, move || {
                        row("横轴/分类", widgets::dropdown(
                            move || {
                                let g = draft.get().group_by;
                                g.and_then(|g| fields().into_iter().find(|f| f.0 == g).map(|f| f.1)).unwrap_or("选择字段".into())
                            },
                            move || fields().into_iter().map(|f| f.1).collect(),
                            move |i| {
                                let id = fields().get(i).map(|f| f.0.clone());
                                draft.update(|d| d.group_by = id);
                            },
                            240.0,
                        ).into_element())
                    }))
                    .child(row("数值", widgets::dropdown(
                        move || draft.get().measure.label().to_string(),
                        || Measure::ALL.iter().map(|m| m.label().to_string()).collect(),
                        move |i| draft.update(|d| d.measure = Measure::ALL[i]),
                        240.0,
                    ).into_element()))
                    .child(when(move || draft.get().measure != Measure::Count, move || {
                        row("统计字段", widgets::dropdown(
                            move || {
                                let g = draft.get().value_field;
                                g.and_then(|g| fields().into_iter().find(|f| f.0 == g).map(|f| f.1)).unwrap_or("选择数字字段".into())
                            },
                            move || fields().into_iter().filter(|f| f.2.is_numeric() || matches!(f.2, FieldKind::Formula { .. } | FieldKind::Lookup { .. })).map(|f| f.1).collect(),
                            move |i| {
                                let id = fields().into_iter().filter(|f| f.2.is_numeric() || matches!(f.2, FieldKind::Formula { .. } | FieldKind::Lookup { .. })).nth(i).map(|f| f.0);
                                draft.update(|d| d.value_field = id);
                            },
                            240.0,
                        ).into_element())
                    }))
                    .child(row("宽度", widgets::dropdown(
                        move || format!("{} 列", draft.get().span),
                        || (1..=4).map(|i| format!("{i} 列")).collect(),
                        move |i| draft.update(|d| d.span = i as u8 + 1),
                        240.0,
                    ).into_element()))
                    .child(
                        view().flex_row().justify_end().gap(px(8.0)).pt(px(8.0)).child(widgets::button("取消", close)).child(widgets::primary_button("确定", move || {
                            let c = draft.get_untracked();
                            let d = dash.clone();
                            st().edit(|b| {
                                if let Some(db) = b.dashboard_mut(&d) {
                                    match db.charts.iter_mut().find(|x| x.id == c.id) {
                                        Some(x) => *x = c,
                                        None => db.charts.push(c),
                                    }
                                }
                            });
                            close();
                        })),
                    ),
            )
            .child(
                view()
                    .flex_1()
                    .h(px(360.0))
                    .rounded(px(8.0))
                    .bg(rgb(BG_GREY))
                    .p(px(16.0))
                    .gap(px(8.0))
                    .child(text(move || draft.get().title).text_size(px(14.0)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT)))
                    .child(canvas(move || {
                        let mut c = draft.get();
                        if c.kind == ChartKind::Metric {
                            c.group_by = None;
                        }
                        paint_chart(&c, 380.0, 300.0)
                    }).w(px(380.0)).h(px(300.0))),
            ),
    )
}
