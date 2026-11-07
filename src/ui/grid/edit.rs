//! Editing cells: the text editor, the option / date / people / record
//! pickers, checkboxes, ratings, attachments, adding records and filling.

use nsh_core::prelude::*;

use super::*;
use crate::{
    model::{
        FieldKind, FilterValue, Id, Op, SelectOption, Value, new_id,
        query::Row,
        value::{self, tag_color},
    },
    ui::{
        cells, icons,
        state::st,
        theme::*,
        widgets::{self, avatar_el, tag_el},
    },
};

pub fn mode_of(kind: &FieldKind) -> Option<EditMode> {
    Some(match kind {
        FieldKind::Text
        | FieldKind::Number { .. }
        | FieldKind::Currency { .. }
        | FieldKind::Progress { .. }
        | FieldKind::Rating { .. }
        | FieldKind::Phone
        | FieldKind::Email
        | FieldKind::Url
        | FieldKind::Barcode
        | FieldKind::Location => EditMode::Text,
        FieldKind::SingleSelect { .. } | FieldKind::MultiSelect { .. } => EditMode::Select,
        FieldKind::Date { .. } => EditMode::Date,
        FieldKind::User { .. } => EditMode::User,
        FieldKind::Link { .. } => EditMode::Link,
        FieldKind::Attachment => EditMode::Attachment,
        _ => return None,
    })
}

/// The text a cell's editor starts with.
pub fn edit_text_of(kind: &FieldKind, v: Option<&Value>) -> String {
    match (kind, v) {
        (_, None) => String::new(),
        (FieldKind::Number { percent: true, .. }, Some(Value::Number(n))) => format!("{}%", value::format_plain(n * 100.0)),
        (FieldKind::Progress { .. }, Some(Value::Number(n))) => value::format_plain((n * 100.0).round()),
        (_, Some(Value::Number(n))) => value::format_plain(*n),
        (_, Some(Value::Text(s))) => s.clone(),
        (_, Some(Value::Url { link, .. })) => link.clone(),
        _ => String::new(),
    }
}

/// Starts editing the active cell; `typed` is the text typed to start it.
pub fn begin(g: Grid, typed: Option<String>) {
    let s = st();
    let Some((rid, fid, r, c)) = g.active() else { return };
    let l = g.layout.get_untracked();
    let kind = l.cols[c].kind.clone();
    if matches!(kind, FieldKind::Checkbox) {
        toggle_checkbox(g, r, c);
        return;
    }
    let Some(mode) = mode_of(&kind) else {
        s.toast(&format!("「{}」字段为自动计算，不支持编辑", l.cols[c].name));
        g.edit_text.set(String::new());
        return;
    };
    g.reveal(r, c);
    if mode == EditMode::Attachment {
        pick_files(g, rid, fid);
        return;
    }
    let current = s.peek(|b| {
        let t = b.table(&l.table)?;
        let rec = t.record(&rid)?;
        let f = t.field(&fid)?;
        crate::model::query::cell(b, t, rec, f)
    });
    match mode {
        EditMode::Text => {
            let text = typed.unwrap_or_else(|| edit_text_of(&kind, current.as_ref()));
            let n = text.encode_utf16().count() as u32;
            g.edit_text.set(text);
            g.editing.set(Some(Editing { row: r, col: c, record: rid, field: fid, mode }));
            g.input.focus();
            g.input.set_selection(n, n);
        }
        _ => {
            g.edit_text.set(String::new());
            g.list_query.set(typed.unwrap_or_default());
            g.list_index.set(0);
            g.editing.set(Some(Editing { row: r, col: c, record: rid, field: fid, mode }));
        }
    }
}

/// The hidden field took text while the grid only had a selection: typing
/// into a cell starts editing it with what was typed.
pub fn typed(g: Grid, t: &str) {
    match g.editing.get_untracked() {
        Some(e) if e.mode == EditMode::Text => g.edit_text.set(t.to_string()),
        Some(_) => {}
        None => {
            if t.is_empty() {
                return;
            }
            if g.sel.get_untracked().is_none() {
                g.edit_text.set(String::new());
                return;
            }
            let text = t.to_string();
            g.edit_text.set(String::new());
            begin(g, Some(text));
        }
    }
}

/// An input method started composing over a selected cell.
pub fn begin_typing(g: Grid) {
    let Some((rid, fid, r, c)) = g.active() else { return };
    let l = g.layout.get_untracked();
    if mode_of(&l.cols[c].kind) == Some(EditMode::Text) {
        g.edit_text.set(String::new());
        g.editing.set(Some(Editing { row: r, col: c, record: rid, field: fid, mode: EditMode::Text }));
    }
}

/// Ends editing, saving the text.
pub fn commit(g: Grid) {
    let Some(e) = g.editing.get_untracked() else { return };
    let s = st();
    if e.mode == EditMode::Text {
        let text = g.edit_text.get_untracked();
        let l = g.layout.get_untracked();
        let table = l.table.clone();
        let result = s.peek(|b| {
            let t = b.table(&table)?;
            let f = t.field(&e.field)?;
            let mut kind = f.kind.clone();
            let old = t.record(&e.record).and_then(|r| r.cells.get(&e.field).cloned());
            let v = match (&kind, &old) {
                // A link's text keeps its address and the reverse.
                (FieldKind::Url, _) if text.trim().is_empty() => None,
                _ => value::parse(b, &mut kind, &text),
            };
            let invalid = !text.trim().is_empty() && v.is_none();
            Some((v, old, invalid, f.kind.is_numeric()))
        });
        if let Some((v, old, invalid, numeric)) = result {
            if invalid && numeric {
                s.toast("请输入数字");
            } else if v != old {
                s.set_cells(&table, &[(e.record.clone(), e.field.clone(), v)]);
            }
        }
    }
    end(g);
}

/// Ends editing without saving the text.
pub fn end(g: Grid) {
    g.editing.set(None);
    g.edit_text.set(String::new());
    g.list_query.set(String::new());
    g.input.focus();
}

pub fn toggle_checkbox(g: Grid, r: usize, c: usize) {
    let s = st();
    let l = g.layout.get_untracked();
    let Some(ri) = l.record_at(r) else { return };
    let fid = l.cols[c].field.clone();
    let (rid, on) = s.peek(|b| {
        let t = b.table(&l.table).unwrap();
        let rec = &t.records[ri];
        (rec.id.clone(), matches!(rec.cells.get(&fid), Some(Value::Bool(true))))
    });
    s.set_cells(&l.table, &[(rid, fid, Some(Value::Bool(!on)))]);
}

pub fn set_rating(g: Grid, r: usize, c: usize, n: u8) {
    let s = st();
    let l = g.layout.get_untracked();
    let Some(ri) = l.record_at(r) else { return };
    let fid = l.cols[c].field.clone();
    let (rid, cur) = s.peek(|b| {
        let t = b.table(&l.table).unwrap();
        let rec = &t.records[ri];
        (rec.id.clone(), rec.cells.get(&fid).and_then(|v| v.as_number()).unwrap_or(0.0) as u8)
    });
    let v = if cur == n { None } else { Some(Value::Number(n as f64)) };
    s.set_cells(&l.table, &[(rid, fid, v)]);
}

/// Values that make a new record show in the current view: its groups'
/// values and the filter's simple conditions.
fn view_presets() -> Vec<(Id, Option<Value>)> {
    let s = st();
    s.peek(|b| {
        let t = b.table(&s.cur.get_untracked())?;
        let v = t.view(&s.view.get_untracked())?;
        if v.filter.any {
            return Some(vec![]);
        }
        let mut out = vec![];
        for c in &v.filter.conditions {
            let Some(f) = t.field(&c.field) else { continue };
            let val = match (&c.op, &c.value, &f.kind) {
                (Op::Is | Op::Contains, FilterValue::Ids(ids), FieldKind::SingleSelect { .. }) if !ids.is_empty() => {
                    Some(Value::Options(vec![ids[0].clone()]))
                }
                (Op::Is | Op::Contains, FilterValue::Ids(ids), FieldKind::MultiSelect { .. }) if !ids.is_empty() => Some(Value::Options(ids.clone())),
                (Op::Is | Op::Contains, FilterValue::Ids(ids), FieldKind::User { .. }) if !ids.is_empty() => Some(Value::Users(ids.clone())),
                (Op::Is, FilterValue::Bool(bv), FieldKind::Checkbox) => Some(Value::Bool(*bv)),
                (Op::Is | Op::Contains, FilterValue::Text(tx), FieldKind::Text) if !tx.is_empty() => Some(Value::Text(tx.clone())),
                (Op::Is, FilterValue::Number(n), k) if k.is_numeric() => Some(Value::Number(*n)),
                _ => None,
            };
            if let Some(val) = val {
                out.push((c.field.clone(), Some(val)));
            }
        }
        Some(out)
    })
    .unwrap_or_default()
}

/// Adds a record with `preset` values, after `after` in the view's order
/// (at the end without it), and selects it.
pub fn add_record(g: Grid, preset: Vec<(Id, Option<Value>)>, after: Option<(Id, bool)>) -> Option<Id> {
    let s = st();
    let mut preset = preset;
    for p in view_presets() {
        if !preset.iter().any(|(f, _)| *f == p.0) {
            preset.push(p);
        }
    }
    let table = s.cur.get_untracked();
    let vid = s.view.get_untracked();
    let id = s.edit(|b| {
        let me = b.me.clone();
        let t = b.table_mut(&table)?;
        let mut r = t.new_record(&me);
        for (f, v) in &preset {
            if let Some(v) = v {
                // Dates of a group are days.
                r.cells.insert(f.clone(), v.clone());
            }
        }
        let id = r.id.clone();
        let pos = after.as_ref().and_then(|(a, _)| t.records.iter().position(|x| x.id == *a));
        match (pos, &after) {
            (Some(p), Some((_, below))) => t.records.insert(if *below { p + 1 } else { p }, std::sync::Arc::new(r)),
            _ => t.push_record(r),
        }
        // Every view with a manual order keeps it there.
        let all: Vec<Id> = t.records.iter().map(|x| x.id.clone()).collect();
        for v in &mut t.views {
            if v.row_order.is_empty() && v.id != vid {
                continue;
            }
            let mut order: Vec<Id> = v.row_order.iter().filter(|x| all.contains(x) && **x != id).cloned().collect();
            for x in &all {
                if !order.contains(x) && *x != id {
                    order.push(x.clone());
                }
            }
            let at = match &after {
                Some((a, below)) => order.iter().position(|x| x == a).map(|p| if *below { p + 1 } else { p }).unwrap_or(order.len()),
                None => order.len(),
            };
            order.insert(at, id.clone());
            v.row_order = order;
        }
        crate::ui::automations::on_add(b, &table, &id);
        Some(id)
    })?;
    g.select_record(&id, 0);
    Some(id)
}

/// Copies the selection's values down to `to_row` (the fill handle).
pub fn fill(g: Grid, to_row: usize) {
    let Some(sel) = g.sel.get_untracked() else { return };
    let (r0, r1) = sel.rows();
    if to_row <= r1 {
        return;
    }
    let (c0, c1) = sel.cols();
    let l = g.layout.get_untracked();
    let s = st();
    let src: Vec<usize> = (r0..=r1).filter_map(|r| l.record_at(r)).collect();
    let dst: Vec<usize> = (r1 + 1..=to_row).filter_map(|r| l.record_at(r)).collect();
    if src.is_empty() {
        return;
    }
    let cells = s.peek(|b| {
        let t = b.table(&l.table).unwrap();
        let mut out = vec![];
        for (k, di) in dst.iter().enumerate() {
            let si = src[k % src.len()];
            for c in c0..=c1 {
                let fid = &l.cols[c].field;
                if l.cols[c].kind.is_computed() {
                    continue;
                }
                let mut v = t.records[si].cells.get(fid).cloned();
                // A single number filled down counts up (1, 2, 3...).
                if src.len() == 1 {
                    if let (Some(Value::Number(n)), FieldKind::Number { .. }) = (&v, &l.cols[c].kind) {
                        let _ = n;
                    }
                } else if src.len() >= 2 && l.cols[c].kind.is_numeric() {
                    let a = t.records[src[0]].cells.get(fid).and_then(|v| v.as_number());
                    let bb = t.records[src[1]].cells.get(fid).and_then(|v| v.as_number());
                    let last = t.records[*src.last().unwrap()].cells.get(fid).and_then(|v| v.as_number());
                    if let (Some(a), Some(bb), Some(last)) = (a, bb, last) {
                        v = Some(Value::Number(last + (bb - a) * (k + 1) as f64));
                    }
                }
                out.push((t.records[*di].id.clone(), fid.clone(), v));
            }
        }
        out
    });
    s.set_cells(&l.table, &cells);
    g.sel.set(Some(Sel { ar: sel.ar, ac: sel.ac, fr: to_row, fc: sel.fc, cols: false }));
}

pub fn path_from_url(u: &str) -> Option<String> {
    let p = u.strip_prefix("file:///").or_else(|| u.strip_prefix("file://"))?;
    let decoded = percent_decode(p);
    Some(decoded.replace('/', "\\"))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Read explicitly selected files and upload bytes directly; no local attachment copy.
pub fn attach(table:&str,record:&str,field:&str,paths:Vec<String>) {
    let s=st();let Some(config)=s.server.with(|v|v.clone()) else {s.toast("请先连接服务器");return};
    if !s.loaded.get_untracked() {s.toast("请先读取服务器文档");return}
    let (table,record,field)=(table.to_string(),record.to_string(),field.to_string());
    let (tx,rx)=async_channel::bounded(1);s.uploads.update(|v|*v+=1);s.toast("正在上传附件…");
    std::thread::spawn(move ||{
        let mut files=vec![];let mut errors=vec![];
        for path in paths {match config.upload(std::path::Path::new(&path)) {Ok(file)=>files.push(file),Err(e)=>errors.push(e.to_string())}}
        let _=tx.send_blocking((files,errors));
    });
    spawn(async move {if let Ok((files,errors))=rx.recv().await {
        let count=files.len();
        if !files.is_empty() {
            let mut current=s.peek(|b|match b.table(&table).and_then(|t|t.record(&record)).and_then(|r|r.cells.get(&field)) {Some(Value::Files(files))=>files.clone(),_=>vec![]});
            current.extend(files);s.set_cells(&table,&[(record,field,Some(Value::Files(current)))]);
        }
        if let Some(error)=errors.first() {s.upload_failed(error);s.toast(error)}else{s.toast(&format!("已上传 {count} 个附件到服务器"))}
        s.upload_finished();
    }else{s.upload_failed("附件上传线程中断");s.upload_finished();s.toast("附件上传线程中断");}});
}

pub fn pick_files(g: Grid, record: Id, field: Id) {
    let table = st().cur.get_untracked();
    let _ = g;
    spawn(async move {
        let paths = widgets::open_files_dialog().await;
        if !paths.is_empty() {
            attach(&table, &record, &field, paths);
        }
    });
}

/// Files dropped on the grid go to the attachment cell under them (or the
/// active one).
pub fn drop_files(g: Grid, paths: Vec<String>) {
    let s = st();
    let Some((rid, fid, _, c)) = g.active() else {
        s.toast("请先选中一个附件单元格");
        return;
    };
    let l = g.layout.get_untracked();
    if !matches!(l.cols[c].kind, FieldKind::Attachment) {
        s.toast("请先选中一个附件单元格");
        return;
    }
    attach(&l.table, &rid, &fid, paths);
}

// ---------------------------------------------------------------------------
// The pickers.

/// The options of a list editor, as (id, label, color) with the query applied.
fn select_items(g: Grid) -> Vec<(Option<Id>, String, u8)> {
    let Some(e) = g.editing.get() else { return vec![] };
    let q = g.list_query.get();
    let s = st();
    s.read(|b| {
        let t = b.table(&s.cur.get_untracked())?;
        let f = t.field(&e.field)?;
        let opts = f.kind.options()?;
        let ql = q.trim().to_lowercase();
        let mut items: Vec<(Option<Id>, String, u8)> = opts
            .iter()
            .filter(|o| ql.is_empty() || o.name.to_lowercase().contains(&ql))
            .map(|o| (Some(o.id.clone()), o.name.clone(), o.color))
            .collect();
        if !q.trim().is_empty() && !opts.iter().any(|o| o.name == q.trim()) {
            items.push((None, q.trim().to_string(), (opts.len() % value::TAG_COLORS.len()) as u8));
        }
        Some(items)
    })
    .unwrap_or_default()
}

fn current_ids(g: Grid) -> Vec<Id> {
    let Some(e) = g.editing.get() else { return vec![] };
    let s = st();
    s.read(|b| {
        b.table(&s.cur.get_untracked())
            .and_then(|t| t.record(&e.record))
            .and_then(|r| r.cells.get(&e.field))
            .map(|v| v.ids().to_vec())
            .unwrap_or_default()
    })
}

/// Picks item `i` of the open list editor.
pub fn choose(g: Grid, i: usize) {
    let Some(e) = g.editing.get_untracked() else { return };
    let s = st();
    let table = s.cur.get_untracked();
    match e.mode {
        EditMode::Select => {
            let items = untracked(|| select_items(g));
            let Some((id, name, color)) = items.get(i).cloned() else { return };
            let multiple = s.peek(|b| b.table(&table).and_then(|t| t.field(&e.field)).is_some_and(|f| matches!(f.kind, FieldKind::MultiSelect { .. })));
            let id = match id {
                Some(id) => id,
                None => {
                    // A new option.
                    let id = new_id("opt");
                    let fid = e.field.clone();
                    let oid = id.clone();
                    s.edit(|b| {
                        if let Some(opts) = b.table_mut(&table).and_then(|t| t.field_mut(&fid)).and_then(|f| f.kind.options_mut()) {
                            opts.push(SelectOption { id: oid, name, color });
                        }
                    });
                    id
                }
            };
            let mut cur = untracked(|| current_ids(g));
            if multiple {
                if let Some(p) = cur.iter().position(|x| *x == id) {
                    cur.remove(p);
                } else {
                    cur.push(id);
                }
                let v = if cur.is_empty() { None } else { Some(Value::Options(cur)) };
                s.set_cells(&table, &[(e.record.clone(), e.field.clone(), v)]);
                g.list_query.set(String::new());
            } else {
                let v = if cur.first() == Some(&id) { None } else { Some(Value::Options(vec![id])) };
                s.set_cells(&table, &[(e.record.clone(), e.field.clone(), v)]);
                end(g);
            }
        }
        EditMode::User => {
            let items = untracked(|| user_items(g));
            let Some((id, _, _)) = items.get(i).cloned() else { return };
            let multiple = s.peek(|b| b.table(&table).and_then(|t| t.field(&e.field)).is_some_and(|f| matches!(f.kind, FieldKind::User { multiple: true })));
            let mut cur = untracked(|| current_ids(g));
            if let Some(p) = cur.iter().position(|x| *x == id) {
                cur.remove(p);
            } else if multiple {
                cur.push(id);
            } else {
                cur = vec![id];
            }
            let v = if cur.is_empty() { None } else { Some(Value::Users(cur)) };
            s.set_cells(&table, &[(e.record.clone(), e.field.clone(), v)]);
            if !multiple {
                end(g);
            }
        }
        EditMode::Link => {
            let items = untracked(|| link_items(g));
            let Some((id, _)) = items.get(i).cloned() else { return };
            let multiple = s.peek(|b| b.table(&table).and_then(|t| t.field(&e.field)).is_some_and(|f| matches!(f.kind, FieldKind::Link { multiple: true, .. })));
            let mut cur = untracked(|| current_ids(g));
            if let Some(p) = cur.iter().position(|x| *x == id) {
                cur.remove(p);
            } else if multiple {
                cur.push(id);
            } else {
                cur = vec![id];
            }
            let v = if cur.is_empty() { None } else { Some(Value::Links(cur)) };
            s.set_cells(&table, &[(e.record.clone(), e.field.clone(), v)]);
            if !multiple {
                end(g);
            }
        }
        _ => {}
    }
}

fn user_items(g: Grid) -> Vec<(Id, String, u8)> {
    let q = g.list_query.get().to_lowercase();
    st().read(|b| {
        b.members
            .iter()
            .filter(|m| q.is_empty() || m.name.to_lowercase().contains(&q))
            .map(|m| (m.id.clone(), m.name.clone(), m.color))
            .collect()
    })
}

fn link_items(g: Grid) -> Vec<(Id, String)> {
    let Some(e) = g.editing.get() else { return vec![] };
    let q = g.list_query.get().to_lowercase();
    let s = st();
    s.read(|b| {
        let t = b.table(&s.cur.get_untracked())?;
        let FieldKind::Link { table: other, .. } = &t.field(&e.field)?.kind else { return None };
        let ot = b.table(other)?;
        Some(
            ot.records
                .iter()
                .map(|r| (r.id.clone(), value::record_title(b, ot, &r.id)))
                .filter(|(_, title)| q.is_empty() || title.to_lowercase().contains(&q))
                .collect(),
        )
    })
    .unwrap_or_default()
}

pub fn list_len(g: Grid) -> usize {
    match g.editing.get_untracked().map(|e| e.mode) {
        Some(EditMode::Select) => untracked(|| select_items(g)).len(),
        Some(EditMode::User) => untracked(|| user_items(g)).len(),
        Some(EditMode::Link) => untracked(|| link_items(g)).len(),
        _ => 0,
    }
}

/// Keys while a list editor is open: true when used.
pub fn list_key(g: Grid, k: crate::win::Key) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    let Some(e) = g.editing.get_untracked() else { return false };
    if e.mode == EditMode::Date {
        if k.vk == VK_RETURN.0 {
            date_commit_text(g);
            return true;
        }
        if k.vk == VK_ESCAPE.0 || k.vk == VK_TAB.0 {
            end(g);
            return true;
        }
        return false;
    }
    let n = list_len(g);
    match k.vk {
        v if v == VK_DOWN.0 => {
            if n > 0 {
                g.list_index.update(|i| *i = (*i + 1) % n);
            }
        }
        v if v == VK_UP.0 => {
            if n > 0 {
                g.list_index.update(|i| *i = (*i + n - 1) % n);
            }
        }
        v if v == VK_RETURN.0 => choose(g, g.list_index.get_untracked()),
        v if v == VK_ESCAPE.0 || v == VK_TAB.0 => end(g),
        _ => return false,
    }
    true
}

fn date_commit_text(g: Grid) {
    let Some(e) = g.editing.get_untracked() else { return };
    let q = g.list_query.get_untracked();
    let s = st();
    if q.trim().is_empty() {
        end(g);
        return;
    }
    match value::parse_date(&q) {
        Some(ms) => {
            s.set_cells(&s.cur.get_untracked(), &[(e.record, e.field, Some(Value::Date(ms)))]);
            end(g);
        }
        None => s.toast("日期格式不正确，例如 2024/01/30"),
    }
}

/// The popup of the cell being edited with a list (or a calendar).
pub fn list_editor(g: Grid) -> Option<AnyElement> {
    let e = g.editing.get()?;
    if e.mode == EditMode::Text || e.mode == EditMode::Attachment {
        return None;
    }
    let l = g.layout.get_untracked();
    if e.row >= l.rows.len() || e.col >= l.cols.len() {
        return None;
    }
    let anchor = move || {
        let l = g.layout.get();
        let (ox, oy) = g.origin.get();
        let _ = (g.sx.get(), g.sy.get());
        let (x, y, w, h) = if e.row < l.rows.len() && e.col < l.cols.len() { g.cell_rect(&l, e.row, e.col) } else { (0.0, 0.0, 1.0, 1.0) };
        bounds(point(px(ox + x), px(oy + y)), size(px(w), px(h)))
    };
    let width = l.cols[e.col].w.max(if e.mode == EditMode::Link { 360.0 } else { 260.0 });
    let body: AnyElement = match e.mode {
        EditMode::Select => select_body(g).into_element(),
        EditMode::User => user_body(g).into_element(),
        EditMode::Link => link_body(g).into_element(),
        EditMode::Date => date_body(g, e.clone()).into_element(),
        _ => return None,
    };
    Some(
        overlay()
            .anchor_bounds(anchor)
            .placement(Placement::Bottom)
            .align(Align::Start)
            .offset(px(4.0))
            .margin(px(8.0))
            .on_press_outside(move |_: &Point<Points>| end(g))
            .on_cancel(move || end(g))
            .w(px(width))
            .bg(rgb(BG))
            .rounded(px(8.0))
            .border_1()
            .border_color(rgb(BORDER))
            .shadow_lg()
            .child(body)
            .into_element(),
    )
}

fn search_box(g: Grid, placeholder: &'static str) -> impl IntoElement {
    view()
        .p(px(8.0))
        .border_b(px(1.0))
        .border_color(rgb(LINE))
        .child(
            nsh_core::element::input()
                .h(px(32.0))
                .px(px(10.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(rgb(BORDER))
                .text_size(px(14.0))
                .text_color(rgb(TEXT))
                .placeholder(placeholder)
                .placeholder_color(rgb(TEXT3))
                .auto_focus(true)
                .value(move || g.list_query.get())
                .on_change(move |t: &str| {
                    g.list_query.set(t.to_string());
                    g.list_index.set(0);
                }),
        )
}

fn select_body(g: Grid) -> impl IntoElement {
    let multiple = move || {
        let Some(e) = g.editing.get() else { return false };
        let s = st();
        s.read(|b| b.table(&s.cur.get_untracked()).and_then(|t| t.field(&e.field)).is_some_and(|f| matches!(f.kind, FieldKind::MultiSelect { .. })))
    };
    view()
        .child(search_box(g, "查找或创建选项"))
        .child(
            scroll().max_h(px(280.0)).py(px(4.0)).child(dynamic(move || {
                let items = select_items(g);
                let cur = current_ids(g);
                let idx = g.list_index.get();
                let multi = multiple();
                if items.is_empty() {
                    return vec![view().px(px(12.0)).py(px(10.0)).child(text("暂无选项，输入内容以创建").text_size(px(13.0)).text_color(rgb(TEXT3))).into_element()];
                }
                items
                    .into_iter()
                    .enumerate()
                    .map(|(i, (id, name, color))| {
                        let selected = id.as_ref().is_some_and(|id| cur.contains(id));
                        let (bg, fg) = tag_color(color);
                        view()
                            .h(px(34.0))
                            .mx(px(4.0))
                            .px(px(8.0))
                            .rounded(px(6.0))
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .bg(if i == idx { rgb(HOVER) } else { rgb(BG) })
                            .hover(|s| s.bg(rgb(HOVER)))
                            .on_press(move || choose(g, i))
                            .child(if multi {
                                widgets::check_box(selected).into_element()
                            } else {
                                view().w(px(0.0)).into_element()
                            })
                            .child(if id.is_none() {
                                text("创建").text_size(px(13.0)).text_color(rgb(TEXT2)).into_element()
                            } else {
                                view().w(px(0.0)).into_element()
                            })
                            .child(tag_el(&name, bg, fg))
                            .child(view().flex_1())
                            .child(if selected && !multi { icons::icon("check", 16.0, PRIMARY).into_element() } else { view().into_element() })
                            .into_element()
                    })
                    .collect::<Vec<_>>()
            })),
        )
}

fn user_body(g: Grid) -> impl IntoElement {
    view().child(search_box(g, "搜索成员")).child(scroll().max_h(px(280.0)).py(px(4.0)).child(dynamic(move || {
        let cur = current_ids(g);
        let idx = g.list_index.get();
        user_items(g)
            .into_iter()
            .enumerate()
            .map(|(i, (id, name, color))| {
                let selected = cur.contains(&id);
                view()
                    .h(px(38.0))
                    .mx(px(4.0))
                    .px(px(8.0))
                    .rounded(px(6.0))
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .bg(if i == idx { rgb(HOVER) } else { rgb(BG) })
                    .hover(|s| s.bg(rgb(HOVER)))
                    .on_press(move || choose(g, i))
                    .child(avatar_el(&name, color, 24.0))
                    .child(text(name).text_size(px(14.0)).text_color(rgb(TEXT)).flex_1())
                    .child(if selected { icons::icon("check", 16.0, PRIMARY).into_element() } else { view().into_element() })
                    .into_element()
            })
            .collect::<Vec<_>>()
    })))
}

fn link_body(g: Grid) -> impl IntoElement {
    let table_name = move || {
        let Some(e) = g.editing.get() else { return String::new() };
        let s = st();
        s.read(|b| {
            let t = b.table(&s.cur.get_untracked())?;
            let FieldKind::Link { table, .. } = &t.field(&e.field)?.kind else { return None };
            b.table(table).map(|t| t.name.clone())
        })
        .unwrap_or_default()
    };
    view()
        .child(
            view()
                .px(px(12.0))
                .pt(px(10.0))
                .child(text(move || format!("关联「{}」中的记录", table_name())).text_size(px(13.0)).text_color(rgb(TEXT2))),
        )
        .child(search_box(g, "搜索记录"))
        .child(scroll().max_h(px(300.0)).py(px(4.0)).child(dynamic(move || {
            let cur = current_ids(g);
            let idx = g.list_index.get();
            let items = link_items(g);
            if items.is_empty() {
                return vec![view().px(px(12.0)).py(px(10.0)).child(text("没有可关联的记录").text_size(px(13.0)).text_color(rgb(TEXT3))).into_element()];
            }
            items
                .into_iter()
                .enumerate()
                .map(|(i, (id, title))| {
                    let selected = cur.contains(&id);
                    view()
                        .h(px(36.0))
                        .mx(px(4.0))
                        .px(px(8.0))
                        .rounded(px(6.0))
                        .flex_row()
                        .items_center()
                        .gap(px(8.0))
                        .bg(if i == idx { rgb(HOVER) } else { rgb(BG) })
                        .hover(|s| s.bg(rgb(HOVER)))
                        .on_press(move || choose(g, i))
                        .child(widgets::check_box(selected))
                        .child(text(title).text_size(px(14.0)).text_color(rgb(TEXT)).number_of_lines(1u32).flex_1())
                        .into_element()
                })
                .collect::<Vec<_>>()
        })))
}

/// The calendar of a date cell: a month of days, today, clear.
fn date_body(g: Grid, e: Editing) -> impl IntoElement {
    let s = st();
    let cur_ms = s.peek(|b| match b.table(&s.cur.get_untracked()).and_then(|t| t.record(&e.record)).and_then(|r| r.cells.get(&e.field)) {
        Some(Value::Date(d)) => Some(*d),
        _ => None,
    });
    let with_time = s.peek(|b| {
        b.table(&s.cur.get_untracked()).and_then(|t| t.field(&e.field)).is_some_and(|f| matches!(f.kind, FieldKind::Date { time: true, .. }))
    });
    let current_text = cur_ms.map(|ms| value::format_date(ms, crate::model::DateFormat::YmdSlash, with_time));
    let record = e.record.clone();
    let field = e.field.clone();
    let set = move |ms: Option<i64>| {
        let s = st();
        s.set_cells(&s.cur.get_untracked(), &[(record.clone(), field.clone(), ms.map(Value::Date))]);
        end(g);
    };
    let set2 = set.clone();
    let time_of = move || {
        if !with_time {
            return 0;
        }
        let q = g.list_query.get_untracked();
        value::parse_date(&q).or(cur_ms).map(|ms| ms - value::day_start(ms)).unwrap_or(9 * 3_600_000)
    };
    view()
        .child(
            view().p(px(8.0)).border_b(px(1.0)).border_color(rgb(LINE)).child(
                nsh_core::element::input()
                    .h(px(32.0))
                    .px(px(10.0))
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .text_size(px(14.0))
                    .placeholder(current_text.unwrap_or_else(|| if with_time { "2024/01/30 09:00".into() } else { "2024/01/30".into() }))
                    .placeholder_color(rgb(TEXT3))
                    .auto_focus(true)
                    .value(move || g.list_query.get())
                    .on_change(move |t: &str| g.list_query.set(t.to_string())),
            ),
        )
        .child(widgets::calendar(cur_ms.unwrap_or_else(value::today_ms), cur_ms, move |day| {
            set(Some(day + time_of()));
        }))
        .child(
            view()
                .flex_row()
                .justify_between()
                .px(px(12.0))
                .py(px(8.0))
                .border_t(px(1.0))
                .border_color(rgb(LINE))
                .child(widgets::link_button("今天", {
                    let set = set2.clone();
                    move || set(Some(value::today_ms() + if with_time { crate::model::now_ms() - value::today_ms() } else { 0 }))
                }))
                .child(widgets::link_button("清除", move || set2(None))),
        )
}

pub fn draw_hint() {
    let _ = cells::text_w("", 12.0);
    let _ = Row::Add { level: 0, preset: vec![] };
}
