//! CSV: exporting a view, importing a file as a new table or into the
//! current one.

use nsh_core::prelude::*;

use super::{state::st, widgets};
use crate::model::{DateFormat, Field, FieldKind, Id, Table, View, ViewKind, new_id, query, value};

fn csv_cell(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

/// The CSV of `table` as its view shows it (the open view, or its first).
pub fn csv_of(table: &str) -> Option<(String, String)> {
    let s = st();
    let cur_view = s.view.get_untracked();
    s.peek(|b| {
        let t = b.table(table)?;
        let v = t.view(&cur_view).or(t.views.first())?;
        let fields: Vec<&Field> = t.view_fields(v).into_iter().filter(|(_, h)| !h).map(|(f, _)| f).collect();
        let mut out = String::from("\u{FEFF}");
        out.push_str(&fields.iter().map(|f| csv_cell(&f.name)).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
        for i in query::records(b, t, v) {
            let r = &t.records[i];
            let row: Vec<String> = fields
                .iter()
                .map(|f| query::cell(b, t, r, f).map(|v| value::display(b, t, &f.kind, &v)).unwrap_or_default())
                .map(|s| csv_cell(&s))
                .collect();
            out.push_str(&row.join(","));
            out.push_str("\r\n");
        }
        Some((format!("{}-{}.csv", t.name, v.name), out))
    })
}

pub fn export_csv(table: Id) {
    let Some((name, data)) = csv_of(&table) else { return };
    spawn(async move {
        if let Some(path) = widgets::save_file_dialog(name, "CSV 文件", "csv").await {
            match std::fs::write(&path, data.as_bytes()) {
                Ok(()) => st().toast("导出成功"),
                Err(e) => st().toast(&format!("导出失败：{e}")),
            }
        }
    });
}

pub fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let text = text.trim_start_matches('\u{FEFF}');
    let mut rows = vec![];
    let mut row = vec![];
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cell.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                cell.push(c);
            }
            continue;
        }
        match c {
            '"' if cell.is_empty() => quoted = true,
            ',' => row.push(std::mem::take(&mut cell)),
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            _ => cell.push(c),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows.retain(|r| r.iter().any(|c| !c.trim().is_empty()));
    rows
}

/// A type for a column of values: numbers, dates, or text.
fn infer(values: &[&str]) -> FieldKind {
    let filled: Vec<&str> = values.iter().copied().filter(|v| !v.trim().is_empty()).collect();
    if filled.is_empty() {
        return FieldKind::Text;
    }
    if filled.iter().all(|v| v.trim().replace(',', "").parse::<f64>().is_ok()) {
        let decimals = filled.iter().map(|v| v.split('.').nth(1).map(|d| d.len()).unwrap_or(0)).max().unwrap_or(0);
        return FieldKind::Number { precision: decimals.min(4) as u8, thousands: false, percent: false };
    }
    if filled.iter().all(|v| v.len() >= 6 && value::parse_date(v).is_some()) {
        return FieldKind::Date { format: DateFormat::YmdSlash, time: filled.iter().any(|v| v.contains(':')) };
    }
    if matches!(filled.iter().map(|v| v.to_lowercase()).collect::<std::collections::HashSet<_>>().len(), 1..=8) && filled.len() >= 6 {
        return FieldKind::SingleSelect { options: vec![] };
    }
    FieldKind::Text
}

fn read_file(path: &str) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

pub fn import_csv_new() {
    spawn(async move {
        let Some(path) = widgets::open_one_dialog("CSV 文件", "*.csv").await else { return };
        let Some(text) = read_file(&path) else { return };
        let rows = parse_csv(&text);
        if rows.is_empty() {
            st().toast("文件为空");
            return;
        }
        let name = std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or("导入数据".into());
        let id = import_rows(&name, rows);
        st().open(&id);
        st().toast("导入成功");
    });
}

/// Makes a table of CSV rows (the first is the header).
pub fn import_rows(name: &str, rows: Vec<Vec<String>>) -> Id {
    let s = st();
    s.edit(|b| {
        let header = rows[0].clone();
        let width = rows.iter().map(Vec::len).max().unwrap_or(0).max(1);
        let mut fields = vec![];
        for c in 0..width {
            let col: Vec<&str> = rows[1..].iter().map(|r| r.get(c).map(String::as_str).unwrap_or("")).collect();
            let fname = header.get(c).cloned().filter(|h| !h.trim().is_empty()).unwrap_or(format!("字段 {}", c + 1));
            let kind = if c == 0 { FieldKind::Text } else { infer(&col) };
            fields.push(Field { id: new_id("fld"), name: fname, kind, description: String::new() });
        }
        let mut t = Table {
            id: new_id("tbl"),
            name: b.unique_table_name(name),
            fields,
            records: vec![],
            views: vec![View::new("表格", ViewKind::Grid)],
            seq: 0,
            option_sources: std::collections::HashMap::new(),
        };
        let me = b.me.clone();
        let snapshot = b.clone();
        for row in &rows[1..] {
            let mut r = t.new_record(&me);
            for (c, txt) in row.iter().enumerate() {
                let Some(f) = t.fields.get_mut(c) else { continue };
                if let Some(v) = value::parse(&snapshot, &mut f.kind, txt) {
                    r.cells.insert(f.id.clone(), v);
                }
            }
            t.push_record(r);
        }
        let id = t.id.clone();
        b.tables.push(t);
        b.order.push(id.clone());
        id
    })
}

pub fn import_csv_append() {
    spawn(async move {
        let Some(path) = widgets::open_one_dialog("CSV 文件", "*.csv").await else { return };
        let Some(text) = read_file(&path) else { return };
        let rows = parse_csv(&text);
        if rows.len() < 2 {
            st().toast("没有可导入的记录");
            return;
        }
        let n = append_rows(rows);
        st().toast(&format!("已导入 {n} 条记录"));
    });
}

/// Adds CSV rows to the current table, columns matched by field name
/// (new text fields for the others).
pub fn append_rows(rows: Vec<Vec<String>>) -> usize {
    let s = st();
    let table = s.cur.get_untracked();
    s.edit(|b| {
        let me = b.me.clone();
        let snapshot = b.clone();
        let Some(t) = b.table_mut(&table) else { return 0 };
        let header = rows[0].clone();
        let mut map = vec![];
        for h in &header {
            let id = match t.field_by_name(h.trim()) {
                Some(f) => f.id.clone(),
                None => {
                    let f = Field { id: new_id("fld"), name: t.unique_field_name(h.trim()), kind: FieldKind::Text, description: String::new() };
                    let id = f.id.clone();
                    t.fields.push(f);
                    id
                }
            };
            map.push(id);
        }
        let mut n = 0;
        for row in &rows[1..] {
            let mut r = t.new_record(&me);
            for (c, txt) in row.iter().enumerate() {
                let Some(fid) = map.get(c) else { continue };
                let Some(f) = t.fields.iter_mut().find(|f| &f.id == fid) else { continue };
                if f.kind.is_computed() {
                    continue;
                }
                if let Some(v) = value::parse(&snapshot, &mut f.kind, txt) {
                    r.cells.insert(fid.clone(), v);
                }
            }
            t.push_record(r);
            n += 1;
        }
        n
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn csv() {
        let r = super::parse_csv("\u{FEFF}a,b\r\n\"x,1\",\"q\"\"z\"\n\n3,4\n");
        assert_eq!(r, vec![vec!["a", "b"], vec!["x,1", "q\"z"], vec!["3", "4"]]);
    }
}
