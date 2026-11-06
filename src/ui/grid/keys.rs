//! The grid's keys: moving the selection, editing, deleting, copying,
//! cutting and pasting, undo.

use nsh_core::prelude::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

use super::{edit, *};
use crate::{
    model::{FieldKind, value},
    ui::state::{Confirm, Popup, st},
    win::Key,
};

fn move_sel(g: Grid, dr: isize, dc: isize, extend: bool, jump: bool) {
    let l = g.layout.get_untracked();
    let Some(mut sel) = g.sel.get_untracked() else {
        if let Some(r) = l.first_record() {
            g.sel.set(Some(Sel::cell(r, 0)));
        }
        return;
    };
    let (mut r, mut c) = if extend { (sel.fr, sel.fc) } else { (sel.ar, sel.ac) };
    if dc != 0 {
        let nc = if jump {
            if dc < 0 { 0 } else { l.cols.len() as isize - 1 }
        } else {
            c as isize + dc
        };
        c = nc.clamp(0, l.cols.len() as isize - 1) as usize;
    }
    if dr != 0 {
        let next = if jump {
            if dr < 0 { l.first_record() } else { l.last_record() }
        } else {
            let mut steps = dr.unsigned_abs();
            let mut cur = r as isize;
            let dir = dr.signum();
            let mut last_ok = r;
            while steps > 0 {
                cur += dir;
                if cur < 0 || cur as usize >= l.rows.len() {
                    break;
                }
                if l.is_record(cur as usize) {
                    last_ok = cur as usize;
                    steps -= 1;
                }
            }
            Some(last_ok)
        };
        if let Some(n) = next {
            r = n;
        }
    }
    if extend {
        sel.fr = r;
        sel.fc = c;
        sel.cols = false;
    } else {
        sel = Sel::cell(r, c);
    }
    g.sel.set(Some(sel));
    g.reveal(r, c);
}

/// Handles a key the window thread took for the grid.
pub fn key(g: Grid, k: Key) {
    let s = st();
    // A list editor (options, a date...) has the keys first.
    if let Some(e) = g.editing.get_untracked() {
        if e.mode != EditMode::Text {
            edit::list_key(g, k);
            return;
        }
        match k.vk {
            v if v == VK_RETURN.0 => {
                edit::commit(g);
                if k.shift {
                    move_sel(g, 1, 0, false, false);
                }
            }
            v if v == VK_TAB.0 => {
                edit::commit(g);
                move_sel(g, 0, if k.shift { -1 } else { 1 }, false, false);
            }
            v if v == VK_ESCAPE.0 => edit::commit(g),
            _ => {}
        }
        return;
    }
    let l = g.layout.get_untracked();
    let page = ((g.body_h() / l.row_h.max(1.0)) as isize - 1).max(1);
    match k.vk {
        v if v == VK_UP.0 => move_sel(g, -1, 0, k.shift, k.ctrl),
        v if v == VK_DOWN.0 => move_sel(g, 1, 0, k.shift, k.ctrl),
        v if v == VK_LEFT.0 => move_sel(g, 0, -1, k.shift, k.ctrl),
        v if v == VK_RIGHT.0 => move_sel(g, 0, 1, k.shift, k.ctrl),
        v if v == VK_PRIOR.0 => move_sel(g, -page, 0, k.shift, false),
        v if v == VK_NEXT.0 => move_sel(g, page, 0, k.shift, false),
        v if v == VK_HOME.0 => {
            if k.ctrl {
                move_sel(g, -1, -1, k.shift, true)
            } else {
                move_sel(g, 0, -1, k.shift, true)
            }
        }
        v if v == VK_END.0 => {
            if k.ctrl {
                move_sel(g, 1, 1, k.shift, true)
            } else {
                move_sel(g, 0, 1, k.shift, true)
            }
        }
        v if v == VK_TAB.0 => {
            let Some(sel) = g.sel.get_untracked() else { return };
            let last = l.cols.len().saturating_sub(1);
            if !k.shift && sel.ac == last {
                // Past the last column: the next record's first.
                g.sel.set(Some(Sel::cell(sel.ar, 0)));
                move_sel(g, 1, 0, false, false);
            } else if k.shift && sel.ac == 0 {
                g.sel.set(Some(Sel::cell(sel.ar, last)));
                move_sel(g, -1, 0, false, false);
            } else {
                move_sel(g, 0, if k.shift { -1 } else { 1 }, false, false);
            }
        }
        v if v == VK_RETURN.0 => {
            if k.shift {
                // Shift+Enter: a new record below.
                if let Some((rid, _, _, _)) = g.active() {
                    edit::add_record(g, vec![], Some((rid, true)));
                }
            } else {
                edit::begin(g, None);
            }
        }
        v if v == VK_F2.0 => edit::begin(g, None),
        v if v == VK_SPACE.0 => {
            if let Some((rid, _, _, c)) = g.active() {
                if matches!(l.cols[c].kind, FieldKind::Checkbox) {
                    edit::begin(g, None);
                } else {
                    s.expanded.set(Some((l.table.clone(), rid)));
                }
            }
        }
        v if v == VK_ESCAPE.0 => {
            if !s.checked.get_untracked().is_empty() {
                s.checked.set(vec![]);
            } else if g.sel.get_untracked().is_some_and(|s| !s.single()) {
                let sel = g.sel.get_untracked().unwrap();
                g.sel.set(Some(Sel::cell(sel.ar, sel.ac)));
            } else {
                g.sel.set(None);
            }
        }
        v if v == VK_DELETE.0 || v == VK_BACK.0 => {
            if !s.checked.get_untracked().is_empty() {
                let ids = s.checked.get_untracked();
                s.popup.set(Some(Popup::Confirm {
                    title: "删除记录".into(),
                    text: format!("确定要删除选中的 {} 条记录吗？", ids.len()),
                    action: Confirm::DeleteRecords(ids),
                }));
            } else {
                clear(g);
            }
        }
        v if k.ctrl && v == b'C' as u16 => copy(g, false),
        v if k.ctrl && v == b'X' as u16 => copy(g, true),
        v if k.ctrl && v == b'V' as u16 => paste(g),
        v if k.ctrl && v == b'Z' as u16 => {
            if k.shift {
                s.redo()
            } else {
                s.undo()
            }
        }
        v if k.ctrl && v == b'Y' as u16 => s.redo(),
        v if k.ctrl && v == b'A' as u16 => {
            if let (Some(a), Some(b)) = (l.first_record(), l.last_record()) {
                g.sel.set(Some(Sel { ar: a, ac: 0, fr: b, fc: l.cols.len().saturating_sub(1), cols: false }));
            }
        }
        v if k.ctrl && v == b'F' as u16 => s.search_open.set(true),
        v if k.ctrl && v == b'D' as u16 => {
            // Fill down: the range's first row into the rest.
            if let Some(sel) = g.sel.get_untracked() {
                let (r0, r1) = sel.rows();
                if r1 > r0 {
                    let mut one = sel;
                    one.ar = r0;
                    one.fr = r0;
                    g.sel.set(Some(one));
                    edit::fill(g, r1);
                    g.sel.set(Some(sel));
                }
            }
        }
        _ => {}
    }
}

/// The selection's cells: (record ids, columns) in the range.
fn range(g: Grid) -> Option<(Vec<Id>, Vec<usize>)> {
    let sel = g.sel.get_untracked()?;
    let l = g.layout.get_untracked();
    let (r0, r1) = sel.rows();
    let (c0, c1) = sel.cols();
    let ids = st().peek(|b| {
        let t = b.table(&l.table)?;
        Some((r0..=r1).filter_map(|r| l.record_at(r)).map(|i| t.records[i].id.clone()).collect::<Vec<_>>())
    })?;
    Some((ids, (c0..=c1).collect()))
}

/// Clears the selected cells.
pub fn clear(g: Grid) {
    let Some((ids, cols)) = range(g) else { return };
    let l = g.layout.get_untracked();
    let mut cells = vec![];
    for id in &ids {
        for &c in &cols {
            if !l.cols[c].kind.is_computed() {
                let v = if matches!(l.cols[c].kind, FieldKind::Checkbox) { Some(crate::model::Value::Bool(false)) } else { None };
                cells.push((id.clone(), l.cols[c].field.clone(), v));
            }
        }
    }
    st().set_cells(&l.table, &cells);
}

/// The selection as tab-separated text.
pub fn selection_text(g: Grid) -> String {
    let Some((ids, cols)) = range(g) else { return String::new() };
    let l = g.layout.get_untracked();
    st().peek(|b| {
        let Some(t) = b.table(&l.table) else { return String::new() };
        let mut out = vec![];
        for id in &ids {
            let Some(r) = t.record(id) else { continue };
            let row: Vec<String> = cols
                .iter()
                .map(|&c| {
                    let f = t.field(&l.cols[c].field);
                    f.and_then(|f| crate::model::query::cell(b, t, r, f).map(|v| value::display(b, t, &f.kind, &v)))
                        .unwrap_or_default()
                })
                .map(|s| if s.contains(['\t', '\n', '"']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s })
                .collect();
            out.push(row.join("\t"));
        }
        out.join("\n")
    })
}

pub fn copy(g: Grid, cut: bool) {
    let text = selection_text(g);
    if text.is_empty() && range(g).is_none() {
        return;
    }
    set_clipboard(text);
    let n = range(g).map(|(r, c)| r.len() * c.len()).unwrap_or(0);
    st().toast(&if cut { format!("已剪切 {n} 个单元格") } else { format!("已复制 {n} 个单元格") });
    if cut {
        clear(g);
    }
}

/// Splits tab-separated text (quotes respected) into rows of cells.
pub fn parse_tsv(text: &str) -> Vec<Vec<String>> {
    let mut rows = vec![];
    let mut row = vec![];
    let mut cell = String::new();
    let mut quoted = false;
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if quoted {
            if c == '"' {
                if chars.get(i + 1) == Some(&'"') {
                    cell.push('"');
                    i += 1;
                } else {
                    quoted = false;
                }
            } else {
                cell.push(c);
            }
        } else {
            match c {
                '"' if cell.is_empty() => quoted = true,
                '\t' => row.push(std::mem::take(&mut cell)),
                '\r' => {}
                '\n' => {
                    row.push(std::mem::take(&mut cell));
                    rows.push(std::mem::take(&mut row));
                }
                _ => cell.push(c),
            }
        }
        i += 1;
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows
}

pub fn paste(g: Grid) {
    spawn(async move {
        let text = clipboard_string().await;
        if text.is_empty() {
            return;
        }
        paste_text(g, &text);
    });
}

/// Pastes `text` at the active cell, adding records past the last.
pub fn paste_text(g: Grid, text: &str) {
    let s = st();
    let Some(sel) = g.sel.get_untracked() else { return };
    let l = g.layout.get_untracked();
    let data = parse_tsv(text);
    if data.is_empty() {
        return;
    }
    // A single value pasted over a range fills it.
    let (r0, r1) = sel.rows();
    let (c0, c1) = sel.cols();
    let single = data.len() == 1 && data[0].len() == 1;
    let target_rows: Vec<usize> = (r0..l.rows.len()).filter(|r| l.is_record(*r)).collect();
    let rows_needed = if single { (r0..=r1).filter(|r| l.is_record(*r)).count() } else { data.len() };
    let cols_needed = if single { c1 - c0 + 1 } else { data.iter().map(Vec::len).max().unwrap_or(0) };
    let start_order: Vec<Id> = target_rows
        .iter()
        .take(rows_needed)
        .filter_map(|r| l.record_at(*r))
        .filter_map(|i| s.peek(|b| b.table(&l.table).map(|t| t.records[i].id.clone())))
        .collect();
    let extra = rows_needed.saturating_sub(start_order.len());
    let table = l.table.clone();
    let vid = l.view.clone();
    let cols: Vec<Id> = (c0..(c0 + cols_needed).min(l.cols.len())).map(|c| l.cols[c].field.clone()).collect();
    let mut written = 0;
    s.edit(|b| {
        let me = b.me.clone();
        let mut ids = start_order.clone();
        if let Some(t) = b.table_mut(&table) {
            for _ in 0..extra {
                let r = t.new_record(&me);
                ids.push(r.id.clone());
                t.records.push(r);
            }
            if extra > 0 {
                let all: Vec<Id> = t.records.iter().map(|r| r.id.clone()).collect();
                if let Some(v) = t.view_mut(&vid) {
                    if !v.row_order.is_empty() {
                        for id in all {
                            if !v.row_order.contains(&id) {
                                v.row_order.push(id);
                            }
                        }
                    }
                }
            }
        }
        let snapshot = b.clone();
        let Some(t) = b.table_mut(&table) else { return };
        for (ri, id) in ids.iter().enumerate() {
            for (ci, fid) in cols.iter().enumerate() {
                let text = if single { data[0][0].clone() } else { data.get(ri).and_then(|r| r.get(ci)).cloned().unwrap_or_default() };
                let Some(fpos) = t.fields.iter().position(|f| &f.id == fid) else { continue };
                if t.fields[fpos].kind.is_computed() {
                    continue;
                }
                let mut kind = t.fields[fpos].kind.clone();
                let v = value::parse(&snapshot, &mut kind, &text);
                t.fields[fpos].kind = kind;
                if let Some(r) = t.record_mut(id) {
                    match v {
                        Some(v) => {
                            r.cells.insert(fid.clone(), v);
                        }
                        None => {
                            r.cells.remove(fid);
                        }
                    }
                    r.updated_at = crate::model::now_ms();
                    r.updated_by = me.clone();
                    written += 1;
                }
            }
        }
    });
    let _ = written;
    // The pasted range stays selected.
    let l2 = g.layout.get_untracked();
    let last_row = (r0..l2.rows.len()).filter(|r| l2.is_record(*r)).take(rows_needed).last().unwrap_or(r0);
    g.sel.set(Some(Sel { ar: r0, ac: c0, fr: last_row, fc: (c0 + cols_needed.max(1) - 1).min(l2.cols.len().saturating_sub(1)), cols: false }));
}

#[cfg(test)]
mod tests {
    #[test]
    fn tsv() {
        let rows = super::parse_tsv("a\tb\n\"x\ty\"\t\"q\"\"z\"\n");
        assert_eq!(rows, vec![vec!["a", "b"], vec!["x\ty", "q\"z"]]);
    }
}
