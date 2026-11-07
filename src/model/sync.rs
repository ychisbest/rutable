//! Delta sync. A document changes item by item: its settings (`meta`: the
//! document without records), each record, and each table's row order.
//! A [`Patch`] carries the changed items with the versions they started
//! from, so whoever applies it can merge them three-way, item by item; a
//! [`Delta`] carries the items' current versions from the server.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use super::{Base, Id, Record};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetaChange {
    pub base: Json,
    pub new: Json,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecordChange {
    pub table: Id,
    pub id: Id,
    /// `None`: the record is new.
    pub base: Option<Arc<Record>>,
    /// `None`: the record is deleted.
    pub new: Option<Arc<Record>>,
}

/// A table's rows were reordered (beyond appending new and dropping deleted ones).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderChange {
    pub table: Id,
    pub base: Vec<Id>,
    pub new: Vec<Id>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Patch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<MetaChange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub records: Vec<RecordChange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub orders: Vec<OrderChange>,
}

impl Patch {
    pub fn is_empty(&self) -> bool {
        self.meta.is_none() && self.records.is_empty() && self.orders.is_empty()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecordState {
    pub table: Id,
    pub id: Id,
    /// `None`: deleted.
    pub record: Option<Arc<Record>>,
}

/// The items changed after a revision, as they are now.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Delta {
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Json>,
    /// In row order; new records go after the existing ones.
    #[serde(default)]
    pub records: Vec<RecordState>,
    /// Full row orders of the tables reordered.
    #[serde(default)]
    pub orders: Vec<(Id, Vec<Id>)>,
    /// Records per table, to check the result.
    #[serde(default)]
    pub counts: HashMap<Id, usize>,
}

/// What [`apply`] changed.
#[derive(Clone, Debug, Default)]
pub struct Touched {
    pub meta: bool,
    pub records: Vec<(Id, Id)>,
    pub orders: Vec<Id>,
}

/// The document without its records.
pub fn meta(b: &Base) -> Json {
    let mut m = b.clone();
    for t in &mut m.tables {
        t.records = vec![];
    }
    serde_json::to_value(&m).expect("document serializes")
}

/// Replaces the document's settings, keeping the records of the tables that remain.
pub fn set_meta(doc: &mut Base, meta: &Json) -> Result<(), String> {
    let mut next: Base = serde_json::from_value(meta.clone()).map_err(|e| format!("文档设置无效：{e}"))?;
    let mut old: HashMap<Id, Vec<Arc<Record>>> = doc.tables.drain(..).map(|t| (t.id, t.records)).collect();
    for t in &mut next.tables {
        t.records = old.remove(&t.id).unwrap_or_default();
    }
    *doc = next;
    Ok(())
}

fn ids(records: &[Arc<Record>]) -> Vec<Id> {
    records.iter().map(|r| r.id.clone()).collect()
}

/// The changes that turn `base` into `local`.
pub fn diff(base: &Base, local: &Base) -> Patch {
    let mut patch = Patch::default();
    let (mb, ml) = (meta(base), meta(local));
    if mb != ml {
        patch.meta = Some(MetaChange { base: mb, new: ml });
    }
    for lt in &local.tables {
        let Some(bt) = base.table(&lt.id) else {
            patch.records.extend(lt.records.iter().map(|r| RecordChange {
                table: lt.id.clone(),
                id: r.id.clone(),
                base: None,
                new: Some(r.clone()),
            }));
            continue;
        };
        let by_id: HashMap<&str, &Arc<Record>> = bt.records.iter().map(|r| (r.id.as_str(), r)).collect();
        let mut kept: HashSet<&str> = HashSet::with_capacity(lt.records.len());
        for r in &lt.records {
            kept.insert(r.id.as_str());
            match by_id.get(r.id.as_str()) {
                Some(&b) if Arc::ptr_eq(b, r) || **b == **r => {}
                b => patch.records.push(RecordChange {
                    table: lt.id.clone(),
                    id: r.id.clone(),
                    base: b.map(|&b| b.clone()),
                    new: Some(r.clone()),
                }),
            }
        }
        for b in &bt.records {
            if !kept.contains(b.id.as_str()) {
                patch.records.push(RecordChange { table: lt.id.clone(), id: b.id.clone(), base: Some(b.clone()), new: None });
            }
        }
        // Appending new rows and dropping deleted ones is not a reorder.
        let expected: Vec<&str> = bt
            .records
            .iter()
            .map(|r| r.id.as_str())
            .filter(|id| kept.contains(id))
            .chain(lt.records.iter().map(|r| r.id.as_str()).filter(|id| !by_id.contains_key(id)))
            .collect();
        if !lt.records.iter().map(|r| r.id.as_str()).eq(expected.into_iter()) {
            patch.orders.push(OrderChange { table: lt.id.clone(), base: ids(&bt.records), new: ids(&lt.records) });
        }
    }
    patch
}

fn json<T: Serialize>(v: &T) -> Json {
    serde_json::to_value(v).expect("serializes")
}

/// Applies `patch` to `doc`, merging three-way each item that changed in
/// `doc` too. Fails (leaving `doc` partly changed) on a conflict.
pub fn apply(doc: &mut Base, patch: &Patch) -> Result<Touched, String> {
    let mut touched = Touched::default();
    if let Some(m) = &patch.meta {
        let cur = meta(doc);
        let merged = crate::merge::merge_option(Some(&m.base), Some(&m.new), Some(&cur), "document")?
            .ok_or("文档设置不能删除")?;
        if merged != cur {
            set_meta(doc, &merged)?;
            touched.meta = true;
        }
    }
    let mut index: HashMap<Id, HashMap<Id, usize>> = HashMap::new();
    let mut removed: HashMap<Id, HashSet<Id>> = HashMap::new();
    for c in &patch.records {
        let Some(t) = doc.table_mut(&c.table) else {
            return Err(format!("数据冲突：数据表 {} 已被删除", c.table));
        };
        let idx = index.entry(c.table.clone()).or_insert_with(|| t.records.iter().enumerate().map(|(i, r)| (r.id.clone(), i)).collect());
        let gone = removed.entry(c.table.clone()).or_default();
        let at = idx.get(&c.id).copied().filter(|_| !gone.contains(&c.id));
        let cur = at.map(|i| &t.records[i]);
        let same = |a: Option<&Arc<Record>>, b: Option<&Arc<Record>>| match (a, b) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b) || **a == **b,
            (None, None) => true,
            _ => false,
        };
        let next: Option<Arc<Record>> = if same(cur, c.base.as_ref()) {
            c.new.clone()
        } else if same(cur, c.new.as_ref()) {
            continue;
        } else {
            let path = format!("{}/{}", c.table, c.id);
            let merged = crate::merge::merge_option(
                c.base.as_ref().map(json).as_ref(),
                c.new.as_ref().map(json).as_ref(),
                cur.map(json).as_ref(),
                &path,
            )
            .map_err(|e| if cur.is_none() || c.new.is_none() { format!("数据冲突：记录已在别处删除或修改（{path}）") } else { e })?;
            match merged {
                Some(v) => Some(Arc::new(serde_json::from_value(v).map_err(|e| format!("合并后的记录无效：{e}"))?)),
                None => None,
            }
        };
        match (at, next) {
            (Some(i), Some(r)) => t.records[i] = r,
            (Some(_), None) => {
                gone.insert(c.id.clone());
            }
            (None, Some(r)) => {
                idx.insert(c.id.clone(), t.records.len());
                t.records.push(r);
            }
            (None, None) => continue,
        }
        touched.records.push((c.table.clone(), c.id.clone()));
    }
    for (tid, gone) in removed {
        if let (false, Some(t)) = (gone.is_empty(), doc.table_mut(&tid)) {
            t.records.retain(|r| !gone.contains(&r.id));
        }
    }
    for o in &patch.orders {
        let Some(t) = doc.table_mut(&o.table) else {
            return Err(format!("数据冲突：数据表 {} 已被删除", o.table));
        };
        let cur = ids(&t.records);
        let order = merge_order(&o.base, &o.new, &cur).ok_or_else(|| format!("排序冲突：数据表 {} 的行顺序在别处也被调整", t.name))?;
        if order != cur {
            reorder(t, &order);
            touched.orders.push(o.table.clone());
        }
    }
    Ok(touched)
}

/// The row order after `base` became `new` here and `cur` elsewhere;
/// `cur` already holds exactly the rows that remain. Only one side may move
/// rows both sides have; rows added here keep the place given to them,
/// rows added elsewhere follow.
fn merge_order(base: &[Id], new: &[Id], cur: &[Id]) -> Option<Vec<Id>> {
    let (b, n, c): (HashSet<&Id>, HashSet<&Id>, HashSet<&Id>) = (base.iter().collect(), new.iter().collect(), cur.iter().collect());
    fn common<'a>(v: &'a [Id], sets: [&HashSet<&Id>; 3]) -> Vec<&'a Id> {
        v.iter().filter(|id| sets.iter().all(|s| s.contains(id))).collect()
    }
    let sets = [&b, &n, &c];
    let cb = common(base, sets);
    let common = |v| common(v, sets);
    if common(cur) == cb {
        let mut out: Vec<Id> = new.iter().filter(|id| c.contains(id)).cloned().collect();
        out.extend(cur.iter().filter(|id| !n.contains(id)).cloned());
        Some(out)
    } else if common(new) == cb {
        Some(cur.to_vec())
    } else {
        None
    }
}

fn reorder(t: &mut super::Table, order: &[Id]) {
    let rank: HashMap<&str, usize> = order.iter().enumerate().map(|(i, id)| (id.as_str(), i)).collect();
    // Stable: rows missing from `order` keep their place at the end.
    t.records.sort_by_key(|r| rank.get(r.id.as_str()).copied().unwrap_or(usize::MAX));
}

/// Brings `doc` (the version at the delta's starting revision) to the delta's revision.
pub fn apply_delta(doc: &mut Base, d: &Delta) -> Result<(), String> {
    if let Some(m) = &d.meta {
        set_meta(doc, m)?;
    }
    let mut by_table: HashMap<&str, Vec<&RecordState>> = HashMap::new();
    for r in &d.records {
        by_table.entry(r.table.as_str()).or_default().push(r);
    }
    for (tid, states) in by_table {
        let Some(t) = doc.table_mut(tid) else { continue };
        let mut next: HashMap<&str, Option<&Arc<Record>>> = states.iter().map(|s| (s.id.as_str(), s.record.as_ref())).collect();
        let mut out = Vec::with_capacity(t.records.len() + states.len());
        for r in t.records.drain(..) {
            match next.remove(r.id.as_str()) {
                None => out.push(r),
                Some(Some(n)) => out.push(n.clone()),
                Some(None) => {}
            }
        }
        for s in states {
            if let Some(Some(n)) = next.remove(s.id.as_str()) {
                out.push(n.clone());
            }
        }
        t.records = out;
    }
    for (tid, order) in &d.orders {
        if let Some(t) = doc.table_mut(tid) {
            reorder(t, order);
        }
    }
    for t in &doc.tables {
        if d.counts.get(&t.id).is_some_and(|&n| n != t.records.len()) {
            return Err(format!("数据表 {} 的记录数与服务器不一致", t.name));
        }
    }
    Ok(())
}

/// Three-way merge of whole documents: `local`'s changes since `base`, applied onto `remote`.
pub fn merge(base: &Base, local: &Base, remote: &Base) -> Result<Base, String> {
    let patch = diff(base, local);
    let mut out = remote.clone();
    if !patch.is_empty() {
        apply(&mut out, &patch)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Value, sample};

    fn doc() -> Base {
        let mut b = sample::empty();
        let me = b.me.clone();
        let t = &mut b.tables[0];
        for _ in 0..3 {
            let r = t.new_record(&me);
            t.push_record(r);
        }
        b
    }
    fn set(b: &mut Base, i: usize, text: &str) {
        let f = b.tables[0].fields[0].id.clone();
        Arc::make_mut(&mut b.tables[0].records[i]).cells.insert(f, Value::Text(text.into()));
    }
    fn text(b: &Base, i: usize) -> Option<Value> {
        b.tables[0].records[i].cells.get(&b.tables[0].fields[0].id).cloned()
    }

    #[test]
    fn untouched_documents_diff_empty_and_clone_shares_records() {
        let a = doc();
        let b = a.clone();
        assert!(diff(&a, &b).is_empty());
        assert!(Arc::ptr_eq(&a.tables[0].records[0], &b.tables[0].records[0]));
    }

    #[test]
    fn a_cell_edit_sends_only_that_record() {
        let a = doc();
        let mut b = a.clone();
        set(&mut b, 1, "x");
        let p = diff(&a, &b);
        assert!(p.meta.is_none() && p.orders.is_empty());
        assert_eq!(p.records.len(), 1);
        let mut c = a.clone();
        apply(&mut c, &p).unwrap();
        assert_eq!(text(&c, 1), Some(Value::Text("x".into())));
    }

    #[test]
    fn appends_and_deletes_are_not_reorders_but_moves_are() {
        let a = doc();
        let mut b = a.clone();
        b.tables[0].records.remove(0);
        let me = b.me.clone();
        let r = b.tables[0].new_record(&me);
        b.tables[0].push_record(r);
        let p = diff(&a, &b);
        assert!(p.orders.is_empty());
        assert_eq!(p.records.len(), 2);
        let mut c = a.clone();
        c.tables[0].records.swap(0, 2);
        assert_eq!(diff(&a, &c).orders.len(), 1);
    }

    #[test]
    fn concurrent_edits_of_different_records_and_cells_merge() {
        let base = doc();
        let mut local = base.clone();
        let mut remote = base.clone();
        set(&mut local, 0, "local");
        set(&mut remote, 2, "remote");
        let f2 = base.tables[0].fields[1].id.clone();
        Arc::make_mut(&mut remote.tables[0].records[0]).cells.insert(f2.clone(), Value::Number(3.0));
        let m = merge(&base, &local, &remote).unwrap();
        assert_eq!(text(&m, 0), Some(Value::Text("local".into())));
        assert_eq!(text(&m, 2), Some(Value::Text("remote".into())));
        assert_eq!(m.tables[0].records[0].cells.get(&f2), Some(&Value::Number(3.0)));
    }

    #[test]
    fn same_cell_conflict_is_an_error() {
        let base = doc();
        let mut local = base.clone();
        let mut remote = base.clone();
        set(&mut local, 0, "a");
        set(&mut remote, 0, "b");
        assert!(merge(&base, &local, &remote).is_err());
    }

    #[test]
    fn delta_reproduces_the_server_document() {
        let base = doc();
        let mut server = base.clone();
        set(&mut server, 0, "s");
        server.tables[0].records.remove(1);
        let me = server.me.clone();
        let r = server.tables[0].new_record(&me);
        server.tables[0].push_record(r);
        server.tables[0].records.swap(0, 1);
        server.name = "renamed".into();
        let p = diff(&base, &server);
        let d = Delta {
            revision: 2,
            meta: p.meta.as_ref().map(|m| m.new.clone()),
            records: p.records.iter().map(|c| RecordState { table: c.table.clone(), id: c.id.clone(), record: c.new.clone() }).collect(),
            orders: p.orders.iter().map(|o| (o.table.clone(), o.new.clone())).collect(),
            counts: server.tables.iter().map(|t| (t.id.clone(), t.records.len())).collect(),
        };
        let mut c = base.clone();
        apply_delta(&mut c, &d).unwrap();
        assert!(diff(&server, &c).is_empty());
        assert_eq!(c.name, "renamed");
    }
}
