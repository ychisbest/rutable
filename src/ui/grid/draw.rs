//! Painting the grid.

use nsh_core::{
    canvas::{CanvasTextAlign, CanvasTextBaseline},
    prelude::*,
};

use super::*;
use crate::{
    model::{FieldKind, query, value},
    ui::{
        cells::{self, ellipsize, rrect, text_at, text_w},
        icons,
        theme::*,
    },
};

fn line(p: Painter, x: f32, y: f32, w: f32, h: f32, color: u32) -> Painter {
    p.fill_color(rgb(color)).fill_rect(x, y, w, h)
}

pub fn paint(g: &Grid) -> Painter {
    let l = g.layout.get();
    let (w, h) = g.size.get();
    let sx = g.sx.get();
    let sy = g.sy.get();
    let sel = g.sel.get();
    let hover = g.hover.get();
    let hover_row = g.hover_row.get();
    let editing = g.editing.get();
    let drag = g.drag.get();
    let checked = st().checked.get();
    let s = st();
    let _ = s.rev.get();
    let mut p = Painter::new().fill_color(rgb(BG)).fill_rect(0.0, 0.0, w, h);
    if w <= 0.0 || h <= 0.0 {
        return p;
    }
    let body_bottom = h - STATS;
    let scroll_left = RH + l.frozen_w;
    let n = l.rows.len();
    // Visible rows.
    let first = l.tops.partition_point(|t| *t + 200.0 < sy).saturating_sub(1);
    let mut last = first;
    while last < n && l.tops[last] < sy + (body_bottom - HEAD) {
        last += 1;
    }
    let last = last.min(n);

    let (r0, r1, c0, c1) = sel.map(|s| {
        let (a, b) = s.rows();
        let (c, d) = s.cols();
        (a, b, c, d)
    }).unwrap_or((usize::MAX, 0, usize::MAX, 0));
    let fill_to = if let Drag::Fill { to_row } = drag { Some(to_row) } else { None };

    p = s.peek(move |b| {
        let mut p = p;
        let Some(t) = b.table(&l.table) else { return p };
        let v_colors = t.view(&l.view).map(|v| v.row_colors.clone()).unwrap_or_default();
        // Body rows: scrolling columns first, frozen columns over them.
        for pass in 0..2 {
            let clip_x = if pass == 0 { scroll_left } else { 0.0 };
            let clip_w = if pass == 0 { w - scroll_left } else { scroll_left };
            p = cells::clip(p, clip_x, HEAD, clip_w, body_bottom - HEAD);
            for r in first..last {
                let y = HEAD + l.tops[r] - sy;
                let rh = l.heights[r];
                match &l.rows[r] {
                    Row::Record(ri) => {
                        let rec = &t.records[*ri];
                        let is_checked = checked.contains(&rec.id);
                        let in_rows = r >= r0 && r <= r1 && sel.is_some_and(|s| !s.single() || s.cols);
                        let fill = v_colors.iter().find(|(c, _)| (c.value != crate::model::FilterValue::None || !c.op.needs_value()) && query::meets(b, t, rec, c)).map(|(_, col)| crate::model::value::tag_color(*col).0);
                        let bg = if is_checked {
                            0xEEF3FF
                        } else if let Some(f) = fill {
                            f
                        } else if hover_row == Some(r) {
                            0xF7F8FA
                        } else {
                            BG
                        };
                        let row_x = if pass == 0 { scroll_left } else { 0.0 };
                        p = line(p, row_x, y, clip_w, rh, bg);
                        let cols: Vec<usize> = if pass == 0 { (l.frozen..l.cols.len()).collect() } else { (0..l.frozen).collect() };
                        for c in cols {
                            let col = &l.cols[c];
                            let cx = if c < l.frozen { RH + col.x } else { RH + col.x - sx };
                            if cx > w || cx + col.w < clip_x {
                                continue;
                            }
                            // The range's light fill.
                            if in_rows && c >= c0 && c <= c1 {
                                p = p.fill_color(rgba(0x3370FF14)).fill_rect(cx, y, col.w, rh);
                            }
                            if let Ok(mi) = l.matches.binary_search(&(r, c)) {
                                let current = mi == g.match_at.get().min(l.matches.len().saturating_sub(1));
                                p = p.fill_color(rgb(if current { 0xFFD666 } else { 0xFFF3C4 })).fill_rect(cx, y, col.w, rh);
                            }
                            if let Some(f) = t.field(&col.field) {
                                let v = query::cell(b, t, rec, f);
                                let lines = if rh > 40.0 { ((rh - 8.0) / 22.0).floor().max(1.0) as usize } else { 1 };
                                p = cells::clip(p, cx, y, col.w, rh);
                                p = cells::draw_value(p, b, t, &f.kind, v.as_ref(), cx, y, col.w, rh, lines);
                                if let FieldKind::Button { label, color, .. } = &f.kind {
                                    let mid = y + rh.min(33.0) / 2.0;
                                    let bw = text_w(label, 12.0) + 24.0;
                                    let on = matches!(hover, Hit::CellButton(hr, hc) if hr == r && hc == c);
                                    let bg = crate::model::value::strong_color(*color);
                                    p = rrect(p, cx + 8.0, mid - 12.0, bw.min(col.w - 16.0), 24.0, 5.0, bg);
                                    if on {
                                        p = p.fill_color(rgba(0xFFFFFF33)).fill_rect(cx + 8.0, mid - 12.0, bw.min(col.w - 16.0), 24.0);
                                    }
                                    p = text_at(p, &ellipsize(label, col.w - 40.0, 12.0), cx + 20.0, mid, 12.0, 0xFFFFFF);
                                }
                                // Hovering the primary cell offers "查看" (open the record).
                                if c == 0 && hover_row == Some(r) && editing.is_none() {
                                    let mid = y + rh.min(33.0) / 2.0;
                                    let bx = cx + col.w - 62.0;
                                    let on = matches!(hover, Hit::View(hr) if hr == r);
                                    p = rrect(p, bx, mid - 11.0, 54.0, 22.0, 4.0, if on { 0xE1EAFF } else { 0xFFFFFF });
                                    p = cells::stroke_rrect(p, bx + 0.5, mid - 10.5, 53.0, 21.0, 4.0, BORDER, 1.0);
                                    p = icons::draw(p, "expand", bx + 6.0, mid - 6.0, 12.0, if on { PRIMARY } else { TEXT2 });
                                    p = text_at(p, "查看", bx + 22.0, mid, 12.0, if on { PRIMARY } else { TEXT2 });
                                }
                                // A selected option cell shows its drop-down arrow.
                                if sel.is_some_and(|s| s.single() && s.ar == r && s.ac == c) && editing.is_none()
                                    && matches!(f.kind, FieldKind::SingleSelect { .. } | FieldKind::MultiSelect { .. } | FieldKind::Date { .. })
                                {
                                    let mid = y + rh.min(33.0) / 2.0;
                                    p = icons::draw(p, "chevron-down", cx + col.w - 24.0, mid - 7.0, 14.0, TEXT2);
                                }
                                // The selected cell's "+" (attachments, links, people).
                                if sel.is_some_and(|s| s.single() && s.ar == r && s.ac == c)
                                    && matches!(f.kind, FieldKind::Attachment | FieldKind::Link { .. } | FieldKind::User { .. })
                                {
                                    let mid = y + rh.min(33.0) / 2.0;
                                    p = rrect(p, cx + col.w - 28.0, mid - 10.0, 20.0, 20.0, 4.0, 0xF2F3F5);
                                    p = icons::draw(p, "plus", cx + col.w - 25.0, mid - 7.0, 14.0, TEXT2);
                                }
                                p = p.restore();
                            }
                            p = line(p, cx + col.w - 1.0, y, 1.0, rh, LINE);
                        }
                        p = line(p, row_x, y + rh - 1.0, clip_w, 1.0, LINE);
                        if pass == 1 {
                            // Row header: grip, number or checkbox, expand.
                            p = line(p, 0.0, y, RH, rh, bg);
                            let mid = y + rh.min(33.0) / 2.0;
                            let hovered = hover_row == Some(r);
                            if hovered && !l.sorted {
                                p = icons::draw(p, "grip", 2.0, mid - 7.0, 14.0, TEXT3);
                            }
                            if hovered || is_checked {
                                p = cells::checkbox(p, 22.0, mid, is_checked);
                            } else {
                                let num = (l.records.iter().position(|x| x == ri).unwrap_or(0) + 1).to_string();
                                p = p
                                    .font(&font(12.0))
                                    .fill_color(rgb(TEXT3))
                                    .text_align(CanvasTextAlign::Center)
                                    .text_baseline(CanvasTextBaseline::Middle)
                                    .fill_text(&num, 30.0, mid);
                            }
                            if hovered {
                                let on = matches!(hover, Hit::RowExpand(hr) if hr == r);
                                if on {
                                    p = rrect(p, 46.0, mid - 11.0, 22.0, 22.0, 4.0, 0xE8EAED);
                                }
                                p = icons::draw(p, "expand", 49.0, mid - 8.0, 16.0, if on { PRIMARY } else { TEXT2 });
                            }
                            let comments = rec.comments.len();
                            if comments > 0 && !hovered {
                                p = icons::draw(p, "comment", 50.0, mid - 6.0, 12.0, TEXT3);
                            }
                            p = line(p, 0.0, y + rh - 1.0, RH, 1.0, LINE);
                        }
                    }
                    Row::Group { level, field, value: v, count, collapsed, .. } => {
                        let row_x = if pass == 0 { scroll_left } else { 0.0 };
                        let bg = [0xF5F6F7, 0xF8F9FA, 0xFAFBFC][(*level).min(2)];
                        p = line(p, row_x, y, clip_w, rh, bg);
                        p = line(p, row_x, y + rh - 1.0, clip_w, 1.0, LINE);
                        if pass == 1 {
                            let x0 = 12.0 + *level as f32 * 20.0;
                            let mid = y + rh / 2.0;
                            p = icons::draw(p, if *collapsed { "chevron-right" } else { "chevron-down" }, x0, mid - 8.0, 16.0, TEXT2);
                            if let Some(f) = t.field(field) {
                                let label = f.name.clone();
                                p = text_at(p, &label, x0 + 22.0, mid, 12.0, TEXT3);
                                let lx = x0 + 22.0 + text_w(&label, 12.0) + 8.0;
                                let avail = scroll_left + 400.0;
                                p = match v {
                                    None => text_at(p, "空", lx, mid, 14.0, TEXT3),
                                    Some(v) => cells::draw_value(p, b, t, &f.kind, Some(v), lx - 8.0, y + (rh - 33.0) / 2.0, avail - lx, 33.0, 1),
                                };
                                let shown = v.as_ref().map(|v| value::display(b, t, &f.kind, v)).unwrap_or_else(|| "空".into());
                                let vw = match v {
                                    Some(crate::model::Value::Options(_)) | Some(crate::model::Value::Users(_)) => text_w(&shown, 12.0) + 34.0,
                                    _ => text_w(&shown, 14.0) + 6.0,
                                };
                                p = text_at(p, &count.to_string(), lx + vw.min(300.0) + 8.0, mid, 12.0, TEXT3);
                            }
                        }
                    }
                    Row::Add { .. } => {
                        let row_x = if pass == 0 { scroll_left } else { 0.0 };
                        let hovered = matches!(hover, Hit::AddRow(hr) if hr == r);
                        let end = (RH + l.content_w - ADD_COL - sx).max(scroll_left);
                        let row_w = if pass == 0 { (end - scroll_left).max(0.0) } else { clip_w };
                        p = line(p, row_x, y, row_w, rh, if hovered { 0xF7F8FA } else { BG });
                        p = line(p, row_x, y + rh - 1.0, row_w, 1.0, LINE);
                        if pass == 1 {
                            p = icons::draw(p, "plus", 24.0, y + rh / 2.0 - 8.0, 16.0, if hovered { PRIMARY } else { TEXT3 });
                        }
                    }
                }
            }
            p = p.restore();
        }

        // Frozen columns' edge.
        if l.frozen > 0 {
            p = line(p, scroll_left - 1.0, HEAD, 1.0, body_bottom - HEAD, BORDER);
            if sx > 0.0 {
                let shadow = nsh_core::canvas::CanvasGradient::linear(point(px(scroll_left), px(0.0)), point(px(scroll_left + 8.0), px(0.0)))
                    .add_color_stop(percentage(0.0), rgba(0x1F232914))
                    .add_color_stop(percentage(1.0), rgba(0x1F232900));
                p = p.fill_style(shadow).fill_rect(scroll_left, 0.0, 8.0, body_bottom);
            }
        }

        // The selection's border and the fill handle.
        if let Some(sel) = sel {
            if r0 < n && c0 < l.cols.len() && c1 < l.cols.len() {
                let (rr0, rr1) = (r0, fill_to.map(|f| f.max(r1)).unwrap_or(r1).min(n - 1));
                let x0 = g.col_x(&l, c0);
                let x1 = g.col_x(&l, c1) + l.cols[c1].w;
                let y0 = HEAD + l.tops[rr0] - sy;
                let y1 = HEAD + l.tops[rr1] + l.heights[rr1] - sy;
                let clip_left = if c0 < l.frozen { RH } else { scroll_left };
                p = cells::clip(p, clip_left, HEAD, w - clip_left, body_bottom - HEAD);
                if !sel.single() || fill_to.is_some() {
                    p = p.stroke_color(rgb(PRIMARY)).line_width(1.0)
                        .stroke_rect(bounds(point(px(x0 + 0.5), px(y0 + 0.5)), size(px(x1 - x0 - 1.0), px(y1 - y0 - 1.0))));
                }
                // The active cell.
                if sel.ar < n && sel.ac < l.cols.len() && editing.is_none() {
                    let (ax, ay, aw, ah) = g.cell_rect(&l, sel.ar, sel.ac);
                    p = p.stroke_color(rgb(PRIMARY)).line_width(2.0)
                        .stroke_rect(bounds(point(px(ax + 1.0), px(ay + 1.0)), size(px(aw - 2.0), px(ah - 2.0))));
                }
                if editing.is_none() && !sel.cols {
                    let fx = x1 - 4.0;
                    let fy = HEAD + l.tops[r1.min(n - 1)] + l.heights[r1.min(n - 1)] - sy - 4.0;
                    p = rrect(p, fx - 1.0, fy - 1.0, 9.0, 9.0, 1.0, BG);
                    p = rrect(p, fx, fy, 7.0, 7.0, 1.0, PRIMARY);
                }
                p = p.restore();
            }
        }

        // Header.
        p = line(p, 0.0, 0.0, w, HEAD, BG_GREY);
        for pass in 0..2 {
            let clip_x = if pass == 0 { scroll_left } else { 0.0 };
            let clip_w = if pass == 0 { w - scroll_left } else { scroll_left };
            p = cells::clip(p, clip_x, 0.0, clip_w, HEAD);
            let cols: Vec<usize> = if pass == 0 { (l.frozen..l.cols.len()).collect() } else { (0..l.frozen).collect() };
            for c in cols {
                let col = &l.cols[c];
                let cx = if c < l.frozen { RH + col.x } else { RH + col.x - sx };
                if cx > w || cx + col.w < clip_x {
                    continue;
                }
                let selected_col = sel.is_some_and(|s| s.cols && c >= c0 && c <= c1);
                let hovered = matches!(hover, Hit::Header(hc) | Hit::HeaderMenu(hc) if hc == c);
                let bg = if selected_col { 0xE1EAFF } else if hovered { 0xEBECEE } else { BG_GREY };
                p = line(p, cx, 0.0, col.w, HEAD, bg);
                let icon_color = if selected_col { PRIMARY } else { TEXT2 };
                p = icons::draw(p, icons::field_icon(&col.kind), cx + 8.0, HEAD / 2.0 - 7.0, 14.0, icon_color);
                let name_w = col.w - 30.0 - if hovered { 24.0 } else { 0.0 };
                p = text_at(p, &ellipsize(&col.name, name_w, 13.0), cx + 28.0, HEAD / 2.0, 13.0, if selected_col { PRIMARY } else { TEXT });
                if hovered {
                    let on = matches!(hover, Hit::HeaderMenu(hc) if hc == c);
                    if on {
                        p = rrect(p, cx + col.w - 26.0, HEAD / 2.0 - 10.0, 20.0, 20.0, 4.0, 0xDEE0E3);
                    }
                    p = icons::draw(p, "chevron-down", cx + col.w - 23.0, HEAD / 2.0 - 7.0, 14.0, TEXT2);
                }
                if !col.desc.is_empty() && !hovered {
                    p = icons::draw(p, "info", cx + col.w - 20.0, HEAD / 2.0 - 6.0, 12.0, TEXT3);
                }
                let resize_on = matches!(hover, Hit::Resize(rc) if rc == c) || matches!(drag, Drag::Resize { col: rc, .. } if rc == c);
                p = line(p, cx + col.w - if resize_on { 2.0 } else { 1.0 }, 0.0, if resize_on { 3.0 } else { 1.0 }, HEAD, if resize_on { PRIMARY } else { BORDER });
            }
            if pass == 0 {
                // The "+" that adds a field.
                let end = RH + l.content_w - ADD_COL - sx;
                let on = hover == Hit::AddField;
                p = line(p, end, 0.0, ADD_COL, HEAD, if on { 0xEBECEE } else { BG_GREY });
                p = icons::draw(p, "plus", end + ADD_COL / 2.0 - 8.0, HEAD / 2.0 - 8.0, 16.0, TEXT2);
                p = line(p, end + ADD_COL - 1.0, 0.0, 1.0, HEAD, BORDER);
            }
            p = p.restore();
        }
        // Corner: tick every record.
        p = line(p, 0.0, 0.0, RH, HEAD, BG_GREY);
        let all = !l.record_ids.is_empty() && l.record_ids.iter().all(|id| checked.contains(id));
        if all || !checked.is_empty() || hover == Hit::Corner {
            p = cells::checkbox(p, 22.0, HEAD / 2.0, all);
            if !all && !checked.is_empty() {
                p = rrect(p, 22.0, HEAD / 2.0 - 8.0, 16.0, 16.0, 3.0, PRIMARY);
                p = line(p, 26.0, HEAD / 2.0 - 1.0, 8.0, 2.0, 0xFFFFFF);
            }
        }
        p = line(p, 0.0, HEAD - 1.0, w, 1.0, BORDER);
        if l.frozen > 0 {
            p = line(p, scroll_left - 1.0, 0.0, 1.0, HEAD, BORDER);
        }

        // A column being moved: its ghost and where it drops.
        if let Drag::MoveCol { col, to, moved: true, start_x } = &drag {
            let _ = start_x;
            if *col < l.cols.len() {
                let tx = if *to < l.cols.len() { g.col_x(&l, *to) } else { g.col_x(&l, l.cols.len() - 1) + l.cols.last().unwrap().w };
                p = line(p, tx - 1.0, 0.0, 2.0, body_bottom, PRIMARY);
            }
        }
        if let Drag::MoveRow { to, moved: true, .. } = &drag {
            let ty = if *to < n { HEAD + l.tops[*to] - sy } else { HEAD + l.content_h - sy };
            p = line(p, 0.0, ty - 1.0, w, 2.0, PRIMARY);
        }

        // Statistics bar.
        let by = body_bottom;
        p = line(p, 0.0, by, w, STATS, BG);
        p = line(p, 0.0, by, w, 1.0, BORDER);
        for pass in 0..2 {
            let clip_x = if pass == 0 { scroll_left } else { 0.0 };
            let clip_w = if pass == 0 { w - scroll_left } else { scroll_left };
            p = cells::clip(p, clip_x, by, clip_w, STATS);
            let cols: Vec<usize> = if pass == 0 { (l.frozen..l.cols.len()).collect() } else { (0..l.frozen).collect() };
            for c in cols {
                let col = &l.cols[c];
                let cx = if c < l.frozen { RH + col.x } else { RH + col.x - sx };
                if cx > w || cx + col.w < clip_x {
                    continue;
                }
                let hovered = hover == Hit::Stat(c);
                if c == 0 {
                    // The first column's area shows the record count.
                    let count = format!("{} 条记录", l.records.len());
                    p = text_at(p, &count, 12.0, by + STATS / 2.0, 12.0, TEXT2);
                }
                if col.stat != Stat::None {
                    if let Some(f) = t.field(&col.field) {
                        let s = query::stat(b, t, f, &l.records, col.stat);
                        p = cells::text_right(p, &ellipsize(&s, col.w - 16.0, 12.0), cx + col.w - 8.0, by + STATS / 2.0, 12.0, TEXT2);
                    }
                } else if hovered {
                    p = cells::text_right(p, "统计", cx + col.w - 24.0, by + STATS / 2.0, 12.0, TEXT3);
                    p = icons::draw(p, "chevron-down", cx + col.w - 22.0, by + STATS / 2.0 - 6.0, 12.0, TEXT3);
                }
                if hovered {
                    p = p.fill_color(rgba(0x1F23290A)).fill_rect(cx, by + 1.0, col.w, STATS - 1.0);
                }
            }
            p = p.restore();
        }

        // Scroll bars.
        let max_sy = g.max_sy(&l);
        if max_sy > 0.0 {
            let track = body_bottom - HEAD - 4.0;
            let thumb = (track * (track / (l.content_h + 40.0))).max(30.0);
            let ty = HEAD + 2.0 + (track - thumb) * (sy / max_sy);
            let on = hover == Hit::VBar || matches!(drag, Drag::VBar { .. });
            p = rrect(p, w - 8.0, ty, 6.0, thumb, 3.0, if on { 0x8F959E } else { 0xC9CDD4 });
        }
        let max_sx = g.max_sx(&l);
        if max_sx > 0.0 {
            let track = w - scroll_left - 12.0;
            let full = l.content_w - l.frozen_w + 40.0;
            let thumb = (track * (track / full)).max(30.0);
            let tx = scroll_left + 2.0 + (track - thumb) * (sx / max_sx);
            let on = hover == Hit::HBar || matches!(drag, Drag::HBar { .. });
            p = rrect(p, tx, body_bottom - 8.0, thumb, 6.0, 3.0, if on { 0x8F959E } else { 0xC9CDD4 });
        }

        // An empty view.
        if l.records.is_empty() && !l.grouped {
            let msg = if s.search.get_untracked().trim().is_empty() { "暂无记录" } else { "没有找到匹配的记录" };
            p = p
                .font(&font(13.0))
                .fill_color(rgb(TEXT3))
                .text_align(CanvasTextAlign::Center)
                .text_baseline(CanvasTextBaseline::Middle)
                .fill_text(msg, w / 2.0, HEAD + ADD_H + 60.0);
        }
        p
    });
    p
}
