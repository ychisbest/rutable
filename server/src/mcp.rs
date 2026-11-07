//! MCP (Model Context Protocol) over streamable HTTP: AI tools read and edit
//! the bases. Each request is one JSON-RPC message (or a batch) answered with
//! JSON. Writes go through the same merge and revision as the desktop, so
//! open windows show them within a second.

use super::*;
use rutable_model::{FieldKind, Table, query, value};
use serde_json::{Value as J, json};

const PROTOCOL: &str = "2025-06-18";
const MAX_ROWS: usize = 500;

pub async fn not_streaming() -> StatusCode {
    // No server-initiated stream; clients fall back to plain requests.
    StatusCode::METHOD_NOT_ALLOWED
}

pub async fn handle(State(state): State<App>, Json(body): Json<J>) -> Response {
    let batch = body.is_array();
    let messages = if let J::Array(items) = body { items } else { vec![body] };
    let mut out = vec![];
    for m in messages {
        if let Some(r) = answer(&state, m).await {
            out.push(r);
        }
    }
    if out.is_empty() {
        return StatusCode::ACCEPTED.into_response();
    }
    if batch { Json(J::Array(out)).into_response() } else { Json(out.remove(0)).into_response() }
}

async fn answer(state: &App, m: J) -> Option<J> {
    let id = m.get("id").cloned()?; // notifications get no answer
    let method = m["method"].as_str().unwrap_or("");
    let params = m.get("params").cloned().unwrap_or(J::Null);
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params["protocolVersion"].as_str().unwrap_or(PROTOCOL),
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {"name": "rutable", "title": "Rutable 多维表格", "version": env!("CARGO_PKG_VERSION")},
            "instructions": "Rutable 是多维表格。先用 list_bases 找到 base_id（没有合适的就用 create_base 新建），再用 describe_table 查看字段名和类型，然后读写记录。字段一律按字段名传值；单选/多选传选项名（多选可传数组），日期传 \"YYYY-MM-DD HH:MM\"（北京时间），复选框传 true/false，传 null 清空。打开的 Rutable 客户端会在约 1 秒内显示修改。"
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools()})),
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or("").to_string();
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let state = state.clone();
            let r = db(state, move |store| call(store, &name, &args)).await;
            Ok(match r {
                Ok(v) => json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default()}], "structuredContent": v}),
                // Tool errors go back to the model as results it can act on.
                Err(e) => json!({"content": [{"type": "text", "text": e.1}], "isError": true}),
            })
        }
        _ => Err(json!({"code": -32601, "message": format!("Method not found: {method}")})),
    };
    Some(match result {
        Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
        Err(e) => json!({"jsonrpc": "2.0", "id": id, "error": e}),
    })
}

fn tools() -> J {
    let base = json!({"type": "string", "description": "多维表格 ID（list_bases 返回的 base_id）"});
    let table = json!({"type": "string", "description": "数据表 ID 或名称"});
    let fields = json!({"type": "object", "description": "字段名 → 值", "additionalProperties": true});
    json!([
        {"name": "create_base", "title": "新建多维表格",
         "description": "Creates a new base (多维表格) with its first table, named `table`, holding the given text fields (the first is the primary field) and no records. Returns base_id and table_id; change field types with create_field, add tables with create_table.",
         "inputSchema": {"type": "object", "properties": {"name": {"type": "string"},
            "icon": {"type": "string", "description": "可选，图标（如一个 emoji）"},
            "table": {"type": "string", "description": "第一张数据表的名称，默认“数据表”"},
            "fields": {"type": "array", "items": {"type": "string"}, "description": "第一张数据表的文本字段，第一个是主字段"}},
            "required": ["name"]}},
        {"name": "list_bases", "title": "列出多维表格",
         "description": "List all bases (多维表格) with their tables and record counts.",
         "inputSchema": {"type": "object", "properties": {}}, "annotations": {"readOnlyHint": true}},
        {"name": "describe_table", "title": "查看数据表结构",
         "description": "Fields of a table: name, type, select options; the first field is the primary (title) field.",
         "inputSchema": {"type": "object", "properties": {"base_id": base, "table": table}, "required": ["base_id", "table"]},
         "annotations": {"readOnlyHint": true}},
        {"name": "list_records", "title": "查询记录",
         "description": "Records of a table in their order. `where` keeps records whose field text contains the given text (all must match). Returns at most `limit` (default 50, max 500) after `offset`, and `total` matching.",
         "inputSchema": {"type": "object", "properties": {
            "base_id": base, "table": table,
            "where": {"type": "object", "description": "字段名 → 包含的文字", "additionalProperties": {"type": "string"}},
            "fields": {"type": "array", "items": {"type": "string"}, "description": "只返回这些字段"},
            "limit": {"type": "integer", "minimum": 1, "maximum": MAX_ROWS}, "offset": {"type": "integer", "minimum": 0}},
            "required": ["base_id", "table"]},
         "annotations": {"readOnlyHint": true}},
        {"name": "create_records", "title": "新增记录",
         "description": "Adds records (at most 500). New select options are created as needed. Returns the new record IDs.",
         "inputSchema": {"type": "object", "properties": {"base_id": base, "table": table,
            "records": {"type": "array", "items": fields, "maxItems": MAX_ROWS}}, "required": ["base_id", "table", "records"]}},
        {"name": "update_records", "title": "修改记录",
         "description": "Changes fields of records (at most 500); fields not given stay as they are; null clears a field.",
         "inputSchema": {"type": "object", "properties": {"base_id": base, "table": table,
            "records": {"type": "array", "maxItems": MAX_ROWS, "items": {"type": "object", "properties": {"record_id": {"type": "string"}, "fields": fields}, "required": ["record_id", "fields"]}}},
            "required": ["base_id", "table", "records"]}},
        {"name": "delete_records", "title": "删除记录",
         "description": "Deletes records by ID (at most 500).",
         "inputSchema": {"type": "object", "properties": {"base_id": base, "table": table,
            "record_ids": {"type": "array", "items": {"type": "string"}, "maxItems": MAX_ROWS}}, "required": ["base_id", "table", "record_ids"]},
         "annotations": {"destructiveHint": true}},
        {"name": "create_field", "title": "新增字段",
         "description": "Adds a field to a table. type: text, number, single_select, multi_select, date, datetime, checkbox, url, email, phone.",
         "inputSchema": {"type": "object", "properties": {"base_id": base, "table": table,
            "name": {"type": "string"},
            "type": {"type": "string", "enum": ["text", "number", "single_select", "multi_select", "date", "datetime", "checkbox", "url", "email", "phone"]},
            "options": {"type": "array", "items": {"type": "string"}, "description": "单选/多选的选项"}},
            "required": ["base_id", "table", "name", "type"]}},
        {"name": "create_table", "title": "新建数据表",
         "description": "Adds a table to a base, with the given text fields (the first is the primary field).",
         "inputSchema": {"type": "object", "properties": {"base_id": base, "name": {"type": "string"},
            "fields": {"type": "array", "items": {"type": "string"}}}, "required": ["base_id", "name"]}}
    ])
}

fn bad(msg: impl Into<String>) -> Error {
    error(StatusCode::BAD_REQUEST, msg)
}

fn arg<'a>(a: &'a J, k: &str) -> Result<&'a str> {
    a[k].as_str().filter(|s| !s.is_empty()).ok_or_else(|| bad(format!("缺少参数 {k}")))
}

fn table_id(doc: &Base, t: &str) -> Result<String> {
    doc.tables
        .iter()
        .find(|x| x.id == t)
        .or_else(|| doc.tables.iter().find(|x| x.name == t))
        .map(|x| x.id.clone())
        .ok_or_else(|| bad(format!("没有数据表 {t}；可用：{}", doc.tables.iter().map(|x| x.name.as_str()).collect::<Vec<_>>().join("、"))))
}

fn type_name(k: &FieldKind) -> &'static str {
    match k {
        FieldKind::Text => "text",
        FieldKind::Number { .. } => "number",
        FieldKind::Currency { .. } => "currency",
        FieldKind::Progress { .. } => "progress",
        FieldKind::Rating { .. } => "rating",
        FieldKind::SingleSelect { .. } => "single_select",
        FieldKind::MultiSelect { .. } => "multi_select",
        FieldKind::Date { .. } => "date",
        FieldKind::Checkbox => "checkbox",
        FieldKind::User { .. } => "user",
        FieldKind::Phone => "phone",
        FieldKind::Url => "url",
        FieldKind::Email => "email",
        FieldKind::Attachment => "attachment (read only here)",
        FieldKind::Link { .. } => "link (record titles, comma separated)",
        FieldKind::Lookup { .. } => "lookup (computed)",
        FieldKind::Formula { .. } => "formula (computed)",
        FieldKind::Location => "location",
        FieldKind::AutoNumber => "auto_number (computed)",
        FieldKind::CreatedTime { .. } => "created_time (computed)",
        FieldKind::ModifiedTime { .. } => "modified_time (computed)",
        FieldKind::CreatedBy => "created_by (computed)",
        FieldKind::ModifiedBy => "modified_by (computed)",
        FieldKind::Barcode => "barcode",
        FieldKind::Button { .. } => "button (computed)",
    }
}

/// A cell as JSON for the model: numbers stay numbers, checkboxes booleans,
/// multi-selects lists; the rest is the text the app shows.
fn out_value(doc: &Base, t: &Table, f: &rutable_model::Field, v: &rutable_model::Value) -> J {
    use rutable_model::Value as V;
    match (&f.kind, v) {
        (k, V::Number(n)) if k.is_numeric() => json!(n),
        (FieldKind::Checkbox, V::Bool(b)) => json!(b),
        (FieldKind::MultiSelect { .. }, _) => {
            json!(value::display(doc, t, &f.kind, v).split(',').map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>())
        }
        _ => json!(value::display(doc, t, &f.kind, v)),
    }
}

fn record_json(doc: &Base, t: &Table, r: &rutable_model::Record, only: &Option<Vec<String>>) -> J {
    let mut m = serde_json::Map::new();
    for f in &t.fields {
        if only.as_ref().is_some_and(|o| !o.contains(&f.name)) {
            continue;
        }
        if let Some(v) = query::cell(doc, t, r, f) {
            if !v.is_empty() {
                m.insert(f.name.clone(), out_value(doc, t, f, &v));
            }
        }
    }
    json!({"record_id": r.id, "fields": m})
}

/// Sets `fields` (by name) on record `rid` of table `tid`; select options
/// that do not exist are added to their field.
fn write_fields(doc: &mut Base, tid: &str, rid: &str, fields: &serde_json::Map<String, J>) -> Result<()> {
    let snapshot = doc.clone();
    let t = doc.table_mut(tid).unwrap();
    let mut sets = vec![];
    for (name, v) in fields {
        let fi = t.fields.iter().position(|f| &f.name == name).ok_or_else(|| {
            bad(format!("没有字段 {name}；可用：{}", t.fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>().join("、")))
        })?;
        let f = &mut t.fields[fi];
        if f.kind.is_computed() || matches!(f.kind, FieldKind::Attachment) {
            return Err(bad(format!("字段 {name} 是 {}，不能直接写入", type_name(&f.kind))));
        }
        let text = match v {
            J::Null => String::new(),
            J::String(s) => s.clone(),
            J::Bool(b) => b.to_string(),
            J::Number(n) => n.to_string(),
            J::Array(items) => items.iter().map(|x| x.as_str().map(str::to_string).unwrap_or_else(|| x.to_string())).collect::<Vec<_>>().join(","),
            J::Object(_) => return Err(bad(format!("字段 {name} 的值不能是对象"))),
        };
        let parsed = if text.is_empty() { None } else { value::parse(&snapshot, &mut f.kind, &text) };
        if parsed.is_none() && !text.is_empty() {
            return Err(bad(format!("字段 {name}（{}）不能识别值 {text}", type_name(&f.kind))));
        }
        sets.push((f.id.clone(), f.name.clone(), parsed));
    }
    let me = doc.me.clone();
    let t = doc.table_mut(tid).unwrap();
    let r = t.record_mut(rid).ok_or_else(|| bad(format!("没有记录 {rid}")))?;
    let now = rutable_model::now_ms();
    for (fid, fname, v) in sets {
        let old = r.cells.get(&fid).cloned();
        if old == v {
            continue;
        }
        let show = |v: &Option<rutable_model::Value>| match v {
            Some(rutable_model::Value::Text(s)) => s.clone(),
            Some(x) => serde_json::to_string(x).unwrap_or_default(),
            None => String::new(),
        };
        r.history.push((now, me.clone(), fname, show(&old), format!("{}（AI）", show(&v))));
        match v {
            Some(v) => r.cells.insert(fid, v),
            None => r.cells.remove(&fid),
        };
        r.updated_at = now;
        r.updated_by = me.clone();
    }
    Ok(())
}

/// Replaces a new table's default fields with text fields named by `a["fields"]`.
fn text_fields(t: &mut Table, a: &J) -> Result<()> {
    let names: Vec<String> = a["fields"].as_array().map(|x| x.iter().filter_map(|n| n.as_str()).map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect()).unwrap_or_default();
    if names.is_empty() {
        return Ok(());
    }
    if let Some(dup) = names.iter().enumerate().find(|(i, n)| names[..*i].contains(n)) {
        return Err(bad(format!("字段 {} 重复", dup.1)));
    }
    t.fields = names
        .iter()
        .map(|n| rutable_model::Field { id: rutable_model::new_id("fld"), name: n.clone(), kind: FieldKind::Text, description: String::new() })
        .collect();
    for v in &mut t.views {
        v.field_order.clear();
        v.hidden.clear();
    }
    Ok(())
}

fn call(store: &mut Store, name: &str, a: &J) -> Result<J> {
    if name == "create_base" {
        let mut doc = rutable_model::sample::empty();
        let t = &mut doc.tables[0];
        if let Some(tname) = a["table"].as_str().map(str::trim).filter(|n| !n.is_empty()) {
            t.name = tname.to_string();
        }
        t.records.clear();
        text_fields(t, a)?;
        let e = workspace::create_base(store, doc, arg(a, "name")?, a["icon"].as_str().unwrap_or(""))?;
        let doc = &store.load(&e.id)?.1;
        return Ok(json!({"base_id": e.id, "name": e.name, "revision": e.revision,
            "tables": doc.tables.iter().map(|t| json!({"table_id": t.id, "name": t.name})).collect::<Vec<_>>()}));
    }
    if name == "list_bases" {
        let list = workspace::entries(&store.conn, false)?;
        let mut out = vec![];
        for e in list {
            let doc = &store.load(&e.id)?.1;
            out.push(json!({"base_id": e.id, "name": e.name, "icon": e.icon,
                "tables": doc.tables.iter().map(|t| json!({"table_id": t.id, "name": t.name, "records": t.records.len()})).collect::<Vec<_>>()}));
        }
        return Ok(json!({"bases": out}));
    }
    let base_id = arg(a, "base_id")?.to_string();
    valid_id(&base_id)?;
    let doc = store.load(&base_id).map_err(|_| bad(format!("没有多维表格 {base_id}，先用 list_bases 查看")))?.1.clone();
    let mut next = doc.clone();
    let result = match name {
        "describe_table" => {
            let t = next.table(&table_id(&doc, arg(a, "table")?)?).unwrap();
            return Ok(json!({"table_id": t.id, "name": t.name, "records": t.records.len(),
                "fields": t.fields.iter().enumerate().map(|(i, f)| {
                    let mut o = json!({"name": f.name, "type": type_name(&f.kind)});
                    if i == 0 { o["primary"] = json!(true); }
                    if let Some(opts) = f.kind.options() { o["options"] = json!(opts.iter().map(|x| &x.name).collect::<Vec<_>>()); }
                    if !f.description.is_empty() { o["description"] = json!(f.description); }
                    o
                }).collect::<Vec<_>>()}));
        }
        "list_records" => {
            let t = doc.table(&table_id(&doc, arg(a, "table")?)?).unwrap();
            let conds: Vec<(&rutable_model::Field, String)> = match a["where"].as_object() {
                Some(w) => w
                    .iter()
                    .map(|(k, v)| {
                        let f = t.field_by_name(k).ok_or_else(|| bad(format!("没有字段 {k}")))?;
                        Ok((f, v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()).to_lowercase()))
                    })
                    .collect::<Result<_>>()?,
                None => vec![],
            };
            let only: Option<Vec<String>> = a["fields"].as_array().map(|x| x.iter().filter_map(|s| s.as_str().map(str::to_string)).collect());
            let limit = a["limit"].as_u64().unwrap_or(50).clamp(1, MAX_ROWS as u64) as usize;
            let offset = a["offset"].as_u64().unwrap_or(0) as usize;
            let matching: Vec<&rutable_model::Record> = t
                .records
                .iter()
                .map(|r| &**r)
                .filter(|r| {
                    conds.iter().all(|(f, want)| {
                        query::cell(&doc, t, r, f).map(|v| value::display(&doc, t, &f.kind, &v).to_lowercase().contains(want)).unwrap_or(want.is_empty())
                    })
                })
                .collect();
            return Ok(json!({"total": matching.len(), "offset": offset,
                "records": matching.iter().skip(offset).take(limit).map(|r| record_json(&doc, t, r, &only)).collect::<Vec<_>>()}));
        }
        "create_records" => {
            let tid = table_id(&doc, arg(a, "table")?)?;
            let rows = a["records"].as_array().ok_or_else(|| bad("records 必须是数组"))?;
            if rows.len() > MAX_ROWS {
                return Err(bad("一次最多 500 条"));
            }
            let me = next.me.clone();
            let mut ids = vec![];
            for row in rows {
                let fields = row.as_object().ok_or_else(|| bad("每条记录是 字段名→值 的对象"))?;
                let t = next.table_mut(&tid).unwrap();
                let r = t.new_record(&me);
                let rid = r.id.clone();
                t.push_record(r);
                write_fields(&mut next, &tid, &rid, fields)?;
                ids.push(rid);
            }
            json!({"created": ids.len(), "record_ids": ids})
        }
        "update_records" => {
            let tid = table_id(&doc, arg(a, "table")?)?;
            let rows = a["records"].as_array().ok_or_else(|| bad("records 必须是数组"))?;
            if rows.len() > MAX_ROWS {
                return Err(bad("一次最多 500 条"));
            }
            for row in rows {
                let rid = row["record_id"].as_str().ok_or_else(|| bad("缺少 record_id"))?;
                let fields = row["fields"].as_object().ok_or_else(|| bad("fields 是 字段名→值 的对象"))?;
                write_fields(&mut next, &tid, rid, fields)?;
            }
            json!({"updated": rows.len()})
        }
        "delete_records" => {
            let tid = table_id(&doc, arg(a, "table")?)?;
            let ids: Vec<String> = a["record_ids"].as_array().ok_or_else(|| bad("record_ids 必须是数组"))?.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
            if ids.len() > MAX_ROWS {
                return Err(bad("一次最多 500 条"));
            }
            let t = next.table_mut(&tid).unwrap();
            let before = t.records.len();
            t.records.retain(|r| !ids.contains(&r.id));
            let n = before - t.records.len();
            next.sync_links();
            json!({"deleted": n})
        }
        "create_field" => {
            let tid = table_id(&doc, arg(a, "table")?)?;
            let fname = arg(a, "name")?.trim().to_string();
            let t = next.table_mut(&tid).unwrap();
            if t.field_by_name(&fname).is_some() {
                return Err(bad(format!("字段 {fname} 已存在")));
            }
            let opts = |a: &J| -> Vec<rutable_model::SelectOption> {
                a["options"].as_array().map(|o| o.iter().filter_map(|x| x.as_str()).enumerate()
                    .map(|(i, n)| rutable_model::SelectOption { id: rutable_model::new_id("opt"), name: n.to_string(), color: (i % 11) as u8 }).collect()).unwrap_or_default()
            };
            let kind = match arg(a, "type")? {
                "text" => FieldKind::Text,
                "number" => FieldKind::Number { precision: 0, thousands: false, percent: false },
                "single_select" => FieldKind::SingleSelect { options: opts(a) },
                "multi_select" => FieldKind::MultiSelect { options: opts(a) },
                "date" => FieldKind::Date { format: rutable_model::DateFormat::YmdSlash, time: false },
                "datetime" => FieldKind::Date { format: rutable_model::DateFormat::YmdSlash, time: true },
                "checkbox" => FieldKind::Checkbox,
                "url" => FieldKind::Url,
                "email" => FieldKind::Email,
                "phone" => FieldKind::Phone,
                other => return Err(bad(format!("不支持的字段类型 {other}"))),
            };
            let fid = rutable_model::new_id("fld");
            t.fields.push(rutable_model::Field { id: fid.clone(), name: fname.clone(), kind, description: String::new() });
            json!({"created": fname, "field_id": fid})
        }
        "create_table" => {
            let tname = arg(a, "name")?.trim().to_string();
            if next.tables.iter().any(|t| t.name == tname) {
                return Err(bad(format!("数据表 {tname} 已存在")));
            }
            let tid = next.new_table(&tname);
            let t = next.table_mut(&tid).unwrap();
            t.records.clear();
            text_fields(t, a)?;
            json!({"table_id": tid, "name": tname})
        }
        other => return Err(bad(format!("没有工具 {other}"))),
    };
    let patch = sync::diff(&doc, &next);
    let rev = store.commit(&base_id, &patch)?;
    let mut r = result;
    r["revision"] = json!(rev);
    Ok(r)
}
