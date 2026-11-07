//! Automations (自动化): the enabled workflows run when records are added or
//! a watched field changes.

use crate::model::{Action, Base, Id, Trigger, Value, now_ms};

fn run(base: &mut Base, wf_index: usize, record: &str) {
    let wf = base.workflows[wf_index].clone();
    match &wf.action {
        Action::SetField { field, value } => {
            if let Some(t) = base.table_mut(&wf.table) {
                if let Some(r) = t.record_mut(record) {
                    r.cells.insert(field.clone(), value.clone());
                    r.updated_at = now_ms();
                }
            }
        }
        Action::OpenUrl { url } => {
            let t = base.table(&wf.table);
            let target = t.and_then(|t| t.record(record).map(|r| fill_template(base, t, r, url))).unwrap_or_default();
            crate::ui::widgets::shell_open(&target);
        }
        Action::Notify { .. } => return,
    }
    base.workflows[wf_index].runs += 1;
}

/// After cells of `table` changed: (record, field) pairs.
pub fn on_change(base: &mut Base, table: &str, changed: &[(Id, Id)]) {
    for i in 0..base.workflows.len() {
        let wf = &base.workflows[i];
        if !wf.enabled || wf.table != table {
            continue;
        }
        if let Trigger::FieldChanged { field } = &wf.trigger {
            // An action that sets the watched field must not loop.
            if let Action::SetField { field: target, .. } = &wf.action {
                if target == field {
                    continue;
                }
            }
            let hits: Vec<Id> = changed.iter().filter(|(_, f)| f == field).map(|(r, _)| r.clone()).collect();
            for r in hits {
                run(base, i, &r);
            }
        }
    }
}

/// After `record` was added to `table`.
pub fn on_add(base: &mut Base, table: &str, record: &str) {
    for i in 0..base.workflows.len() {
        let wf = &base.workflows[i];
        if wf.enabled && wf.table == table && wf.trigger == Trigger::RecordAdded {
            run(base, i, record);
        }
    }
}

/// `{字段名}` in `s` replaced by the record's values.
pub fn fill_template(base: &Base, t: &crate::model::Table, r: &crate::model::Record, s: &str) -> String {
    let mut out = s.to_string();
    for f in &t.fields {
        let key = format!("{{{}}}", f.name);
        if out.contains(&key) {
            let v = crate::model::query::cell(base, t, r, f).map(|v| crate::model::value::display(base, t, &f.kind, &v)).unwrap_or_default();
            out = out.replace(&key, &v);
        }
    }
    out
}

/// A button field was pressed on `record`.
pub fn press_button(table: &str, field: &str, record: &str) {
    let st = crate::ui::state::st();
    let Some(action) = st.peek(|b| match b.table(table).and_then(|t| t.field(field)).map(|f| f.kind.clone()) {
        Some(crate::model::FieldKind::Button { action, .. }) => Some(action),
        _ => None,
    }) else {
        return;
    };
    match action {
        Action::SetField { field: target, value } => {
            st.set_cells(table, &[(record.to_string(), target, Some(value))]);
            st.toast("已执行按钮操作");
        }
        Action::OpenUrl { url } if !url.trim().is_empty() => {
            let target = st.peek(|b| b.table(table).and_then(|t| t.record(record).map(|r| fill_template(b, t, r, &url)))).unwrap_or_default();
            crate::ui::widgets::shell_open(&target);
        }
        Action::OpenUrl { .. } | Action::Notify { .. } => st.toast("这个按钮还没有设置点击后的操作"),
    }
}

pub fn describe(base: &Base, wf: &crate::model::Workflow) -> String {
    let t = base.table(&wf.table);
    let fname = |id: &str| t.and_then(|t| t.field(id)).map(|f| f.name.clone()).unwrap_or("?".into());
    let trig = match &wf.trigger {
        Trigger::RecordAdded => "新增记录时".to_string(),
        Trigger::FieldChanged { field } => format!("「{}」修改时", fname(field)),
    };
    let act = match &wf.action {
        Action::SetField { field, value } => {
            let shown = t
                .and_then(|t| t.field(field).map(|f| crate::model::value::display(base, t, &f.kind, value)))
                .unwrap_or_default();
            format!("将「{}」设为 {}", fname(field), if shown.is_empty() { "空".into() } else { shown })
        }
        Action::Notify { .. } => "无操作（通知功能已移除）".to_string(),
        Action::OpenUrl { url } => format!("打开链接：{url}"),
    };
    let _ = Value::Bool(true);
    format!("{trig} → {act}")
}
