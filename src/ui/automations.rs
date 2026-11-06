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
        Action::Notify { text } => {
            let title = base
                .table(&wf.table)
                .map(|t| crate::model::value::record_title(base, t, record))
                .unwrap_or_default();
            let msg = format!("「{}」{}：{}", wf.name, title, text);
            let st = crate::ui::state::st();
            st.notices.update(|n| n.insert(0, msg.clone()));
            st.toast(&msg);
        }
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
        Action::Notify { text } => format!("发送通知：{text}"),
    };
    let _ = Value::Bool(true);
    format!("{trig} → {act}")
}
