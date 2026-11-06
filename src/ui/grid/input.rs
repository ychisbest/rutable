//! The grid's component: the canvas, the pointer, the hidden text field
//! that takes typing (and becomes the cell editor), and the editors' popups.

use nsh_core::prelude::*;

use super::{draw, edit, *};
use crate::{
    model::{FieldKind, Value, query::Row},
    ui::{
        state::{Popup, st},
        theme::*,
    },
    win,
};

fn now_ms() -> f64 {
    crate::model::now_ms() as f64
}

#[component]
pub fn GridView() -> impl IntoElement {
    let g = Grid::new();
    let s = st();
    crate::ui::wheel::take(move |dx, dy| g.scroll_by(dx, dy));

    // The selection resets when the view changes.
    {
        let last = stored(String::new());
        effect(move || {
            let key = format!("{}/{}", s.cur.get(), s.view.get());
            if last.with(|l| *l != key) {
                last.with(|l| *l = key);
                crate::ui::automation::log("sel reset: view changed");
                g.sel.set(None);
                g.editing.set(None);
                g.sx.set(0.0);
                g.sy.set(0.0);
            }
        });
    }
    // Keep the selection inside the rows when they change.
    effect(move || {
        let l = g.layout.get();
        if let Some(mut sel) = g.sel.get_untracked() {
            let n = l.rows.len();
            let c = l.cols.len();
            if n == 0 || c == 0 {
                crate::ui::automation::log("sel reset: no rows");
                g.sel.set(None);
                return;
            }
            let fix = |r: usize| l.next_record(r.min(n - 1) as isize, -1).or(l.first_record());
            let (Some(ar), Some(fr)) = (fix(sel.ar), fix(sel.fr)) else {
                crate::ui::automation::log("sel reset: rows gone");
                g.sel.set(None);
                return;
            };
            sel.ar = ar;
            sel.fr = fr;
            sel.ac = sel.ac.min(c - 1);
            sel.fc = sel.fc.min(c - 1);
            if Some(sel) != g.sel.get_untracked() {
                g.sel.set(Some(sel));
            }
        }
        // Scroll stays in range.
        let my = g.max_sy(&l);
        if g.sy.get_untracked() > my {
            g.sy.set(my);
        }
    });
    // Where the keyboard goes.
    effect(move || {
        let popup_open = s.popup.get().is_some() || s.expanded.get().is_some();
        let m = if popup_open {
            win::mode::FREE
        } else if let Some(e) = g.editing.get() {
            if e.mode == EditMode::Text { win::mode::EDITING } else { win::mode::EDITING_LIST }
        } else if g.focused.get() && g.sel.get().is_some() {
            win::mode::GRID
        } else {
            win::mode::FREE
        };
        win::set_key_mode(m);
        win::set_grid_wheel(!popup_open && g.editing.get().is_none_or(|e| e.mode == EditMode::Text));
        let multi = g.editing.get().is_some_and(|e| {
            g.layout.get_untracked().cols.get(e.col).is_some_and(|c| matches!(c.kind, FieldKind::Text))
        });
        win::set_multiline(multi);
    });
    // The grid's box for the wheel.
    effect(move || {
        let (x, y) = g.origin.get();
        let (w, h) = g.size.get();
        win::set_grid_rect(x, y, w, h);
    });
    // Back to the grid's keyboard after a popup closes.
    effect(move || {
        if s.popup.get().is_none() && s.expanded.get().is_none() && g.sel.get_untracked().is_some() {
            g.input.focus();
        }
    });

    let grant = move |e: &ResponderEvent| {
        let (x, y) = (e.location.x.0, e.location.y.0);
        g.origin.set((e.page.x.0 - x, e.page.y.0 - y));
        on_down(g, x, y);
    };
    let moved = move |e: &ResponderEvent| on_move(g, e.location.x.0, e.location.y.0);
    let released = move |_: &ResponderEvent| on_up(g);

    let editor_rect = move || {
        let l = g.layout.get();
        let _ = g.sx.get();
        let _ = g.sy.get();
        let target = g.editing.get().map(|e| (e.row, e.col)).or_else(|| g.sel.get().map(|s| (s.ar, s.ac)));
        match target {
            Some((r, c)) if r < l.rows.len() && c < l.cols.len() => g.cell_rect(&l, r, c),
            _ => (0.0, 0.0, 1.0, 1.0),
        }
    };
    let text_editing = move || g.editing.get().is_some_and(|e| e.mode == EditMode::Text);
    let multiline = move || {
        g.editing.get().is_some_and(|e| g.layout.get_untracked().cols.get(e.col).is_some_and(|c| matches!(c.kind, FieldKind::Text)))
    };

    view()
        .flex_1()
        .relative()
        .overflow_hidden()
        .bg(rgb(BG))
        .child(
            canvas(move || draw::paint(&g))
                .absolute()
                .inset(px(0.0))
                .claim_touches(TouchClaim::on_start())
                .keep_touches(true)
                .on_responder_grant(grant)
                .on_responder_move(moved)
                .on_responder_release(released)
                .on_responder_terminate(released)
                .on_hover_move(move |e: &HoverEvent| {
                    let (x, y) = (e.position.x.0, e.position.y.0);
                    let o = (e.page_position.x.0 - x, e.page_position.y.0 - y);
                    if g.origin.get_untracked() != o {
                        g.origin.set(o);
                    }
                    if g.drag.get_untracked() != Drag::None {
                        // Hover moves come with no button down: the release was missed.
                        on_up(g);
                    }
                    let h = g.hit(x, y);
                    let row = match &h {
                        Hit::Cell(r, _) | Hit::CellCheckbox(r, _) | Hit::CellAdd(r, _) | Hit::CellRating(r, _, _)
                        | Hit::RowCheck(r) | Hit::RowExpand(r) | Hit::RowGrip(r) | Hit::RowNum(r) => Some(*r),
                        _ => None,
                    };
                    if g.hover.get_untracked() != h {
                        g.hover.set(h);
                    }
                    if g.hover_row.get_untracked() != row {
                        g.hover_row.set(row);
                    }
                })
                .on_hover_out(move || {
                    if g.drag.get_untracked() == Drag::None {
                        g.hover.set(Hit::None);
                        g.hover_row.set(None);
                    }
                })
                .on_context_menu(move |e: &ContextMenuEvent| {
                    let (x, y) = (e.position.x.0, e.position.y.0);
                    g.origin.set((e.page_position.x.0 - x, e.page_position.y.0 - y));
                    context_menu(g, x, y, e.page_position.x.0, e.page_position.y.0);
                })
                .on_file_drop(move |e: &FileDropEvent| {
                    let paths: Vec<String> = e.uris.iter().filter_map(|u| edit::path_from_url(u)).collect();
                    edit::drop_files(g, paths);
                })
                .on_layout(move |b: &Bounds<Points>| g.size.set((b.size.width.0, b.size.height.0))),
        )
        // The text field: invisible while the grid only has the selection
        // (it takes the typing), the cell's editor while editing.
        .child(
            view()
                .absolute()
                .left(move || px(editor_rect().0 - if text_editing() { 1.0 } else { 0.0 }))
                .top(move || px(editor_rect().1 - if text_editing() { 1.0 } else { 0.0 }))
                .w(move || px(if text_editing() { editor_rect().2.max(160.0) + 2.0 } else { 2.0 }))
                .min_h(move || px(if text_editing() { editor_rect().3 + 2.0 } else { 2.0 }))
                .max_h(px(260.0))
                .bg(move || if text_editing() { rgb(BG) } else { rgba(0xFFFFFF00) })
                .border(move || px(if text_editing() { 2.0 } else { 0.0 }))
                .border_color(rgb(PRIMARY))
                .opacity(move || if text_editing() { 1.0 } else { 0.0 })
                .pointer_events(move || if text_editing() { PointerEvents::Auto } else { PointerEvents::None })
                .child(
                    nsh_core::element::input()
                        .element_ref(&g.input)
                        .w_full()
                        .min_h(move || px(if text_editing() { (editor_rect().3 - 2.0).max(28.0) } else { 2.0 }))
                        .px(px(7.0))
                        .py(px(6.0))
                        .text_size(px(14.0))
                        .text_color(rgb(TEXT))
                        .multiline(move || multiline())
                        .caret(move || text_editing())
                        .value(move || g.edit_text.get())
                        .on_focus(move || g.focused.set(true))
                        .on_blur(move || {
                            g.focused.set(false);
                        })
                        .on_change(move |t: &str| edit::typed(g, t))
                        .on_composition_change(move |c: &nsh_core::element::Composition| {
                            if c.composing && g.editing.get_untracked().is_none() {
                                edit::begin_typing(g);
                            }
                        }),
                ),
        )
        .child(dynamic(move || edit::list_editor(g)))
}

fn double(g: Grid, hit: &Hit) -> bool {
    let t = now_ms();
    let (last_t, last_hit) = g.last_press.with(|l| l.clone());
    g.last_press.with(|l| *l = (t, hit.clone()));
    t - last_t < 450.0 && &last_hit == hit
}

fn on_down(g: Grid, x: f32, y: f32) {
    let s = st();
    let hit = g.hit(x, y);
    let dbl = double(g, &hit);
    let (_ctrl, shift, _alt) = win::modifiers();
    // A press outside the editing cell ends the edit.
    if g.editing.get_untracked().is_some() {
        edit::commit(g);
    }
    let l = g.layout.get_untracked();
    match hit.clone() {
        Hit::Corner => {
            let all = !l.record_ids.is_empty() && l.record_ids.iter().all(|id| s.checked.get_untracked().contains(id));
            s.checked.set(if all { vec![] } else { l.record_ids.clone() });
        }
        Hit::Header(c) => {
            if dbl {
                let (px_, py_) = g.origin.get_untracked();
                s.popup.set(Some(Popup::FieldEditor {
                    field: Some(l.cols[c].field.clone()),
                    at: None,
                    x: px_ + g.col_x(&l, c),
                    y: py_ + HEAD,
                }));
                return;
            }
            let first = l.first_record();
            let last = l.last_record();
            if let (Some(a), Some(b)) = (first, last) {
                let mut sel = Sel { ar: a, ac: c, fr: b, fc: c, cols: true };
                if shift {
                    if let Some(old) = g.sel.get_untracked().filter(|s| s.cols) {
                        sel.ac = old.ac;
                    }
                }
                g.sel.set(Some(sel));
            }
            g.drag.set(Drag::MoveCol { col: c, start_x: x, to: c, moved: false });
        }
        Hit::HeaderMenu(c) => {
            let (ox, oy) = g.origin.get_untracked();
            s.popup.set(Some(Popup::FieldMenu { field: l.cols[c].field.clone(), x: ox + g.col_x(&l, c) + l.cols[c].w - 28.0, y: oy + HEAD }));
        }
        Hit::Resize(c) => {
            g.drag.set(Drag::Resize { col: c, start_x: x, start_w: l.cols[c].w });
        }
        Hit::AddField => {
            let (ox, oy) = g.origin.get_untracked();
            s.popup.set(Some(Popup::FieldEditor { field: None, at: None, x: ox + x - 40.0, y: oy + HEAD }));
        }
        Hit::RowCheck(r) => {
            if let Some(i) = l.records.iter().position(|&ri| Some(ri) == l.record_at(r)) {
                let id = l.record_ids[i].clone();
                s.checked.update(|c| {
                    if let Some(p) = c.iter().position(|x| *x == id) {
                        c.remove(p);
                    } else {
                        c.push(id);
                    }
                });
            }
        }
        Hit::RowExpand(r) => {
            if let Some(i) = l.records.iter().position(|&ri| Some(ri) == l.record_at(r)) {
                s.expanded.set(Some((l.table.clone(), l.record_ids[i].clone())));
            }
        }
        Hit::RowGrip(r) => {
            if l.sorted {
                s.toast("视图已设置排序，无法拖动记录");
            } else if l.grouped {
                s.toast("视图已设置分组，无法拖动记录");
            } else {
                g.sel.set(Some(Sel { ar: r, ac: 0, fr: r, fc: l.cols.len().saturating_sub(1), cols: false }));
                g.drag.set(Drag::MoveRow { row: r, to: r, start_y: y, moved: false });
            }
        }
        Hit::RowNum(r) => {
            g.sel.set(Some(Sel { ar: r, ac: 0, fr: r, fc: l.cols.len().saturating_sub(1), cols: false }));
        }
        Hit::Cell(r, c) => {
            let cur = g.sel.get_untracked();
            if shift && cur.is_some() {
                let mut sel = cur.unwrap();
                sel.fr = r;
                sel.fc = c;
                sel.cols = false;
                g.sel.set(Some(sel));
            } else if dbl || cur.is_some_and(|s| s.single() && !s.cols && s.ar == r && s.ac == c) {
                g.sel.set(Some(Sel::cell(r, c)));
                edit::begin(g, None);
            } else {
                g.sel.set(Some(Sel::cell(r, c)));
                g.drag.set(Drag::Select);
            }
            s.checked.set(vec![]);
        }
        Hit::CellCheckbox(r, c) => {
            g.sel.set(Some(Sel::cell(r, c)));
            edit::toggle_checkbox(g, r, c);
        }
        Hit::CellRating(r, c, n) => {
            g.sel.set(Some(Sel::cell(r, c)));
            if !dbl {
                edit::set_rating(g, r, c, n);
            }
        }
        Hit::CellAdd(r, c) => {
            g.sel.set(Some(Sel::cell(r, c)));
            edit::begin(g, None);
        }
        Hit::AddRow(r) => {
            let preset = match &l.rows[r] {
                Row::Add { preset, .. } => preset.clone(),
                _ => vec![],
            };
            edit::add_record(g, preset, None);
        }
        Hit::Group(r) => {
            if let Row::Group { key, .. } = &l.rows[r] {
                let key = key.clone();
                s.edit_view(|v| {
                    if let Some(p) = v.collapsed.iter().position(|k| *k == key) {
                        v.collapsed.remove(p);
                    } else {
                        v.collapsed.push(key);
                    }
                });
            }
        }
        Hit::Stat(c) => {
            let (ox, oy) = g.origin.get_untracked();
            let (_, h) = g.size.get_untracked();
            s.popup.set(Some(Popup::Stat { field: l.cols[c].field.clone(), x: ox + g.col_x(&l, c), y: oy + h - STATS }));
        }
        Hit::Fill => {
            let to = g.sel.get_untracked().map(|s| s.rows().1).unwrap_or(0);
            g.drag.set(Drag::Fill { to_row: to });
        }
        Hit::VBar => g.drag.set(Drag::VBar { start_y: y, start_sy: g.sy.get_untracked() }),
        Hit::HBar => g.drag.set(Drag::HBar { start_x: x, start_sx: g.sx.get_untracked() }),
        Hit::StatCount | Hit::None => {}
    }
    if !matches!(hit, Hit::Corner | Hit::RowCheck(_)) && s.popup.get_untracked().is_none() {
        g.input.focus();
    }
}

fn on_move(g: Grid, x: f32, y: f32) {
    let l = g.layout.get_untracked();
    let (w, h) = g.size.get_untracked();
    match g.drag.get_untracked() {
        Drag::None => {}
        Drag::Select => {
            // Scroll when the pointer passes the edges.
            if y > h - STATS - 10.0 {
                g.scroll_by(0.0, 20.0);
            } else if y < HEAD + 10.0 {
                g.scroll_by(0.0, -20.0);
            }
            if x > w - 20.0 {
                g.scroll_by(20.0, 0.0);
            } else if x < RH + l.frozen_w + 10.0 && g.sx.get_untracked() > 0.0 && x > RH + l.frozen_w - 30.0 {
                g.scroll_by(-20.0, 0.0);
            }
            let cy = (y.clamp(HEAD + 1.0, h - STATS - 1.0)) - HEAD + g.sy.get_untracked();
            let r = l.row_at_y(cy).and_then(|r| l.next_record(r as isize, -1).or(l.next_record(r as isize, 1)));
            let c = g.col_at_x(&l, x.max(RH + 1.0)).or(if x > RH { Some(l.cols.len().saturating_sub(1)) } else { None });
            if let (Some(r), Some(c), Some(mut sel)) = (r, c, g.sel.get_untracked()) {
                if sel.fr != r || sel.fc != c {
                    sel.fr = r;
                    sel.fc = c;
                    g.sel.set(Some(sel));
                }
            }
        }
        Drag::Resize { col, start_x, start_w } => {
            let nw = (start_w + x - start_x).clamp(60.0, 900.0);
            g.resize_w.set(Some((col, nw)));
        }
        Drag::MoveCol { col, start_x, moved, .. } => {
            let moved = moved || (x - start_x).abs() > 5.0;
            let to = g
                .col_at_x(&l, x)
                .map(|c| {
                    let (cx, _, cw, _) = g.cell_rect(&l, 0.min(l.rows.len().saturating_sub(1)), c);
                    if x > cx + cw / 2.0 { c + 1 } else { c }
                })
                .unwrap_or(if x < RH { 0 } else { l.cols.len() });
            let to = to.max(1); // The primary field stays first.
            g.drag.set(Drag::MoveCol { col, start_x, to, moved });
        }
        Drag::MoveRow { row, start_y, moved, .. } => {
            let moved = moved || (y - start_y).abs() > 4.0;
            if y > h - STATS - 10.0 {
                g.scroll_by(0.0, 20.0);
            } else if y < HEAD + 10.0 {
                g.scroll_by(0.0, -20.0);
            }
            let cy = y - HEAD + g.sy.get_untracked();
            let to = match l.row_at_y(cy) {
                Some(r) if l.is_record(r) => {
                    if cy > l.tops[r] + l.heights[r] / 2.0 { r + 1 } else { r }
                }
                Some(r) => r,
                None => if cy < 0.0 { 0 } else { l.rows.len().saturating_sub(1) },
            };
            g.drag.set(Drag::MoveRow { row, to, start_y, moved });
        }
        Drag::Fill { .. } => {
            if y > h - STATS - 10.0 {
                g.scroll_by(0.0, 20.0);
            }
            let cy = y - HEAD + g.sy.get_untracked();
            let r1 = g.sel.get_untracked().map(|s| s.rows().1).unwrap_or(0);
            let to = l.row_at_y(cy).and_then(|r| l.next_record(r as isize, -1)).unwrap_or(r1).max(r1);
            g.drag.set(Drag::Fill { to_row: to });
        }
        Drag::VBar { start_y, start_sy } => {
            let track = h - STATS - HEAD - 4.0;
            let thumb = (track * (track / (l.content_h + 40.0))).max(30.0);
            let max = g.max_sy(&l);
            let ny = (start_sy + (y - start_y) * max / (track - thumb).max(1.0)).clamp(0.0, max);
            g.sy.set(ny);
        }
        Drag::HBar { start_x, start_sx } => {
            let track = w - RH - l.frozen_w - 12.0;
            let full = l.content_w - l.frozen_w + 40.0;
            let thumb = (track * (track / full)).max(30.0);
            let max = g.max_sx(&l);
            let nx = (start_sx + (x - start_x) * max / (track - thumb).max(1.0)).clamp(0.0, max);
            g.sx.set(nx);
        }
    }
}

fn on_up(g: Grid) {
    let s = st();
    let l = g.layout.get_untracked();
    match g.drag.get_untracked() {
        Drag::Resize { col, .. } => {
            if let Some((c, w)) = g.resize_w.get_untracked() {
                let fid = l.cols[c].field.clone();
                let _ = col;
                s.edit_view(|v| {
                    v.widths.insert(fid, w);
                });
            }
            g.resize_w.set(None);
        }
        Drag::MoveCol { col, to, moved: true, .. } => {
            if to != col && to != col + 1 && col != 0 {
                let moving = l.cols[col].field.clone();
                let target = l.cols.get(to).map(|c| c.field.clone());
                s.edit(|b| {
                    let cur = s.cur.get_untracked();
                    let vid = s.view.get_untracked();
                    let Some(t) = b.table_mut(&cur) else { return };
                    let all: Vec<_> = {
                        let v = t.view(&vid).cloned();
                        match v {
                            Some(v) => t.view_fields(&v).into_iter().map(|(f, _)| f.id.clone()).collect(),
                            None => vec![],
                        }
                    };
                    let mut order = all;
                    order.retain(|f| *f != moving);
                    let at = target.and_then(|tg| order.iter().position(|f| *f == tg)).unwrap_or(order.len());
                    order.insert(at, moving.clone());
                    if let Some(v) = t.view_mut(&vid) {
                        v.field_order = order;
                    }
                });
                let new_col = if to > col { to - 1 } else { to };
                let l2 = g.layout.get_untracked();
                if let (Some(a), Some(b)) = (l2.first_record(), l2.last_record()) {
                    g.sel.set(Some(Sel { ar: a, ac: new_col, fr: b, fc: new_col, cols: true }));
                }
            }
        }
        Drag::MoveRow { row, to, moved: true, .. } => {
            if to != row && to != row + 1 {
                if let Some(ri) = l.record_at(row) {
                    let order_idx: Vec<usize> = l.records.clone();
                    let ids: Vec<String> = l.record_ids.clone();
                    let moving = ids[order_idx.iter().position(|x| *x == ri).unwrap()].clone();
                    // The record before which it drops.
                    let target = (to..l.rows.len()).find_map(|r| l.record_at(r)).and_then(|ti| order_idx.iter().position(|x| *x == ti)).map(|p| ids[p].clone());
                    s.edit(|b| {
                        let cur = s.cur.get_untracked();
                        let vid = s.view.get_untracked();
                        let Some(t) = b.table_mut(&cur) else { return };
                        let all: Vec<String> = t.records.iter().map(|r| r.id.clone()).collect();
                        let Some(v) = t.view_mut(&vid) else { return };
                        // The view's full order: its manual order, then the rest.
                        let mut order: Vec<String> = v.row_order.iter().filter(|id| all.contains(id)).cloned().collect();
                        for id in &all {
                            if !order.contains(id) {
                                order.push(id.clone());
                            }
                        }
                        order.retain(|id| *id != moving);
                        let at = target.as_ref().and_then(|tg| order.iter().position(|x| x == tg)).unwrap_or(order.len());
                        order.insert(at, moving.clone());
                        v.row_order = order;
                    });
                    g.select_record(&moving, 0);
                }
            }
        }
        Drag::Fill { to_row } => edit::fill(g, to_row),
        _ => {}
    }
    g.drag.set(Drag::None);
}

fn context_menu(g: Grid, x: f32, y: f32, px_: f32, py_: f32) {
    let s = st();
    if g.editing.get_untracked().is_some() {
        edit::commit(g);
    }
    let l = g.layout.get_untracked();
    match g.hit(x, y) {
        Hit::Header(c) | Hit::HeaderMenu(c) | Hit::Resize(c) => {
            s.popup.set(Some(Popup::FieldMenu { field: l.cols[c].field.clone(), x: px_, y: py_ }));
        }
        Hit::Cell(r, c) | Hit::CellCheckbox(r, c) | Hit::CellAdd(r, c) | Hit::CellRating(r, c, _) => {
            let inside = g.sel.get_untracked().is_some_and(|sel| {
                let (r0, r1) = sel.rows();
                let (c0, c1) = sel.cols();
                r >= r0 && r <= r1 && c >= c0 && c <= c1
            });
            if !inside {
                g.sel.set(Some(Sel::cell(r, c)));
            }
            s.popup.set(Some(Popup::RecordMenu { x: px_, y: py_ }));
        }
        Hit::RowCheck(r) | Hit::RowExpand(r) | Hit::RowGrip(r) | Hit::RowNum(r) => {
            if !g.sel.get_untracked().is_some_and(|sel| r >= sel.rows().0 && r <= sel.rows().1) {
                g.sel.set(Some(Sel { ar: r, ac: 0, fr: r, fc: l.cols.len().saturating_sub(1), cols: false }));
            }
            s.popup.set(Some(Popup::RecordMenu { x: px_, y: py_ }));
        }
        _ => {}
    }
    let _ = Value::Bool(true);
}
