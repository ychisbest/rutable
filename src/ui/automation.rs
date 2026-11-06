//! Answers of the automation port: the app's state as JSON, for a driver
//! that checks what a user's actions did.

use serde_json::{Value as J, json};

use super::{grid, state::st};
use crate::{
    model::{query, value},
    win,
};

thread_local! {
    static LOG: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Notes an event for the driver (the last 30 show in `state`).
pub fn log(msg: impl Into<String>) {
    LOG.with(|l| {
        let mut l = l.borrow_mut();
        l.push(msg.into());
        if l.len() > 30 {
            l.remove(0);
        }
    });
}

pub fn handle(req: &str) -> String {
    let s = st();
    let (cmd, arg) = req.split_once(' ').unwrap_or((req, ""));
    let out: J = match cmd {
        "state" => state(),
        "grid" => grid_geometry(),
        "table" => table_dump(arg.parse().unwrap_or(50)),
        "record" => record_dump(arg),
        "doc" => s.peek(|b| serde_json::to_value(b).unwrap_or(J::Null)),
        "undo" => {
            s.undo();
            json!({"ok": true})
        }
        "save" => {
            s.save_now();
            json!({"ok": true, "path": super::state::data_path().to_string_lossy()})
        }
        _ => json!({"error": format!("unknown request {cmd}")}),
    };
    out.to_string()
}

fn state() -> J {
    let s = st();
    let g = grid::grid();
    let (table, view) = (s.cur.get_untracked(), s.view.get_untracked());
    let names = s.peek(|b| {
        let t = b.table(&table);
        json!({
            "base": b.name,
            "item": t.map(|t| t.name.clone()).or_else(|| b.dashboard(&table).map(|d| d.name.clone())),
            "view": t.and_then(|t| t.view(&view)).map(|v| v.name.clone()),
            "view_kind": t.and_then(|t| t.view(&view)).map(|v| format!("{:?}", v.kind)),
            "tables": b.tables.iter().map(|t| t.name.clone()).collect::<Vec<_>>(),
            "views": t.map(|t| t.views.iter().map(|v| v.name.clone()).collect::<Vec<_>>()),
            "dashboards": b.dashboards.iter().map(|d| d.name.clone()).collect::<Vec<_>>(),
        })
    });
    let grid_state = g.map(|g| {
        let l = g.layout.get_untracked();
        json!({
            "sel": g.sel.get_untracked().map(|s| json!([s.ar, s.ac, s.fr, s.fc, s.cols])),
            "editing": g.editing.get_untracked().map(|e| json!({"row": e.row, "col": e.col, "mode": format!("{:?}", e.mode), "text": g.edit_text.get_untracked()})),
            "list_query": g.list_query.get_untracked(),
            "rows": l.rows.len(),
            "records": l.records.len(),
            "columns": l.cols.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
            "scroll": [g.sx.get_untracked(), g.sy.get_untracked()],
            "focused": g.focused.get_untracked(),
        })
    });
    json!({
        "doc": names,
        "popup": s.popup.get_untracked().map(|p| format!("{p:?}")),
        "expanded": s.expanded.get_untracked(),
        "toast": s.toast.get_untracked().map(|t| t.0),
        "checked": s.checked.get_untracked().len(),
        "search": s.search.get_untracked(),
        "key_mode": win::key_mode(),
        "grid": grid_state,
        "can_undo": s.can_undo(),
        "log": LOG.with(|l| l.borrow().clone()),
    })
}

/// Where the grid's cells are, in window points: for a driver to click.
fn grid_geometry() -> J {
    let Some(g) = grid::grid() else { return json!({"error": "no grid"}) };
    let l = g.layout.get_untracked();
    let (ox, oy) = g.origin.get_untracked();
    let (w, h) = g.size.get_untracked();
    let cols: Vec<J> = (0..l.cols.len())
        .map(|c| json!({"name": l.cols[c].name, "x": ox + g.col_x(&l, c), "w": l.cols[c].w}))
        .collect();
    let rows: Vec<J> = (0..l.rows.len())
        .map(|r| {
            let kind = match &l.rows[r] {
                query::Row::Record(_) => "record",
                query::Row::Group { .. } => "group",
                query::Row::Add { .. } => "add",
            };
            json!({"kind": kind, "y": oy + g.row_y(&l, r), "h": l.heights[r]})
        })
        .collect();
    json!({"origin": [ox, oy], "size": [w, h], "head": grid::HEAD, "rh": grid::RH, "cols": cols, "rows": rows})
}

/// The current view's rows as the grid shows them.
fn table_dump(limit: usize) -> J {
    let s = st();
    let Some(g) = grid::grid() else {
        return s
            .with_view(|b, t, v| {
                let idx = query::records(b, t, v);
                json!({"records": idx.iter().take(limit).map(|&i| value::record_title(b, t, &t.records[i].id)).collect::<Vec<_>>()})
            })
            .unwrap_or(J::Null);
    };
    let l = g.layout.get_untracked();
    s.peek(|b| {
        let Some(t) = b.table(&l.table) else { return J::Null };
        let rows: Vec<J> = l
            .rows
            .iter()
            .take(limit)
            .map(|r| match r {
                query::Row::Record(i) => {
                    let rec = &t.records[*i];
                    J::Array(
                        l.cols
                            .iter()
                            .map(|c| {
                                let f = t.field(&c.field).unwrap();
                                J::String(query::cell(b, t, rec, f).map(|v| value::display(b, t, &f.kind, &v)).unwrap_or_default())
                            })
                            .collect(),
                    )
                }
                query::Row::Group { value: v, count, field, .. } => {
                    let f = t.field(field);
                    json!({"group": v.as_ref().and_then(|v| f.map(|f| value::display(b, t, &f.kind, v))), "count": count})
                }
                query::Row::Add { .. } => json!("+"),
            })
            .collect();
        json!({"columns": l.cols.iter().map(|c| c.name.clone()).collect::<Vec<_>>(), "rows": rows})
    })
}

fn record_dump(title: &str) -> J {
    let s = st();
    s.with_table(|b, t| {
        let r = t.records.iter().find(|r| value::record_title(b, t, &r.id) == title)?;
        let mut m = serde_json::Map::new();
        for f in &t.fields {
            let v = query::cell(b, t, r, f).map(|v| value::display(b, t, &f.kind, &v)).unwrap_or_default();
            m.insert(f.name.clone(), J::String(v));
        }
        m.insert("_history".into(), json!(r.history.len()));
        m.insert("_comments".into(), json!(r.comments.iter().map(|c| c.text.clone()).collect::<Vec<_>>()));
        Some(J::Object(m))
    })
    .flatten()
    .unwrap_or(J::Null)
}
