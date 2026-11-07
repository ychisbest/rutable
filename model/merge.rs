//! Three-way merge for documents. Arrays of objects with stable IDs merge by ID.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub fn merge(base: &Value, local: &Value, remote: &Value) -> Result<Value, String> {
    merge_option(Some(base), Some(local), Some(remote), "document")?.ok_or("文档已删除".into())
}
fn merge_option(base: Option<&Value>, local: Option<&Value>, remote: Option<&Value>, path: &str) -> Result<Option<Value>, String> {
    if local == base {
        return Ok(remote.cloned());
    }
    if remote == base || remote == local {
        return Ok(local.cloned());
    }
    if matches!(path.rsplit('/').next(), Some("updated_at" | "seq")) {
        if let (Some(Value::Number(a)), Some(Value::Number(b))) = (local, remote) {
            if let (Some(a), Some(b)) = (a.as_i64(), b.as_i64()) {
                return Ok(Some(Value::from(a.max(b))));
            }
        }
    }
    if path.ends_with("/history") {
        if let (Some(Value::Array(local)), Some(Value::Array(remote))) = (local, remote) {
            let mut history = remote.clone();
            for event in local {
                if !history.contains(event) {
                    history.push(event.clone())
                }
            }
            history.sort_by_key(|v| v.get(0).and_then(Value::as_i64).unwrap_or(0));
            return Ok(Some(Value::Array(history)));
        }
    }
    match (base, local, remote) {
        (Some(Value::Object(b)), Some(Value::Object(l)), Some(Value::Object(r))) => {
            let keys: BTreeSet<_> = b.keys().chain(l.keys()).chain(r.keys()).collect();
            let mut out = serde_json::Map::new();
            for k in keys {
                if let Some(v) = merge_option(b.get(k), l.get(k), r.get(k), &format!("{path}/{k}"))? {
                    out.insert(k.clone(), v);
                }
            }
            Ok(Some(Value::Object(out)))
        }
        (Some(Value::Array(b)), Some(Value::Array(l)), Some(Value::Array(r))) => {
            if let (Some(b), Some(l), Some(r)) = (keyed(b), keyed(l), keyed(r)) {
                let keys: BTreeSet<_> = b.keys().chain(l.keys()).chain(r.keys()).collect();
                let mut merged = BTreeMap::new();
                for k in keys {
                    if let Some(v) = merge_option(b.get(k).copied(), l.get(k).copied(), r.get(k).copied(), &format!("{path}/{k}"))? {
                        merged.insert(k.clone(), v);
                    }
                }
                let ba = base.unwrap().as_array().unwrap();
                let la = local.unwrap().as_array().unwrap();
                let ra = remote.unwrap().as_array().unwrap();
                let ids = |a: &Vec<Value>| a.iter().map(|v| key(v).unwrap()).collect::<Vec<_>>();
                let (bi, li, ri) = (ids(ba), ids(la), ids(ra));
                let existing = |order: &Vec<String>| {
                    order.iter().filter(|id| b.contains_key(*id) && l.contains_key(*id) && r.contains_key(*id)).cloned().collect::<Vec<_>>()
                };
                let (bo, lo, ro) = (existing(&bi), existing(&li), existing(&ri));
                if lo != bo && ro != bo && lo != ro {
                    return Err(format!("排序冲突：{path}"));
                }
                let preferred = if lo != bo { &li } else { &ri };
                let mut out = vec![];
                for id in preferred.iter().chain(li.iter()).chain(ri.iter()) {
                    if let Some(v) = merged.remove(id) {
                        out.push(v)
                    }
                }
                Ok(Some(Value::Array(out)))
            } else {
                Err(format!("数据冲突：{path}"))
            }
        }
        _ => Err(format!("数据冲突：{path}")),
    }
}
fn key(v: &Value) -> Option<String> {
    v.get("id").and_then(Value::as_str).or_else(|| v.as_str()).map(str::to_string)
}
fn keyed(v: &[Value]) -> Option<BTreeMap<String, &Value>> {
    let mut out = BTreeMap::new();
    for v in v {
        if out.insert(key(v)?, v).is_some() {
            return None;
        }
    }
    Some(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn different_records_and_fields_merge() {
        let b = json!({"tables":[{"id":"t","records":[{"id":"a","x":1,"y":1}]}],"order":["t"]});
        let mut l = b.clone();
        let mut r = b.clone();
        l["tables"][0]["records"][0]["x"] = json!(2);
        r["tables"][0]["records"][0]["y"] = json!(3);
        let m = merge(&b, &l, &r).unwrap();
        assert_eq!(m["tables"][0]["records"][0], json!({"id":"a","x":2,"y":3}));
    }
    #[test]
    fn record_metadata_does_not_block_independent_field_edits() {
        let b = json!({"id":"r","cells":{"a":1,"b":1},"updated_at":1,"history":[]});
        let l = json!({"id":"r","cells":{"a":2,"b":1},"updated_at":2,"history":[[2,"a"]]});
        let r = json!({"id":"r","cells":{"a":1,"b":3},"updated_at":3,"history":[[3,"b"]]});
        let m = merge(&b, &l, &r).unwrap();
        assert_eq!(m["updated_at"], 3);
        assert_eq!(m["cells"], json!({"a":2,"b":3}));
        assert_eq!(m["history"].as_array().unwrap().len(), 2);
    }
    #[test]
    fn conflicts_do_not_overwrite() {
        let b = json!({"x":1});
        assert!(merge(&b, &json!({"x":2}), &json!({"x":3})).is_err());
    }
    #[test]
    fn additions_and_deletions_merge_by_id() {
        let b = json!([{"id":"a"},{"id":"b"}]);
        let l = json!([{"id":"b"},{"id":"c"}]);
        let r = json!([{"id":"a"},{"id":"b"},{"id":"d"}]);
        assert_eq!(merge(&b, &l, &r).unwrap(), json!([{"id":"b"},{"id":"d"},{"id":"c"}]));
    }
    #[test]
    fn deleting_a_record_changed_elsewhere_conflicts() {
        assert!(merge(&json!([{"id":"a","x":1}]), &json!([]), &json!([{"id":"a","x":2}])).is_err());
    }
}
