//! The list of bases (多维表格): their order, the trash, names and icons
//! (kept in each document, so renaming inside a base and in the list agree),
//! and waiting for a change.

use super::*;

#[derive(Debug, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub revision: u64,
    pub updated_at: i64,
    pub tables: usize,
    pub records: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<i64>,
}

/// Lists documents that predate the list (or were imported) at its end.
pub(crate) fn adopt(conn: &Connection) -> Result<()> {
    let ids: Vec<String> = {
        let mut q = conn.prepare("SELECT id FROM doc_meta WHERE id NOT IN (SELECT id FROM workspace) ORDER BY id")?;
        q.query_map([], |r| r.get(0))?.collect::<std::result::Result<_, _>>()?
    };
    for id in ids {
        add(conn, &id)?;
    }
    Ok(())
}

pub(crate) fn add(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO workspace(id,pos,created_at) VALUES (?1,(SELECT coalesce(max(pos),-1)+1 FROM workspace),?2)",
        params![id, rutable_model::now_ms()],
    )?;
    Ok(())
}

pub(crate) fn entries(conn: &Connection, deleted: bool) -> Result<Vec<Entry>> {
    let mut q = conn.prepare(
        "SELECT w.id, m.revision, m.updated_at, w.deleted_at, m.meta FROM workspace w JOIN doc_meta m ON m.id=w.id
         WHERE (w.deleted_at IS NOT NULL)=?1 ORDER BY CASE WHEN ?1 THEN -w.deleted_at ELSE w.pos END",
    )?;
    let rows = q.query_map([deleted], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, Option<i64>>(3)?, r.get::<_, String>(4)?))
    })?;
    let mut out = vec![];
    for row in rows {
        let (id, revision, updated_at, deleted_at, meta) = row?;
        let meta: serde_json::Value = serde_json::from_str(&meta)?;
        let records: i64 =
            conn.query_row("SELECT count(*) FROM doc_records WHERE document_id=?1 AND body IS NOT NULL", [&id], |r| r.get(0))?;
        out.push(Entry {
            name: meta["name"].as_str().unwrap_or("").to_string(),
            icon: meta["icon"].as_str().unwrap_or("").to_string(),
            tables: meta["tables"].as_array().map_or(0, Vec::len),
            id,
            revision: revision as u64,
            updated_at,
            records: records as usize,
            deleted_at,
        });
    }
    Ok(out)
}

fn entry(conn: &Connection, id: &str) -> Result<Entry> {
    entries(conn, false)?.into_iter().find(|e| e.id == id).ok_or_else(|| error(StatusCode::NOT_FOUND, "document_not_found"))
}

pub async fn list(State(state): State<App>) -> Result<Json<Vec<Entry>>> {
    db(state, |store| entries(&store.conn, false)).await.map(Json)
}

pub async fn trash(State(state): State<App>) -> Result<Json<Vec<Entry>>> {
    db(state, |store| entries(&store.conn, true)).await.map(Json)
}

fn check_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 100 || name.chars().any(char::is_control) {
        return Err(error(StatusCode::BAD_REQUEST, "名称不能为空，且不超过 100 个字符"));
    }
    Ok(name.to_string())
}

fn check_icon(icon: &str) -> Result<String> {
    let icon = icon.trim();
    if icon.len() > 32 || icon.chars().any(char::is_control) {
        return Err(error(StatusCode::BAD_REQUEST, "图标无效"));
    }
    Ok(icon.to_string())
}

#[derive(Deserialize)]
pub struct Change {
    pub name: Option<String>,
    pub icon: Option<String>,
}

/// A new base holding `doc`'s tables (by default one empty table).
pub(crate) fn create_base(store: &mut Store, mut doc: Base, name: &str, icon: &str) -> Result<Entry> {
    doc.name = check_name(name)?;
    doc.icon = check_icon(icon)?;
    let id = format!("b{}", &uuid::Uuid::new_v4().simple().to_string()[..15]);
    let tx = store.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    insert_full(&tx, &id, 1, &doc)?;
    add(&tx, &id)?;
    tx.commit()?;
    store.cache.insert(id.clone(), (1, doc));
    store.changed.send_modify(|n| *n += 1);
    entry(&store.conn, &id)
}

pub async fn create(State(state): State<App>, Json(req): Json<Change>) -> Result<Json<Entry>> {
    db(state, move |store| create_base(store, rutable_model::sample::empty(), req.name.as_deref().unwrap_or("未命名多维表格"), req.icon.as_deref().unwrap_or(""))).await.map(Json)
}

/// Renames a base or changes its icon: an ordinary change of the document.
pub async fn rename(State(state): State<App>, Path(id): Path<String>, Json(req): Json<Change>) -> Result<Json<Entry>> {
    valid_id(&id)?;
    db(state, move |store| {
        let doc = store.load(&id)?.1.clone();
        let mut next = doc.clone();
        if let Some(n) = &req.name {
            next.name = check_name(n)?;
        }
        if let Some(i) = &req.icon {
            next.icon = check_icon(i)?;
        }
        store.commit(&id, &sync::diff(&doc, &next))?;
        entry(&store.conn, &id)
    })
    .await
    .map(Json)
}

/// Moves a base to the trash; nothing is erased.
pub async fn remove(State(state): State<App>, Path(id): Path<String>) -> Result<StatusCode> {
    valid_id(&id)?;
    db(state, move |store| {
        let n = store.conn.execute("UPDATE workspace SET deleted_at=?2 WHERE id=?1 AND deleted_at IS NULL", params![id, rutable_model::now_ms()])?;
        if n == 0 {
            return Err(error(StatusCode::NOT_FOUND, "document_not_found"));
        }
        store.cache.remove(&id);
        store.changed.send_modify(|n| *n += 1);
        Ok(StatusCode::NO_CONTENT)
    })
    .await
}

pub async fn restore(State(state): State<App>, Path(id): Path<String>) -> Result<Json<Entry>> {
    valid_id(&id)?;
    db(state, move |store| {
        // Back at the end of the list.
        let n = store.conn.execute(
            "UPDATE workspace SET deleted_at=NULL, pos=(SELECT coalesce(max(pos),-1)+1 FROM workspace) WHERE id=?1 AND deleted_at IS NOT NULL",
            [&id],
        )?;
        if n == 0 {
            return Err(error(StatusCode::NOT_FOUND, "document_not_found"));
        }
        store.changed.send_modify(|n| *n += 1);
        entry(&store.conn, &id)
    })
    .await
    .map(Json)
}

#[derive(Deserialize)]
pub struct Order {
    pub ids: Vec<String>,
}

pub async fn reorder(State(state): State<App>, Json(req): Json<Order>) -> Result<Json<Vec<Entry>>> {
    db(state, move |store| {
        let tx = store.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        for (i, id) in req.ids.iter().enumerate() {
            tx.execute("UPDATE workspace SET pos=?2 WHERE id=?1", params![id, i as i64])?;
        }
        tx.commit()?;
        store.changed.send_modify(|n| *n += 1);
        entries(&store.conn, false)
    })
    .await
    .map(Json)
}

#[derive(Deserialize)]
pub struct Wait {
    pub since: u64,
    /// Seconds, at most 30.
    pub timeout: Option<u64>,
}

/// Answers once the document is past `since` (or after the timeout):
/// open windows learn of a change in about a second, without polling.
pub async fn wait(State(state): State<App>, Path(id): Path<String>, Query(q): Query<Wait>) -> Result<Json<serde_json::Value>> {
    valid_id(&id)?;
    let mut rx = state.changed.subscribe();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(q.timeout.unwrap_or(25).min(30));
    loop {
        rx.borrow_and_update();
        let id2 = id.clone();
        let rev = db(state.clone(), move |store| revision(&store.conn, &id2)?.ok_or_else(|| error(StatusCode::NOT_FOUND, "document_not_found"))).await?;
        if rev != q.since {
            return Ok(Json(serde_json::json!({"revision": rev, "changed": true})));
        }
        if tokio::time::timeout_at(deadline, rx.changed()).await.is_err() {
            return Ok(Json(serde_json::json!({"revision": rev, "changed": false})));
        }
    }
}
