//! Reading the document: a cell's value (computed fields included), which
//! records a view shows and in what order, its groups, and column statistics.

use std::{cell::RefCell, collections::HashMap};

use chrono::Datelike;

use super::{
    Aggregate, Base, Condition, DateRef, Field, FieldKind, FilterValue, Id, Op, Record, SortRule, Stat, Table, View,
    formula,
    value::{self, Value},
};

thread_local! {
    static CACHE: RefCell<HashMap<(Id, Id), Option<Value>>> = RefCell::new(HashMap::new());
    static DEPTH: RefCell<u32> = const { RefCell::new(0) };
}

/// Drops the computed values (after any change to the document).
pub fn invalidate() {
    CACHE.with(|c| c.borrow_mut().clear());
}

/// The value of `field` in `record`, computed fields included.
pub fn cell(base: &Base, table: &Table, record: &Record, field: &Field) -> Option<Value> {
    match &field.kind {
        FieldKind::AutoNumber => Some(Value::Number(record.seq as f64)),
        FieldKind::CreatedTime { .. } => Some(Value::Date(record.created_at)),
        FieldKind::ModifiedTime { .. } => Some(Value::Date(record.updated_at)),
        FieldKind::CreatedBy => Some(Value::Users(vec![record.created_by.clone()])).filter(|_| !record.created_by.is_empty()),
        FieldKind::ModifiedBy => Some(Value::Users(vec![record.updated_by.clone()])).filter(|_| !record.updated_by.is_empty()),
        FieldKind::Formula { .. } | FieldKind::Lookup { .. } => {
            let key = (record.id.clone(), field.id.clone());
            if let Some(v) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
                return v;
            }
            let depth = DEPTH.with(|d| {
                *d.borrow_mut() += 1;
                *d.borrow()
            });
            let v = if depth > 12 {
                Some(Value::Error("#CYCLE!".into()))
            } else {
                match &field.kind {
                    FieldKind::Formula { expr } => formula::evaluate(base, table, record, expr),
                    FieldKind::Lookup { link, target, aggregate } => lookup(base, table, record, link, target, *aggregate),
                    _ => None,
                }
            };
            DEPTH.with(|d| *d.borrow_mut() -= 1);
            CACHE.with(|c| c.borrow_mut().insert(key, v.clone()));
            v
        }
        _ => record.cells.get(&field.id).cloned(),
    }
}

fn lookup(base: &Base, table: &Table, record: &Record, link: &str, target: &str, agg: Aggregate) -> Option<Value> {
    let lf = table.field(link)?;
    let FieldKind::Link { table: other, .. } = &lf.kind else { return None };
    let ot = base.table(other)?;
    let tf = ot.field(target)?;
    let ids = match record.cells.get(link) {
        Some(Value::Links(ids)) => ids.clone(),
        _ => vec![],
    };
    let mut vals: Vec<Value> = ids
        .iter()
        .filter_map(|id| ot.record(id))
        .filter_map(|r| cell(base, ot, r, tf))
        .collect();
    // Lookups of options and people show their names.
    let names = |v: &Value| value::display(base, ot, &tf.kind, v);
    let nums = |vals: &[Value]| -> Vec<f64> { vals.iter().filter_map(|v| v.as_number()).collect() };
    match agg {
        Aggregate::Original | Aggregate::Unique => {
            let mut out: Vec<Value> = vec![];
            for v in vals.drain(..) {
                let shown = match &v {
                    Value::Options(_) | Value::Users(_) | Value::Links(_) => {
                        names(&v).split(',').map(|s| Value::Text(s.to_string())).collect::<Vec<_>>()
                    }
                    Value::List(l) => l.clone(),
                    Value::Bool(_) | Value::Date(_) | Value::Number(_) => {
                        vec![if matches!(v, Value::Number(_)) && !matches!(tf.kind, FieldKind::Number { .. }) {
                            Value::Text(names(&v))
                        } else {
                            v.clone()
                        }]
                    }
                    _ => vec![Value::Text(names(&v))],
                };
                for s in shown {
                    if agg == Aggregate::Unique && out.contains(&s) {
                        continue;
                    }
                    out.push(s);
                }
            }
            if out.is_empty() { None } else { Some(Value::List(out)) }
        }
        Aggregate::Sum => Some(Value::Number(nums(&vals).iter().sum())),
        Aggregate::Average => {
            let n = nums(&vals);
            if n.is_empty() { None } else { Some(Value::Number(n.iter().sum::<f64>() / n.len() as f64)) }
        }
        Aggregate::Max => nums(&vals).into_iter().reduce(f64::max).map(Value::Number),
        Aggregate::Min => nums(&vals).into_iter().reduce(f64::min).map(Value::Number),
        Aggregate::Count => Some(Value::Number(vals.len() as f64)),
        Aggregate::CountUnique => {
            let mut seen: Vec<String> = vec![];
            for v in &vals {
                let s = names(v);
                if !seen.contains(&s) {
                    seen.push(s);
                }
            }
            Some(Value::Number(seen.len() as f64))
        }
    }
}

/// The start and end (excluded) of a relative date, in local days.
pub fn date_range(r: &DateRef) -> (i64, i64) {
    let today = value::today_ms();
    let d = value::DAY_MS;
    let wd = value::local(today).weekday().num_days_from_monday() as i64;
    let month_start = |offset: i32| {
        let x = value::local(today);
        let ym = x.year() * 12 + x.month() as i32 - 1 + offset;
        value::date_ms(ym.div_euclid(12), ym.rem_euclid(12) as u32 + 1, 1)
    };
    match r {
        DateRef::Today => (today, today + d),
        DateRef::Tomorrow => (today + d, today + 2 * d),
        DateRef::Yesterday => (today - d, today),
        DateRef::ThisWeek => (today - wd * d, today + (7 - wd) * d),
        DateRef::LastWeek => (today - (wd + 7) * d, today - wd * d),
        DateRef::ThisMonth => (month_start(0), month_start(1)),
        DateRef::LastMonth => (month_start(-1), month_start(0)),
        DateRef::Past7 => (today - 7 * d, today + d),
        DateRef::Next7 => (today, today + 8 * d),
        DateRef::Past30 => (today - 30 * d, today + d),
        DateRef::Next30 => (today, today + 31 * d),
        DateRef::Exact(ms) => {
            let s = value::day_start(*ms);
            (s, s + d)
        }
    }
}

/// Whether `record` meets `cond`.
pub fn meets(base: &Base, table: &Table, record: &Record, cond: &Condition) -> bool {
    let Some(field) = table.field(&cond.field) else { return true };
    let v = cell(base, table, record, field);
    let empty = v.as_ref().is_none_or(Value::is_empty);
    match cond.op {
        Op::Empty => return empty,
        Op::NotEmpty => return !empty,
        _ => {}
    }
    // An unset value matches everything (a condition being written).
    let unset = match &cond.value {
        FilterValue::None => true,
        FilterValue::Text(s) => s.is_empty(),
        FilterValue::Ids(ids) => ids.is_empty(),
        _ => false,
    };
    if unset && !matches!(field.kind, FieldKind::Checkbox) {
        return true;
    }
    let kind = &field.kind;
    match (&cond.value, kind) {
        (FilterValue::Bool(want), FieldKind::Checkbox) => {
            let has = matches!(v, Some(Value::Bool(true)));
            has == *want
        }
        (FilterValue::None, FieldKind::Checkbox) => true,
        (FilterValue::Date(r), _) => {
            let Some(ms) = v.as_ref().and_then(|v| match v {
                Value::Date(d) => Some(*d),
                Value::List(l) => l.iter().find_map(|x| if let Value::Date(d) = x { Some(*d) } else { None }),
                _ => None,
            }) else {
                return false;
            };
            let (s, e) = date_range(r);
            match cond.op {
                Op::Is => ms >= s && ms < e,
                Op::IsNot => !(ms >= s && ms < e),
                Op::Before | Op::Lt => ms < s,
                Op::After | Op::Gt => ms >= e,
                Op::Lte => ms < e,
                Op::Gte => ms >= s,
                _ => true,
            }
        }
        (FilterValue::Ids(want), _) => {
            let have: Vec<Id> = v.as_ref().map(|v| v.ids().to_vec()).unwrap_or_default();
            let any = want.iter().any(|w| have.contains(w));
            let single = matches!(kind, FieldKind::SingleSelect { .. });
            match cond.op {
                Op::Is => {
                    if single {
                        any
                    } else {
                        let mut a = have.clone();
                        let mut b = want.clone();
                        a.sort();
                        b.sort();
                        a == b
                    }
                }
                Op::IsNot => {
                    if single {
                        !any
                    } else {
                        let mut a = have.clone();
                        let mut b = want.clone();
                        a.sort();
                        b.sort();
                        a != b
                    }
                }
                Op::Contains => any,
                Op::NotContains => !any,
                _ => true,
            }
        }
        (FilterValue::Number(n), _) => {
            let Some(x) = v.as_ref().and_then(|v| v.as_number()) else {
                return matches!(cond.op, Op::IsNot);
            };
            match cond.op {
                Op::Is => (x - n).abs() < 1e-9,
                Op::IsNot => (x - n).abs() >= 1e-9,
                Op::Gt => x > *n,
                Op::Gte => x >= *n,
                Op::Lt => x < *n,
                Op::Lte => x <= *n,
                _ => true,
            }
        }
        (FilterValue::Text(s), _) => {
            let shown = v.as_ref().map(|v| value::display(base, table, kind, v)).unwrap_or_default().to_lowercase();
            let s = s.to_lowercase();
            if kind.is_numeric() {
                if let (Ok(n), Some(x)) = (s.parse::<f64>(), v.as_ref().and_then(|v| v.as_number())) {
                    return match cond.op {
                        Op::Is => (x - n).abs() < 1e-9,
                        Op::IsNot => (x - n).abs() >= 1e-9,
                        Op::Gt => x > n,
                        Op::Gte => x >= n,
                        Op::Lt => x < n,
                        Op::Lte => x <= n,
                        _ => true,
                    };
                }
            }
            match cond.op {
                Op::Is => shown == s,
                Op::IsNot => shown != s,
                Op::Contains => shown.contains(&s),
                Op::NotContains => !shown.contains(&s),
                _ => true,
            }
        }
        _ => true,
    }
}

/// Whether `record` passes the view's filter.
pub fn passes(base: &Base, table: &Table, record: &Record, filter: &super::Filter) -> bool {
    if filter.conditions.is_empty() {
        return true;
    }
    if filter.any {
        filter.conditions.iter().any(|c| meets(base, table, record, c))
    } else {
        filter.conditions.iter().all(|c| meets(base, table, record, c))
    }
}

/// Whether any shown field of `record` holds `text` (the search).
pub fn found(base: &Base, table: &Table, record: &Record, fields: &[&Field], text: &str) -> bool {
    let t = text.to_lowercase();
    fields.iter().any(|f| {
        cell(base, table, record, f)
            .map(|v| value::display(base, table, &f.kind, &v).to_lowercase().contains(&t))
            .unwrap_or(false)
    })
}

/// The view's records in their order: filtered and sorted (by the view's
/// sort, then its manual order).
pub fn records(base: &Base, table: &Table, view: &View) -> Vec<usize> {
    let pos: HashMap<&str, usize> = view.row_order.iter().enumerate().map(|(i, id)| (id.as_str(), i)).collect();
    let mut idx: Vec<usize> = (0..table.records.len())
        .filter(|&i| passes(base, table, &table.records[i], &view.filter))
        .collect();
    // Manual order: records the view has ordered, then the rest as created.
    idx.sort_by_key(|&i| pos.get(table.records[i].id.as_str()).copied().unwrap_or(usize::MAX / 2 + i));
    if !view.sorts.is_empty() {
        sort_by(base, table, &mut idx, &view.sorts);
    }
    idx
}

pub fn sort_by(base: &Base, table: &Table, idx: &mut [usize], rules: &[SortRule]) {
    let fields: Vec<(&Field, bool)> = rules.iter().filter_map(|r| table.field(&r.field).map(|f| (f, r.desc))).collect();
    let keys: Vec<Vec<Option<Value>>> = table
        .records
        .iter()
        .map(|r| fields.iter().map(|(f, _)| cell(base, table, r, f)).collect())
        .collect();
    idx.sort_by(|&a, &b| {
        for (k, (f, desc)) in fields.iter().enumerate() {
            let (x, y) = (keys[a][k].as_ref(), keys[b][k].as_ref());
            // Empty values stay last whichever the direction.
            let xe = x.is_none_or(Value::is_empty);
            let ye = y.is_none_or(Value::is_empty);
            if xe != ye {
                return if xe { std::cmp::Ordering::Greater } else { std::cmp::Ordering::Less };
            }
            let mut o = value::compare(base, table, &f.kind, x, y);
            if *desc {
                o = o.reverse();
            }
            if o != std::cmp::Ordering::Equal {
                return o;
            }
        }
        std::cmp::Ordering::Equal
    });
}

/// A row of a grid: a group's header, a record, or a group's add row.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    Group {
        level: usize,
        key: String,
        field: Id,
        value: Option<Value>,
        count: usize,
        collapsed: bool,
    },
    Record(usize),
    /// The "+" row that adds a record (to a group: its values).
    Add { level: usize, preset: Vec<(Id, Option<Value>)> },
}

/// The key a record groups under for `field` (and the value it shows).
pub fn group_key(base: &Base, table: &Table, record: &Record, field: &Field) -> (String, Option<Value>) {
    let v = cell(base, table, record, field).filter(|v| !v.is_empty());
    let key = match &v {
        None => String::new(),
        Some(Value::Date(ms)) => format!("d{}", value::day_start(*ms)),
        Some(Value::Options(ids)) | Some(Value::Users(ids)) | Some(Value::Links(ids)) => ids.join(","),
        Some(Value::Bool(b)) => b.to_string(),
        Some(x) => value::display(base, table, &field.kind, x),
    };
    let v = match v {
        Some(Value::Date(ms)) => Some(Value::Date(value::day_start(ms))),
        x => x,
    };
    (key, v)
}

/// The grid's rows for `view`: records under their groups, with the "+" rows.
pub fn rows(base: &Base, table: &Table, view: &View, search: &str) -> Vec<Row> {
    let mut idx = records(base, table, view);
    if !search.trim().is_empty() {
        let fields: Vec<&Field> = table.view_fields(view).into_iter().filter(|(_, h)| !h).map(|(f, _)| f).collect();
        idx.retain(|&i| found(base, table, &table.records[i], &fields, search.trim()));
    }
    let groups: Vec<(&Field, bool)> =
        view.groups.iter().filter_map(|g| table.field(&g.field).map(|f| (f, g.desc))).take(3).collect();
    let mut out = vec![];
    if groups.is_empty() {
        out.extend(idx.into_iter().map(Row::Record));
        out.push(Row::Add { level: 0, preset: vec![] });
        return out;
    }
    group_level(base, table, view, &groups, 0, &idx, &mut out, &mut vec![], "");
    out
}

#[allow(clippy::too_many_arguments)]
fn group_level(
    base: &Base,
    table: &Table,
    view: &View,
    groups: &[(&Field, bool)],
    level: usize,
    idx: &[usize],
    out: &mut Vec<Row>,
    preset: &mut Vec<(Id, Option<Value>)>,
    parent_key: &str,
) {
    let (field, desc) = groups[level];
    // Buckets in the order of their first record after sorting by the key.
    let mut sorted = idx.to_vec();
    sort_by(base, table, &mut sorted, &[SortRule { field: field.id.clone(), desc }]);
    let mut buckets: Vec<(String, Option<Value>, Vec<usize>)> = vec![];
    for i in sorted {
        let (k, v) = group_key(base, table, &table.records[i], field);
        match buckets.iter_mut().find(|b| b.0 == k) {
            Some(b) => b.2.push(i),
            None => buckets.push((k, v, vec![i])),
        }
    }
    // Inside a group, records keep the view's order.
    let order: HashMap<usize, usize> = idx.iter().enumerate().map(|(p, &i)| (i, p)).collect();
    for (k, v, mut members) in buckets {
        members.sort_by_key(|i| order[i]);
        let full_key = format!("{parent_key}{}\u{1}{}\u{2}", field.id, k);
        let collapsed = view.collapsed.contains(&full_key);
        out.push(Row::Group {
            level,
            key: full_key.clone(),
            field: field.id.clone(),
            value: v.clone(),
            count: members.len(),
            collapsed,
        });
        if collapsed {
            continue;
        }
        preset.push((field.id.clone(), v));
        if level + 1 < groups.len() {
            group_level(base, table, view, groups, level + 1, &members, out, preset, &full_key);
        } else {
            out.extend(members.into_iter().map(Row::Record));
            out.push(Row::Add { level, preset: preset.clone() });
        }
        preset.pop();
    }
}

/// A column's statistic over `records`, as the bottom bar shows it.
pub fn stat(base: &Base, table: &Table, field: &Field, records: &[usize], s: Stat) -> String {
    let vals: Vec<Option<Value>> = records.iter().map(|&i| cell(base, table, &table.records[i], field)).collect();
    let total = vals.len();
    let filled = vals.iter().filter(|v| v.as_ref().is_some_and(|v| !v.is_empty())).count();
    let pct = |n: usize| if total == 0 { "0%".to_string() } else { format!("{}%", ((n as f64 / total as f64) * 1000.0).round() / 10.0) };
    let nums: Vec<f64> = vals.iter().filter_map(|v| v.as_ref().and_then(|v| match v {
        Value::List(l) if l.len() == 1 => l[0].as_number(),
        Value::Text(_) => None,
        x => x.as_number(),
    })).collect();
    let fmt = |n: f64| match &field.kind {
        k if k.is_numeric() => value::format_number(n, k),
        _ => value::format_plain((n * 100.0).round() / 100.0),
    };
    let dates: Vec<i64> = vals.iter().filter_map(|v| if let Some(Value::Date(d)) = v { Some(*d) } else { None }).collect();
    let date_fmt = |d: i64| match &field.kind {
        FieldKind::Date { format, time } | FieldKind::CreatedTime { format, time } | FieldKind::ModifiedTime { format, time } => {
            value::format_date(d, *format, *time)
        }
        _ => value::format_date(d, super::DateFormat::YmdSlash, false),
    };
    match s {
        Stat::None => String::new(),
        Stat::Count => format!("{total} 条记录"),
        Stat::Filled => format!("已填写 {filled}"),
        Stat::Empty => format!("未填写 {}", total - filled),
        Stat::FilledPct => format!("已填写 {}", pct(filled)),
        Stat::EmptyPct => format!("未填写 {}", pct(total - filled)),
        Stat::Unique => {
            let mut seen: Vec<String> = vec![];
            for v in vals.iter().flatten() {
                let t = value::display(base, table, &field.kind, v);
                if !t.is_empty() && !seen.contains(&t) {
                    seen.push(t);
                }
            }
            format!("唯一值 {}", seen.len())
        }
        Stat::Sum => format!("求和 {}", fmt(nums.iter().sum())),
        Stat::Average => {
            if nums.is_empty() {
                "平均值 -".into()
            } else {
                format!("平均值 {}", fmt(nums.iter().sum::<f64>() / nums.len() as f64))
            }
        }
        Stat::Max => nums.iter().copied().reduce(f64::max).map(|n| format!("最大值 {}", fmt(n))).unwrap_or("最大值 -".into()),
        Stat::Min => nums.iter().copied().reduce(f64::min).map(|n| format!("最小值 {}", fmt(n))).unwrap_or("最小值 -".into()),
        Stat::Range => {
            let (mx, mn) = (nums.iter().copied().reduce(f64::max), nums.iter().copied().reduce(f64::min));
            match (mx, mn) {
                (Some(a), Some(b)) => format!("极差 {}", fmt(a - b)),
                _ => "极差 -".into(),
            }
        }
        Stat::Checked => format!("已勾选 {}", vals.iter().filter(|v| matches!(v, Some(Value::Bool(true)))).count()),
        Stat::Unchecked => format!("未勾选 {}", vals.iter().filter(|v| !matches!(v, Some(Value::Bool(true)))).count()),
        Stat::CheckedPct => format!("已勾选 {}", pct(vals.iter().filter(|v| matches!(v, Some(Value::Bool(true)))).count())),
        Stat::Earliest => dates.iter().min().map(|d| format!("最早 {}", date_fmt(*d))).unwrap_or("最早 -".into()),
        Stat::Latest => dates.iter().max().map(|d| format!("最晚 {}", date_fmt(*d))).unwrap_or("最晚 -".into()),
    }
}
