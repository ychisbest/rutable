use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use rutable_model::sync::{self, Delta, Patch, RecordState};
use rutable_model::{Base, Value};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};
use tower_http::{compression::CompressionLayer, decompression::RequestDecompressionLayer};

mod auth;
mod mcp;
mod web;
mod workspace;

#[derive(Clone)]
pub struct App {
    db: Arc<Mutex<Store>>,
    /// The server key (RUTABLE_API_TOKEN): always accepted, and needed once to set the password.
    token: Arc<String>,
    /// Bumps on every saved change of any document (for waiting clients).
    changed: Arc<tokio::sync::watch::Sender<u64>>,
    logins: Arc<Mutex<auth::Limiter>>,
    /// Where the downloadable client lives (RUTABLE_DIST).
    dist: Option<Arc<std::path::PathBuf>>,
}
/// SQLite plus the documents in memory, so a save touches only its items.
pub struct Store {
    pub conn: Connection,
    cache: HashMap<String, (u64, Base)>,
    changed: Arc<tokio::sync::watch::Sender<u64>>,
}
#[derive(Debug)]
pub struct Error(pub StatusCode, pub String);
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({"error":self.1}))).into_response()
    }
}
impl From<rusqlite::Error> for Error {
    fn from(_: rusqlite::Error) -> Self {
        Self(StatusCode::INTERNAL_SERVER_ERROR, "数据库操作失败".into())
    }
}
impl From<serde_json::Error> for Error {
    fn from(_: serde_json::Error) -> Self {
        Self(StatusCode::BAD_REQUEST, "文档格式错误".into())
    }
}
pub(crate) type Result<T> = std::result::Result<T, Error>;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub revision: u64,
    pub document: Base,
}
#[derive(Serialize, Deserialize)]
pub struct WriteDocument {
    pub expected_revision: u64,
    pub document: Base,
}
#[derive(Serialize, Deserialize)]
pub struct WritePatch {
    pub base_revision: u64,
    pub patch: Patch,
}
#[derive(Serialize, Deserialize)]
pub struct Attachment {
    pub name: String,
    pub path: String,
    pub size: u64,
}
#[derive(Deserialize)]
struct Upload {
    name: String,
}
#[derive(Deserialize)]
struct Link {
    token: String,
}
#[derive(Deserialize)]
struct Since {
    since: u64,
}
pub(crate) fn error(status: StatusCode, message: impl Into<String>) -> Error {
    Error(status, message.into())
}
const SCHEMA: &str = "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;
  CREATE TABLE IF NOT EXISTS attachments(id TEXT PRIMARY KEY, document_id TEXT NOT NULL, name TEXT NOT NULL, mime TEXT NOT NULL, body BLOB NOT NULL, created_at INTEGER NOT NULL);
  CREATE INDEX IF NOT EXISTS attachments_document ON attachments(document_id);
  CREATE TABLE IF NOT EXISTS attachment_links(token TEXT PRIMARY KEY,attachment_id TEXT NOT NULL,expires_at INTEGER NOT NULL);
  CREATE TABLE IF NOT EXISTS doc_meta(id TEXT PRIMARY KEY, revision INTEGER NOT NULL, meta TEXT NOT NULL, meta_rev INTEGER NOT NULL, updated_at INTEGER NOT NULL);
  CREATE TABLE IF NOT EXISTS doc_records(document_id TEXT NOT NULL, table_id TEXT NOT NULL, id TEXT NOT NULL, pos INTEGER NOT NULL, rev INTEGER NOT NULL, body TEXT, PRIMARY KEY(document_id,table_id,id));
  CREATE INDEX IF NOT EXISTS doc_records_rev ON doc_records(document_id,rev);
  CREATE TABLE IF NOT EXISTS doc_orders(document_id TEXT NOT NULL, table_id TEXT NOT NULL, rev INTEGER NOT NULL, PRIMARY KEY(document_id,table_id));
  CREATE TABLE IF NOT EXISTS doc_log(document_id TEXT NOT NULL, revision INTEGER NOT NULL, patch TEXT NOT NULL, saved_at INTEGER NOT NULL, PRIMARY KEY(document_id,revision));
  CREATE TABLE IF NOT EXISTS workspace(id TEXT PRIMARY KEY, pos INTEGER NOT NULL, created_at INTEGER NOT NULL, deleted_at INTEGER);
  CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
  CREATE TABLE IF NOT EXISTS credentials(id TEXT PRIMARY KEY, hash TEXT NOT NULL UNIQUE, kind TEXT NOT NULL, name TEXT NOT NULL, created_at INTEGER NOT NULL, last_used INTEGER NOT NULL, expires_at INTEGER);";
impl App {
    pub fn open(path: impl AsRef<std::path::Path>, token: String) -> Result<Self> {
        let db = Connection::open(path)?;
        Self::from_connection(db, token)
    }
    pub fn from_connection(db: Connection, token: String) -> Result<Self> {
        if token.len() < 32 {
            return Err(error(StatusCode::INTERNAL_SERVER_ERROR, "RUTABLE_API_TOKEN 至少需要 32 个字符"));
        }
        db.busy_timeout(Duration::from_secs(5))?;
        db.execute_batch(SCHEMA)?;
        let changed = Arc::new(tokio::sync::watch::channel(0u64).0);
        let mut store = Store { conn: db, cache: HashMap::new(), changed: changed.clone() };
        migrate(&mut store)?;
        workspace::adopt(&store.conn)?;
        let dist = std::env::var_os("RUTABLE_DIST").map(|d| Arc::new(std::path::PathBuf::from(d)));
        Ok(Self { db: Arc::new(Mutex::new(store)), token: Arc::new(token), changed, logins: Arc::new(Mutex::new(auth::Limiter::default())), dist })
    }
    pub fn import(&self, id: &str, document: Base) -> Result<()> {
        valid_id(id)?;
        validate(&document)?;
        let mut store = self.db.lock().map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "数据库锁异常"))?;
        if revision(&store.conn, id)?.is_some() {
            return Err(error(StatusCode::CONFLICT, "目标文档已存在，导入不会覆盖现有数据"));
        }
        let mut files = vec![];
        for t in &document.tables {
            for r in &t.records {
                for v in r.cells.values() {
                    attachment_ids(v, &mut files)?
                }
            }
        }
        let tx = store.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        check_attachments(&tx, id, files)?;
        insert_full(&tx, id, 1, &document)?;
        workspace::add(&tx, id)?;
        tx.commit()?;
        store.cache.insert(id.into(), (1, document));
        Ok(())
    }
}
/// Version 1 kept each document as one JSON body; version 2 keeps a row per
/// record. The old tables stay untouched as a fallback.
fn migrate(store: &mut Store) -> Result<()> {
    let version: i64 = store.conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 2 {
        return Ok(());
    }
    let old: bool = store.conn.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name='documents'", [], |r| r.get::<_, i64>(0))? > 0;
    let tx = store.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if old {
        let rows: Vec<(String, i64, String)> = {
            let mut q = tx.prepare("SELECT id,revision,body FROM documents")?;
            q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<std::result::Result<_, _>>()?
        };
        for (id, rev, body) in rows {
            if tx.query_row("SELECT 1 FROM doc_meta WHERE id=?1", [&id], |r| r.get::<_, i64>(0)).optional()?.is_some() {
                continue;
            }
            let doc: Base = serde_json::from_str(&body).map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "旧文档无法迁移"))?;
            insert_full(&tx, &id, rev as u64, &doc)?;
        }
    }
    tx.execute_batch("PRAGMA user_version=2")?;
    tx.commit()?;
    Ok(())
}
pub(crate) fn insert_full(tx: &Connection, id: &str, rev: u64, doc: &Base) -> Result<()> {
    let now = rutable_model::now_ms();
    tx.execute(
        "INSERT INTO doc_meta(id,revision,meta,meta_rev,updated_at) VALUES (?1,?2,?3,?2,?4)",
        params![id, rev as i64, serde_json::to_string(&sync::meta(doc))?, now],
    )?;
    let mut q = tx.prepare("INSERT INTO doc_records(document_id,table_id,id,pos,rev,body) VALUES (?1,?2,?3,?4,?5,?6)")?;
    for t in &doc.tables {
        for (i, r) in t.records.iter().enumerate() {
            q.execute(params![id, t.id, r.id, i as i64, rev as i64, serde_json::to_string(r)?])?;
        }
    }
    Ok(())
}
/// The document's revision; `None` if it does not exist or is in the trash.
pub(crate) fn revision(conn: &Connection, id: &str) -> Result<Option<u64>> {
    Ok(conn
        .query_row(
            "SELECT m.revision FROM doc_meta m LEFT JOIN workspace w ON w.id=m.id WHERE m.id=?1 AND w.deleted_at IS NULL",
            [id],
            |r| r.get::<_, i64>(0).map(|v| v as u64),
        )
        .optional()?)
}
impl Store {
    /// The document at its current revision.
    pub(crate) fn load(&mut self, id: &str) -> Result<&mut (u64, Base)> {
        let rev = revision(&self.conn, id)?.ok_or_else(|| error(StatusCode::NOT_FOUND, "document_not_found"))?;
        if self.cache.get(id).is_none_or(|c| c.0 != rev) {
            let meta: String = self.conn.query_row("SELECT meta FROM doc_meta WHERE id=?1", [id], |r| r.get(0))?;
            let mut doc: Base = serde_json::from_str(&meta)?;
            let mut q = self.conn.prepare("SELECT table_id,body FROM doc_records WHERE document_id=?1 AND body IS NOT NULL ORDER BY pos")?;
            let rows = q.query_map([id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            let mut by_table: HashMap<String, Vec<Arc<rutable_model::Record>>> = HashMap::new();
            for row in rows {
                let (t, body) = row?;
                by_table.entry(t).or_default().push(Arc::new(serde_json::from_str(&body)?));
            }
            for t in &mut doc.tables {
                t.records = by_table.remove(&t.id).unwrap_or_default();
            }
            self.cache.insert(id.into(), (rev, doc));
        }
        Ok(self.cache.get_mut(id).unwrap())
    }
    pub(crate) fn snapshot(&mut self, id: &str) -> Result<Snapshot> {
        let (revision, document) = self.load(id)?.clone();
        Ok(Snapshot { revision, document })
    }
    /// The items changed after `since`.
    fn delta(&mut self, id: &str, since: u64) -> Result<Delta> {
        let (rev, doc) = self.load(id)?.clone();
        let conn = &self.conn;
        let meta_rev: i64 = conn.query_row("SELECT meta_rev FROM doc_meta WHERE id=?1", [id], |r| r.get(0))?;
        let mut delta = Delta { revision: rev, ..Default::default() };
        if meta_rev as u64 > since {
            delta.meta = Some(sync::meta(&doc));
        }
        let mut q = conn.prepare("SELECT table_id,id,body FROM doc_records WHERE document_id=?1 AND rev>?2 ORDER BY pos")?;
        let rows = q.query_map(params![id, since as i64], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?)))?;
        for row in rows {
            let (table, rid, body) = row?;
            let record = match body {
                Some(b) => Some(Arc::new(serde_json::from_str(&b)?)),
                None => None,
            };
            delta.records.push(RecordState { table, id: rid, record });
        }
        let mut q = conn.prepare("SELECT table_id FROM doc_orders WHERE document_id=?1 AND rev>?2")?;
        let tables: Vec<String> = q.query_map(params![id, since as i64], |r| r.get(0))?.collect::<std::result::Result<_, _>>()?;
        for t in tables {
            if let Some(t) = doc.table(&t) {
                delta.orders.push((t.id.clone(), t.records.iter().map(|r| r.id.clone()).collect()));
            }
        }
        delta.counts = doc.tables.iter().map(|t| (t.id.clone(), t.records.len())).collect();
        Ok(delta)
    }
    /// Applies `patch` (made against `base_revision`, merged item by item
    /// with what changed since) and stores only the items it changed.
    pub(crate) fn commit(&mut self, id: &str, patch: &Patch) -> Result<u64> {
        let (current, doc) = self.load(id)?.clone();
        let mut next = doc;
        let touched = sync::apply(&mut next, patch).map_err(|e| error(StatusCode::CONFLICT, e))?;
        if !touched.meta && touched.records.is_empty() && touched.orders.is_empty() {
            return Ok(current);
        }
        validate(&next)?;
        let mut files = vec![];
        for (t, r) in &touched.records {
            if let Some(r) = next.table(t).and_then(|t| t.record(r)) {
                for v in r.cells.values() {
                    attachment_ids(v, &mut files)?
                }
            }
        }
        let rev = current + 1;
        let now = rutable_model::now_ms();
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        check_attachments(&tx, id, files)?;
        if revision(&tx, id)? != Some(current) {
            return Err(error(StatusCode::CONFLICT, "revision_conflict"));
        }
        if touched.meta {
            tx.execute("UPDATE doc_meta SET meta=?2, meta_rev=?3 WHERE id=?1", params![id, serde_json::to_string(&sync::meta(&next))?, rev as i64])?;
            // Records of deleted tables go with them.
            let tables: Vec<String> = next.tables.iter().map(|t| t.id.clone()).collect();
            let list = serde_json::to_string(&tables)?;
            tx.execute("DELETE FROM doc_records WHERE document_id=?1 AND table_id NOT IN (SELECT value FROM json_each(?2))", params![id, list])?;
            tx.execute("DELETE FROM doc_orders WHERE document_id=?1 AND table_id NOT IN (SELECT value FROM json_each(?2))", params![id, list])?;
        }
        for tid in &touched.orders {
            let t = next.table(tid).unwrap();
            let mut q = tx.prepare_cached("UPDATE doc_records SET pos=?4 WHERE document_id=?1 AND table_id=?2 AND id=?3")?;
            for (i, r) in t.records.iter().enumerate() {
                q.execute(params![id, tid, r.id, i as i64])?;
            }
            tx.execute(
                "INSERT INTO doc_orders(document_id,table_id,rev) VALUES (?1,?2,?3) ON CONFLICT(document_id,table_id) DO UPDATE SET rev=excluded.rev",
                params![id, tid, rev as i64],
            )?;
        }
        for (tid, rid) in &touched.records {
            match next.table(tid).and_then(|t| t.record(rid)) {
                Some(r) => {
                    let body = serde_json::to_string(r)?;
                    let updated = tx.execute(
                        "UPDATE doc_records SET body=?4, rev=?5 WHERE document_id=?1 AND table_id=?2 AND id=?3",
                        params![id, tid, rid, body, rev as i64],
                    )?;
                    if updated == 0 || touched.orders.contains(tid) {
                        // New rows go after every existing one (unless the table was just reordered).
                        let pos: i64 = if touched.orders.contains(tid) {
                            next.table(tid).unwrap().records.iter().position(|x| &x.id == rid).unwrap() as i64
                        } else {
                            tx.query_row("SELECT coalesce(max(pos),-1)+1 FROM doc_records WHERE document_id=?1 AND table_id=?2", params![id, tid], |r| r.get(0))?
                        };
                        tx.execute(
                            "INSERT INTO doc_records(document_id,table_id,id,pos,rev,body) VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(document_id,table_id,id) DO UPDATE SET pos=excluded.pos, rev=excluded.rev, body=excluded.body",
                            params![id, tid, rid, pos, rev as i64, body],
                        )?;
                    }
                }
                None => {
                    tx.execute("UPDATE doc_records SET body=NULL, rev=?4 WHERE document_id=?1 AND table_id=?2 AND id=?3", params![id, tid, rid, rev as i64])?;
                }
            }
        }
        tx.execute("UPDATE doc_meta SET revision=?2, updated_at=?3 WHERE id=?1", params![id, rev as i64, now])?;
        tx.execute("INSERT INTO doc_log(document_id,revision,patch,saved_at) VALUES (?1,?2,?3,?4)", params![id, rev as i64, serde_json::to_string(patch)?, now])?;
        tx.execute("DELETE FROM doc_log WHERE document_id=?1 AND revision<=?2", params![id, rev.saturating_sub(100) as i64])?;
        tx.commit()?;
        self.cache.insert(id.into(), (rev, next));
        self.changed.send_modify(|n| *n += 1);
        Ok(rev)
    }
}
pub(crate) fn check_attachments(tx: &Connection, id: &str, files: Vec<String>) -> Result<()> {
    for file in files {
        if tx.query_row("SELECT 1 FROM attachments WHERE id=?1 AND document_id=?2", params![file, id], |r| r.get::<_, i64>(0)).optional()?.is_none() {
            return Err(error(StatusCode::BAD_REQUEST, "引用的服务器附件不存在"));
        }
    }
    Ok(())
}
pub(crate) fn valid_id(id: &str) -> Result<()> {
    if id.is_empty() || id.len() > 80 || !id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')) {
        return Err(error(StatusCode::BAD_REQUEST, "文档 ID 无效"));
    }
    Ok(())
}
pub(crate) fn validate(b: &Base) -> Result<()> {
    if b.tables.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "文档至少需要一个数据表"));
    }
    let mut tables = HashSet::new();
    for t in &b.tables {
        if t.id.is_empty() || !tables.insert(&t.id) || t.fields.is_empty() || t.views.is_empty() {
            return Err(error(StatusCode::BAD_REQUEST, "数据表 ID、字段或视图无效"));
        }
        let mut fields = HashSet::new();
        let mut records = HashSet::new();
        let mut views = HashSet::new();
        for f in &t.fields {
            if f.id.is_empty() || !fields.insert(&f.id) {
                return Err(error(StatusCode::BAD_REQUEST, "字段 ID 重复或为空"));
            }
        }
        for r in &t.records {
            if r.id.is_empty() || !records.insert(&r.id) {
                return Err(error(StatusCode::BAD_REQUEST, "记录 ID 重复或为空"));
            }
        }
        for v in &t.views {
            if v.id.is_empty() || !views.insert(&v.id) {
                return Err(error(StatusCode::BAD_REQUEST, "视图 ID 重复或为空"));
            }
        }
    }
    Ok(())
}
pub(crate) fn attachment_ids(v: &Value, ids: &mut Vec<String>) -> Result<()> {
    match v {
        Value::Files(files) => {
            for file in files {
                let id = file
                    .path
                    .strip_prefix("rutable://attachment/")
                    .filter(|id| uuid::Uuid::parse_str(id).is_ok())
                    .ok_or_else(|| error(StatusCode::BAD_REQUEST, "附件必须先上传到服务器，不能保存客户端文件路径"))?;
                ids.push(id.into());
            }
        }
        Value::List(values) => {
            for v in values {
                attachment_ids(v, ids)?
            }
        }
        _ => {}
    }
    Ok(())
}
pub(crate) async fn db<T: Send + 'static>(state: App, job: impl FnOnce(&mut Store) -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(move || {
        let mut store = state.db.lock().map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "数据库锁异常"))?;
        job(&mut store)
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "数据库线程中断"))?
}
pub fn router(state: App) -> Router {
    let api = Router::new()
        .route("/documents", get(workspace::list).post(workspace::create))
        .route("/trash", get(workspace::trash))
        .route("/workspace/order", axum::routing::put(workspace::reorder))
        .route("/documents/{id}", get(read_document).put(save_document).patch(workspace::rename).delete(workspace::remove))
        .route("/documents/{id}/restore", post(workspace::restore))
        .route("/documents/{id}/wait", get(workspace::wait))
        .route("/documents/{id}/changes", get(read_changes).post(save_changes))
        .route("/auth/tokens", get(auth::list_tokens).post(auth::create_token))
        .route("/auth/tokens/{token}", axum::routing::delete(auth::delete_token))
        .route("/auth/password", post(auth::change_password))
        .route("/auth/logout", post(auth::logout))
        .route("/documents/{id}/attachments", post(upload))
        .route("/documents/{id}/attachments/{file}", get(download))
        .route("/documents/{id}/attachments/{file}/link", post(create_link))
        .layer(middleware::from_fn_with_state(state.clone(), auth::require));
    let public = Router::new()
        .route("/auth/status", get(auth::status))
        .route("/auth/setup", post(auth::setup))
        .route("/auth/login", post(auth::login));
    let mcp = Router::new().route("/mcp", post(mcp::handle).get(mcp::not_streaming)).layer(middleware::from_fn_with_state(state.clone(), auth::require));
    Router::new()
        .route("/healthz", get(|| async { Json(serde_json::json!({"status":"ok","storage":"sqlite"})) }))
        .route("/", get(web::home))
        .route("/download/rutable.exe", get(web::download))
        .nest("/api/v1", api.merge(public))
        .merge(mcp)
        .route("/attachments/{file}/view", get(view_attachment))
        .route("/attachments/{file}/content", get(linked_content))
        .layer(DefaultBodyLimit::max(64 * 1024 * 1024))
        .layer(RequestDecompressionLayer::new())
        // Brotli's own default (11) takes seconds on a large document; 5 is fast and small.
        .layer(CompressionLayer::new().quality(tower_http::CompressionLevel::Precise(5)))
        .layer(middleware::from_fn(|request: axum::extract::Request, next: Next| async move {
            let mut response = next.run(request).await;
            response.headers_mut().insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
            response.headers_mut().insert("X-Content-Type-Options", "nosniff".parse().unwrap());
            response
        }))
        .with_state(state)
}
fn not_modified(headers: &HeaderMap, rev: u64) -> Option<Response> {
    let etag = format!("\"{rev}\"");
    (headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) == Some(etag.as_str()))
        .then(|| (StatusCode::NOT_MODIFIED, [(header::ETAG, etag)]).into_response())
}
async fn read_document(State(state): State<App>, Path(id): Path<String>, headers: HeaderMap) -> Result<Response> {
    valid_id(&id)?;
    db(state, move |store| {
        let rev = revision(&store.conn, &id)?.ok_or_else(|| error(StatusCode::NOT_FOUND, "document_not_found"))?;
        if let Some(r) = not_modified(&headers, rev) {
            return Ok(r);
        }
        let snapshot = store.snapshot(&id)?;
        Ok(([(header::ETAG, format!("\"{}\"", snapshot.revision))], Json(snapshot)).into_response())
    })
    .await
}
/// Whole-document save (older clients, imports): stored as the changes from the current version.
async fn save_document(State(state): State<App>, Path(id): Path<String>, Json(request): Json<WriteDocument>) -> Result<Json<Snapshot>> {
    valid_id(&id)?;
    db(state, move |store| {
        validate(&request.document)?;
        let current = revision(&store.conn, &id)?;
        let Some(current) = current else {
            // Revision 0 creates the document.
            if request.expected_revision != 0 {
                return Err(error(StatusCode::CONFLICT, "revision_conflict"));
            }
            let mut files = vec![];
            for t in &request.document.tables {
                for r in &t.records {
                    for v in r.cells.values() {
                        attachment_ids(v, &mut files)?
                    }
                }
            }
            let tx = store.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            check_attachments(&tx, &id, files)?;
            insert_full(&tx, &id, 1, &request.document)?;
            workspace::add(&tx, &id)?;
            tx.commit()?;
            return store.snapshot(&id);
        };
        if request.expected_revision != current {
            return Err(error(StatusCode::CONFLICT, "revision_conflict"));
        }
        let patch = sync::diff(&store.load(&id)?.1, &request.document);
        store.commit(&id, &patch)?;
        store.snapshot(&id)
    })
    .await
    .map(Json)
}
async fn read_changes(State(state): State<App>, Path(id): Path<String>, Query(q): Query<Since>, headers: HeaderMap) -> Result<Response> {
    valid_id(&id)?;
    db(state, move |store| {
        let rev = revision(&store.conn, &id)?.ok_or_else(|| error(StatusCode::NOT_FOUND, "document_not_found"))?;
        if q.since > rev {
            return Err(error(StatusCode::CONFLICT, "revision_ahead"));
        }
        if let Some(r) = not_modified(&headers, rev) {
            return Ok(r);
        }
        let delta = store.delta(&id, q.since)?;
        Ok(([(header::ETAG, format!("\"{}\"", delta.revision))], Json(delta)).into_response())
    })
    .await
}
/// Saves a patch; answers with everything changed since its base revision.
async fn save_changes(State(state): State<App>, Path(id): Path<String>, Json(request): Json<WritePatch>) -> Result<Json<Delta>> {
    valid_id(&id)?;
    db(state, move |store| {
        let current = revision(&store.conn, &id)?.ok_or_else(|| error(StatusCode::NOT_FOUND, "document_not_found"))?;
        if request.base_revision > current {
            return Err(error(StatusCode::CONFLICT, "revision_ahead"));
        }
        store.commit(&id, &request.patch)?;
        store.delta(&id, request.base_revision)
    })
    .await
    .map(Json)
}
fn mime(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "txt" | "md" | "csv" | "json" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
async fn upload(State(state): State<App>, Path(id): Path<String>, Query(query): Query<Upload>, body: Bytes) -> Result<Json<Attachment>> {
    valid_id(&id)?;
    if query.name.is_empty() || query.name.len() > 512 || query.name.chars().any(char::is_control) {
        return Err(error(StatusCode::BAD_REQUEST, "文件名无效"));
    }
    let file = uuid::Uuid::new_v4().to_string();
    let result = Attachment { path: format!("rutable://attachment/{file}"), name: query.name.clone(), size: body.len() as u64 };
    db(state, move |store| {
        let db = &store.conn;
        if revision(db, &id)?.is_none() {
            return Err(error(StatusCode::NOT_FOUND, "document_not_found"));
        }
        db.execute(
            "INSERT INTO attachments(id,document_id,name,mime,body,created_at) VALUES (?1,?2,?3,?4,?5,?6)",
            params![file, id, query.name, mime(&query.name), body.as_ref(), rutable_model::now_ms()],
        )?;
        Ok(())
    })
    .await?;
    Ok(Json(result))
}
async fn download(State(state): State<App>, Path((id, file)): Path<(String, String)>) -> Result<Response> {
    db(state, move |store| {
        let db = &store.conn;
        let (mime, body): (String, Vec<u8>) = db
            .query_row("SELECT mime,body FROM attachments WHERE id=?1 AND document_id=?2", params![file, id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "附件不存在"))?;
        Ok(([(header::CONTENT_TYPE, mime), (header::CONTENT_DISPOSITION, "attachment".into())], body).into_response())
    })
    .await
}
async fn create_link(State(state): State<App>, Path((id, file)): Path<(String, String)>) -> Result<Json<serde_json::Value>> {
    db(state, move |store| {
        let db = &store.conn;
        if db
            .query_row("SELECT 1 FROM attachments WHERE id=?1 AND document_id=?2", params![file, id], |r| r.get::<_, i64>(0))
            .optional()?
            .is_none()
        {
            return Err(error(StatusCode::NOT_FOUND, "附件不存在"));
        }
        let token = uuid::Uuid::new_v4().simple().to_string();
        let expires = rutable_model::now_ms() + 300_000;
        db.execute("DELETE FROM attachment_links WHERE expires_at<?1", [rutable_model::now_ms()])?;
        db.execute("INSERT INTO attachment_links(token,attachment_id,expires_at) VALUES (?1,?2,?3)", params![token, file, expires])?;
        Ok(Json(serde_json::json!({"path":format!("/attachments/{file}/view?token={token}"),"expires_at":expires})))
    })
    .await
}
pub(crate) fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}
fn linked(db: &Connection, file: &str, token: &str) -> Result<(String, String, Vec<u8>)> {
    db.query_row("SELECT a.name,a.mime,a.body FROM attachments a JOIN attachment_links l ON l.attachment_id=a.id WHERE a.id=?1 AND l.token=?2 AND l.expires_at>?3",params![file,token,rutable_model::now_ms()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?.ok_or_else(||error(StatusCode::NOT_FOUND,"附件链接无效或已过期"))
}
async fn view_attachment(State(state): State<App>, Path(file): Path<String>, Query(link): Query<Link>) -> Result<Html<String>> {
    db(state,move |store| {
  let (name,mime,body)=linked(&store.conn,&file,&link.token)?;
  let content=format!("/attachments/{}/content?token={}",escape(&file),escape(&link.token));
  let preview=if mime.starts_with("text/") {format!("<pre>{}</pre>",escape(&String::from_utf8_lossy(&body)))}
    else if mime.starts_with("image/") {format!("<img alt=\"附件\" src=\"{content}\" style=\"max-width:100%\">")}
    else if mime=="application/pdf" {format!("<iframe title=\"PDF 预览\" src=\"{content}\" style=\"width:100%;height:80vh;border:0\"></iframe>")}
    else {"<p>此文件不能在线预览，可手动下载。</p>".into()};
  Ok(Html(format!("<!doctype html><html lang=zh><meta charset=utf-8><meta name=referrer content=no-referrer><title>{}</title><style>body{{font:16px system-ui;max-width:1100px;margin:32px auto;padding:0 20px}}pre{{white-space:pre-wrap;word-break:break-word}}</style><h1>{}</h1><p>附件保存在服务器；此链接五分钟后失效。</p><p><a download href=\"{content}&download=1\">下载文件</a></p>{preview}</html>",escape(&name),escape(&name))))
 }).await
}
#[derive(Deserialize)]
struct ContentLink {
    token: String,
    download: Option<u8>,
}
async fn linked_content(State(state): State<App>, Path(file): Path<String>, Query(link): Query<ContentLink>) -> Result<Response> {
    db(state, move |store| {
        let (_, mime, body) = linked(&store.conn, &file, &link.token)?;
        Ok((
            [
                (header::CONTENT_TYPE, mime),
                (header::CONTENT_DISPOSITION, if link.download == Some(1) { "attachment" } else { "inline" }.into()),
                (header::CONTENT_SECURITY_POLICY, "sandbox".into()),
            ],
            body,
        )
            .into_response())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    const TOKEN: &str = "test-key-with-at-least-thirty-two-characters";
    fn app() -> App {
        App::from_connection(Connection::open_in_memory().unwrap(), TOKEN.into()).unwrap()
    }
    fn request(method: &str, path: &str, body: serde_json::Value) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }
    #[tokio::test]
    async fn private_data_requires_authentication() {
        let state = app();
        state.import("default", rutable_model::sample::empty()).unwrap();
        let response =
            router(state).oneshot(Request::builder().uri("/api/v1/documents/default").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    #[tokio::test]
    async fn stale_revision_cannot_overwrite_saved_data_and_etag_avoids_download() {
        let state = app();
        state.import("default", rutable_model::sample::empty()).unwrap();
        let api = router(state);
        let response = api.clone().oneshot(request("GET", "/api/v1/documents/default", serde_json::Value::Null)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let mut snap: Snapshot = serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
        snap.document.name = "Saved".into();
        let body = serde_json::to_value(WriteDocument { expected_revision: 1, document: snap.document }).unwrap();
        assert_eq!(api.clone().oneshot(request("PUT", "/api/v1/documents/default", body.clone())).await.unwrap().status(), StatusCode::OK);
        assert_eq!(api.clone().oneshot(request("PUT", "/api/v1/documents/default", body)).await.unwrap().status(), StatusCode::CONFLICT);
        let mut req = request("GET", "/api/v1/documents/default", serde_json::Value::Null);
        req.headers_mut().insert(header::IF_NONE_MATCH, "\"2\"".parse().unwrap());
        assert_eq!(api.oneshot(req).await.unwrap().status(), StatusCode::NOT_MODIFIED);
    }
    #[tokio::test]
    async fn uploaded_attachments_are_sqlite_blobs_not_client_paths() {
        let state = app();
        state.import("default", rutable_model::sample::empty()).unwrap();
        let api = router(state);
        let req = Request::builder()
            .method("POST")
            .uri("/api/v1/documents/default/attachments?name=notes.txt")
            .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
            .body(Body::from("hello sqlite"))
            .unwrap();
        let response = api.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let a: Attachment = serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
        let id = a.path.strip_prefix("rutable://attachment/").unwrap();
        let response =
            api.oneshot(request("GET", &format!("/api/v1/documents/default/attachments/{id}"), serde_json::Value::Null)).await.unwrap();
        assert_eq!(to_bytes(response.into_body(), usize::MAX).await.unwrap(), "hello sqlite");
    }
    #[test]
    fn sqlite_survives_close_and_reopen() {
        let path = std::env::temp_dir().join(format!("rutable-server-{}.sqlite3", uuid::Uuid::new_v4()));
        let state = App::open(&path, TOKEN.into()).unwrap();
        let mut base = rutable_model::sample::empty();
        base.name = "Persisted".into();
        state.import("default", base).unwrap();
        drop(state);
        let state = App::open(&path, TOKEN.into()).unwrap();
        let mut store = state.db.lock().unwrap();
        store.cache.clear();
        assert_eq!(store.snapshot("default").unwrap().document.name, "Persisted");
        drop(store);
        drop(state);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn rejects_unuploaded_local_attachment_paths() {
        let state = app();
        let mut base = rutable_model::sample::empty();
        let t = &mut base.tables[0];
        let mut r = t.new_record("");
        r.cells.insert(
            t.fields[0].id.clone(),
            Value::Files(vec![rutable_model::Attachment { name: "secret.txt".into(), path: r"C:\private\secret.txt".into(), size: 1 }]),
        );
        t.push_record(r);
        assert_eq!(state.import("default", base).unwrap_err().0, StatusCode::BAD_REQUEST);
    }

    fn records(n: usize) -> Base {
        let mut base = rutable_model::sample::empty();
        let me = base.me.clone();
        let t = &mut base.tables[0];
        t.records.clear();
        for _ in 0..n {
            let r = t.new_record(&me);
            t.push_record(r);
        }
        base
    }
    fn set(b: &mut Base, i: usize, text: &str) {
        let f = b.tables[0].fields[0].id.clone();
        Arc::make_mut(&mut b.tables[0].records[i]).cells.insert(f, Value::Text(text.into()));
    }
    async fn call<T: serde::de::DeserializeOwned>(api: &Router, method: &str, path: &str, body: serde_json::Value) -> (StatusCode, Option<T>) {
        let response = api.clone().oneshot(request(method, path, body)).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).ok())
    }
    #[tokio::test]
    async fn patches_store_only_changed_records_and_merge_concurrent_windows() {
        let state = app();
        state.import("default", records(50)).unwrap();
        let api = router(state.clone());
        let (_, snap) = call::<Snapshot>(&api, "GET", "/api/v1/documents/default", serde_json::Value::Null).await;
        let snap = snap.unwrap();
        // Two windows edit different records from the same revision.
        let mut a = snap.document.clone();
        set(&mut a, 3, "a");
        let mut b = snap.document.clone();
        set(&mut b, 7, "b");
        let pa = sync::diff(&snap.document, &a);
        assert_eq!(pa.records.len(), 1);
        let body = |p: &Patch| serde_json::to_value(WritePatch { base_revision: snap.revision, patch: p.clone() }).unwrap();
        let (st, da) = call::<Delta>(&api, "POST", "/api/v1/documents/default/changes", body(&pa)).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(da.unwrap().records.len(), 1);
        let (st, db) = call::<Delta>(&api, "POST", "/api/v1/documents/default/changes", body(&sync::diff(&snap.document, &b))).await;
        assert_eq!(st, StatusCode::OK);
        let db = db.unwrap();
        // B learns A's record too, and nothing else.
        assert_eq!(db.revision, snap.revision + 2);
        assert_eq!(db.records.len(), 2);
        let mut b2 = snap.document.clone();
        sync::apply_delta(&mut b2, &db).unwrap();
        let (_, full) = call::<Snapshot>(&api, "GET", "/api/v1/documents/default", serde_json::Value::Null).await;
        assert!(sync::diff(&full.unwrap().document, &b2).is_empty());
        // Only the two records' rows changed in SQLite.
        let store = state.db.lock().unwrap();
        let n: i64 = store.conn.query_row("SELECT count(*) FROM doc_records WHERE rev>?1", [snap.revision as i64], |r| r.get(0)).unwrap();
        assert_eq!(n, 2);
    }
    #[tokio::test]
    async fn same_cell_edits_conflict_and_nothing_is_overwritten() {
        let state = app();
        state.import("default", records(3)).unwrap();
        let api = router(state);
        let (_, snap) = call::<Snapshot>(&api, "GET", "/api/v1/documents/default", serde_json::Value::Null).await;
        let snap = snap.unwrap();
        let mut a = snap.document.clone();
        set(&mut a, 0, "a");
        let mut b = snap.document.clone();
        set(&mut b, 0, "b");
        let body = |d: &Base| serde_json::to_value(WritePatch { base_revision: snap.revision, patch: sync::diff(&snap.document, d) }).unwrap();
        assert_eq!(call::<Delta>(&api, "POST", "/api/v1/documents/default/changes", body(&a)).await.0, StatusCode::OK);
        assert_eq!(call::<Delta>(&api, "POST", "/api/v1/documents/default/changes", body(&b)).await.0, StatusCode::CONFLICT);
        let (_, full) = call::<Snapshot>(&api, "GET", "/api/v1/documents/default", serde_json::Value::Null).await;
        let f = snap.document.tables[0].fields[0].id.clone();
        assert_eq!(full.unwrap().document.tables[0].records[0].cells.get(&f), Some(&Value::Text("a".into())));
    }
    #[tokio::test]
    async fn changes_since_cover_deletes_appends_reorders_and_settings() {
        let state = app();
        state.import("default", records(5)).unwrap();
        let api = router(state);
        let (_, snap) = call::<Snapshot>(&api, "GET", "/api/v1/documents/default", serde_json::Value::Null).await;
        let snap = snap.unwrap();
        let mut a = snap.document.clone();
        a.tables[0].records.remove(1);
        let me = a.me.clone();
        let r = a.tables[0].new_record(&me);
        a.tables[0].records.insert(0, Arc::new(r));
        a.name = "Renamed".into();
        let patch = sync::diff(&snap.document, &a);
        assert_eq!(patch.orders.len(), 1);
        let body = serde_json::to_value(WritePatch { base_revision: snap.revision, patch }).unwrap();
        assert_eq!(call::<Delta>(&api, "POST", "/api/v1/documents/default/changes", body).await.0, StatusCode::OK);
        // Another window polls.
        let (st, d) = call::<Delta>(&api, "GET", &format!("/api/v1/documents/default/changes?since={}", snap.revision), serde_json::Value::Null).await;
        assert_eq!(st, StatusCode::OK);
        let mut other = snap.document.clone();
        sync::apply_delta(&mut other, &d.unwrap()).unwrap();
        let dd = sync::diff(&a, &other); assert!(dd.is_empty(), "{:?}", (dd.meta.is_some(), dd.records.iter().map(|r| (r.base.is_some(), r.new.is_some())).collect::<Vec<_>>(), dd.orders.iter().map(|o| (&o.base, &o.new)).collect::<Vec<_>>()));
        // Nothing new: 304.
        let mut req = request("GET", &format!("/api/v1/documents/default/changes?since={}", snap.revision + 1), serde_json::Value::Null);
        req.headers_mut().insert(header::IF_NONE_MATCH, format!("\"{}\"", snap.revision + 1).parse().unwrap());
        assert_eq!(api.clone().oneshot(req).await.unwrap().status(), StatusCode::NOT_MODIFIED);
        // The order survives a restart of the cache.
        let (_, full) = call::<Snapshot>(&api, "GET", "/api/v1/documents/default", serde_json::Value::Null).await;
        assert!(sync::diff(&a, &full.unwrap().document).is_empty());
    }
    #[test]
    fn version_one_databases_migrate_to_rows() {
        let conn = Connection::open_in_memory().unwrap();
        let base = records(4);
        conn.execute_batch("CREATE TABLE documents(id TEXT PRIMARY KEY, revision INTEGER NOT NULL, body TEXT NOT NULL, updated_at INTEGER NOT NULL); PRAGMA user_version=1;").unwrap();
        conn.execute("INSERT INTO documents VALUES ('default', 42, ?1, 0)", [serde_json::to_string(&base).unwrap()]).unwrap();
        let state = App::from_connection(conn, TOKEN.into()).unwrap();
        let mut store = state.db.lock().unwrap();
        let snap = store.snapshot("default").unwrap();
        assert_eq!(snap.revision, 42);
        assert!(sync::diff(&base, &snap.document).is_empty());
    }

    fn plain(method: &str, path: &str, bearer: Option<&str>, body: serde_json::Value) -> Request<Body> {
        let mut b = Request::builder().method(method).uri(path).header(header::CONTENT_TYPE, "application/json");
        if let Some(t) = bearer {
            b = b.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        b.body(Body::from(body.to_string())).unwrap()
    }
    async fn send(api: &Router, req: Request<Body>) -> (StatusCode, serde_json::Value) {
        let r = api.clone().oneshot(req).await.unwrap();
        let st = r.status();
        let bytes = to_bytes(r.into_body(), usize::MAX).await.unwrap();
        (st, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
    }
    #[tokio::test]
    async fn password_setup_needs_the_server_key_then_logins_get_their_own_credential() {
        let state = app();
        state.import("default", records(1)).unwrap();
        let api = router(state);
        let (_, st) = send(&api, plain("GET", "/api/v1/auth/status", None, serde_json::Value::Null)).await;
        assert_eq!(st["password_set"], false);
        // A stranger without the server key cannot claim the server.
        let (code, _) = send(&api, plain("POST", "/api/v1/auth/setup", None, serde_json::json!({"setup_key":"guess","password":"hunter2hunter2"}))).await;
        assert_eq!(code, StatusCode::UNAUTHORIZED);
        let (code, b) = send(&api, plain("POST", "/api/v1/auth/setup", None, serde_json::json!({"setup_key":TOKEN,"password":"hunter2hunter2"}))).await;
        assert_eq!(code, StatusCode::OK);
        let session = b["token"].as_str().unwrap().to_string();
        assert_eq!(send(&api, plain("POST", "/api/v1/auth/setup", None, serde_json::json!({"setup_key":TOKEN,"password":"another-one"}))).await.0, StatusCode::CONFLICT);
        assert_eq!(send(&api, plain("GET", "/api/v1/documents", Some(&session), serde_json::Value::Null)).await.0, StatusCode::OK);
        assert_eq!(send(&api, plain("GET", "/api/v1/documents", Some("rts_forged"), serde_json::Value::Null)).await.0, StatusCode::UNAUTHORIZED);
        let (code, b) = send(&api, plain("POST", "/api/v1/auth/login", None, serde_json::json!({"password":"hunter2hunter2"}))).await;
        assert_eq!(code, StatusCode::OK);
        let second = b["token"].as_str().unwrap().to_string();
        // Wrong passwords lock logins for a while.
        for _ in 0..5 {
            assert_eq!(send(&api, plain("POST", "/api/v1/auth/login", None, serde_json::json!({"password":"wrong"}))).await.0, StatusCode::UNAUTHORIZED);
        }
        assert_eq!(send(&api, plain("POST", "/api/v1/auth/login", None, serde_json::json!({"password":"hunter2hunter2"}))).await.0, StatusCode::TOO_MANY_REQUESTS);
        // Logging out ends that credential only.
        assert_eq!(send(&api, plain("POST", "/api/v1/auth/logout", Some(&second), serde_json::Value::Null)).await.0, StatusCode::NO_CONTENT);
        assert_eq!(send(&api, plain("GET", "/api/v1/documents", Some(&second), serde_json::Value::Null)).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(send(&api, plain("GET", "/api/v1/documents", Some(&session), serde_json::Value::Null)).await.0, StatusCode::OK);
        // AI keys are separate and revocable.
        let (_, k) = send(&api, plain("POST", "/api/v1/auth/tokens", Some(&session), serde_json::json!({"name":"Claude"}))).await;
        let key = k["token"].as_str().unwrap().to_string();
        assert!(key.starts_with("rtk_"));
        assert_eq!(send(&api, plain("GET", "/api/v1/documents", Some(&key), serde_json::Value::Null)).await.0, StatusCode::OK);
        let path = format!("/api/v1/auth/tokens/{}", k["id"].as_str().unwrap());
        assert_eq!(send(&api, plain("DELETE", &path, Some(&session), serde_json::Value::Null)).await.0, StatusCode::NO_CONTENT);
        assert_eq!(send(&api, plain("GET", "/api/v1/documents", Some(&key), serde_json::Value::Null)).await.0, StatusCode::UNAUTHORIZED);
    }
    #[tokio::test]
    async fn bases_are_listed_renamed_reordered_trashed_and_restored() {
        let state = app();
        state.import("default", records(3)).unwrap();
        let api = router(state);
        let (code, b) = send(&api, plain("POST", "/api/v1/documents", Some(TOKEN), serde_json::json!({"name":"项目","icon":"🚀"}))).await;
        assert_eq!(code, StatusCode::OK);
        let id = b["id"].as_str().unwrap().to_string();
        let (_, list) = send(&api, plain("GET", "/api/v1/documents", Some(TOKEN), serde_json::Value::Null)).await;
        let names: Vec<&str> = list.as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap()).collect();
        assert_eq!(names.len(), 2);
        assert_eq!(list[0]["records"], 3);
        assert_eq!(list[1]["icon"], "🚀");
        let (_, e) = send(&api, plain("PATCH", &format!("/api/v1/documents/{id}"), Some(TOKEN), serde_json::json!({"name":"新项目","icon":"📊"}))).await;
        assert_eq!((e["name"].as_str(), e["icon"].as_str()), (Some("新项目"), Some("📊")));
        // The name lives in the document: a client reading it sees the same.
        let (_, doc) = send(&api, plain("GET", &format!("/api/v1/documents/{id}"), Some(TOKEN), serde_json::Value::Null)).await;
        assert_eq!(doc["document"]["icon"], "📊");
        let (_, list) = send(&api, plain("PUT", "/api/v1/workspace/order", Some(TOKEN), serde_json::json!({"ids":[id, "default"]}))).await;
        assert_eq!(list[0]["id"].as_str(), Some(id.as_str()));
        assert_eq!(send(&api, plain("DELETE", &format!("/api/v1/documents/{id}"), Some(TOKEN), serde_json::Value::Null)).await.0, StatusCode::NO_CONTENT);
        assert_eq!(send(&api, plain("GET", &format!("/api/v1/documents/{id}"), Some(TOKEN), serde_json::Value::Null)).await.0, StatusCode::NOT_FOUND);
        let (_, trash) = send(&api, plain("GET", "/api/v1/trash", Some(TOKEN), serde_json::Value::Null)).await;
        assert_eq!(trash[0]["id"].as_str(), Some(id.as_str()));
        assert_eq!(send(&api, plain("POST", &format!("/api/v1/documents/{id}/restore"), Some(TOKEN), serde_json::Value::Null)).await.0, StatusCode::OK);
        let (_, list) = send(&api, plain("GET", "/api/v1/documents", Some(TOKEN), serde_json::Value::Null)).await;
        assert_eq!(list.as_array().unwrap().len(), 2);
    }
    #[tokio::test]
    async fn mcp_tools_read_and_write_records_by_field_name() {
        let state = app();
        state.import("default", records(2)).unwrap();
        let api = router(state);
        let rpc = |id: u64, method: &str, params: serde_json::Value| plain("POST", "/mcp", Some(TOKEN), serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        let (_, init) = send(&api, rpc(1, "initialize", serde_json::json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}))).await;
        assert_eq!(init["result"]["serverInfo"]["name"], "rutable");
        assert_eq!(send(&api, plain("POST", "/mcp", Some(TOKEN), serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}))).await.0, StatusCode::ACCEPTED);
        let (_, tools) = send(&api, rpc(2, "tools/list", serde_json::json!({}))).await;
        assert!(tools["result"]["tools"].as_array().unwrap().len() >= 7);
        let call = |id: u64, name: &str, args: serde_json::Value| rpc(id, "tools/call", serde_json::json!({"name":name,"arguments":args}));
        let (_, bases) = send(&api, call(3, "list_bases", serde_json::json!({}))).await;
        let table = bases["result"]["structuredContent"]["bases"][0]["tables"][0]["name"].as_str().unwrap().to_string();
        let (_, d) = send(&api, call(4, "describe_table", serde_json::json!({"base_id":"default","table":table}))).await;
        let fields = d["result"]["structuredContent"]["fields"].as_array().unwrap().clone();
        let primary = fields[0]["name"].as_str().unwrap().to_string();
        let (_, f) = send(&api, call(5, "create_field", serde_json::json!({"base_id":"default","table":table,"name":"状态","type":"single_select","options":["待办"]}))).await;
        assert!(f["result"].get("isError").is_none(), "{f}");
        let (_, c) = send(&api, call(6, "create_records", serde_json::json!({"base_id":"default","table":table,"records":[{primary.clone():"AI 写入","状态":"进行中"}]}))).await;
        let rid = c["result"]["structuredContent"]["record_ids"][0].as_str().unwrap().to_string();
        let (_, l) = send(&api, call(7, "list_records", serde_json::json!({"base_id":"default","table":table,"where":{primary.clone():"ai 写"}}))).await;
        let got = &l["result"]["structuredContent"];
        assert_eq!(got["total"], 1);
        assert_eq!(got["records"][0]["fields"]["状态"], "进行中"); // option created on the fly
        let (_, u) = send(&api, call(8, "update_records", serde_json::json!({"base_id":"default","table":table,"records":[{"record_id":rid,"fields":{"状态":null}}]}))).await;
        assert_eq!(u["result"]["structuredContent"]["updated"], 1);
        let (_, bad) = send(&api, call(9, "update_records", serde_json::json!({"base_id":"default","table":table,"records":[{"record_id":rid,"fields":{"不存在":1}}]}))).await;
        assert_eq!(bad["result"]["isError"], true);
        let (_, del) = send(&api, call(10, "delete_records", serde_json::json!({"base_id":"default","table":table,"record_ids":[rid]}))).await;
        assert_eq!(del["result"]["structuredContent"]["deleted"], 1);
        // Every write was an ordinary revision the desktop syncs.
        let (_, snap) = send(&api, plain("GET", "/api/v1/documents/default", Some(TOKEN), serde_json::Value::Null)).await;
        assert_eq!(snap["revision"], 5);
        // A new base of its own, listed alongside and ready for tables.
        let (_, nb) = send(&api, call(11, "create_base", serde_json::json!({"name":"AI 新建","icon":"🤖","table":"客户","fields":["名称","电话"]}))).await;
        let nb = &nb["result"]["structuredContent"];
        let bid = nb["base_id"].as_str().unwrap().to_string();
        assert_eq!(nb["tables"].as_array().unwrap().len(), 1);
        assert_eq!(nb["tables"][0]["name"], "客户");
        let (_, d) = send(&api, call(12, "describe_table", serde_json::json!({"base_id":bid,"table":"客户"}))).await;
        let d = &d["result"]["structuredContent"];
        assert_eq!(d["records"], 0);
        let names: Vec<&str> = d["fields"].as_array().unwrap().iter().map(|f| f["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["名称", "电话"]);
        let (_, dup) = send(&api, call(15, "create_base", serde_json::json!({"name":"重复","fields":["a","a"]}))).await;
        assert_eq!(dup["result"]["isError"], true);
        let (_, bases) = send(&api, call(13, "list_bases", serde_json::json!({}))).await;
        assert_eq!(bases["result"]["structuredContent"]["bases"][1]["name"], "AI 新建");
        let (_, empty) = send(&api, call(14, "create_base", serde_json::json!({"name":"  "}))).await;
        assert_eq!(empty["result"]["isError"], true);
    }
    #[tokio::test]
    async fn waiting_clients_wake_on_a_change() {
        let state = app();
        state.import("default", records(1)).unwrap();
        let api = router(state.clone());
        let waiter = {
            let api = api.clone();
            tokio::spawn(async move { send(&api, plain("GET", "/api/v1/documents/default/wait?since=1&timeout=10", Some(TOKEN), serde_json::Value::Null)).await })
        };
        tokio::time::sleep(Duration::from_millis(100)).await;
        let started = std::time::Instant::now();
        let (_, _) = send(&api, plain("PATCH", "/api/v1/documents/default", Some(TOKEN), serde_json::json!({"name":"改名"}))).await;
        let (code, b) = waiter.await.unwrap();
        assert_eq!(code, StatusCode::OK);
        assert_eq!(b["changed"], true);
        assert_eq!(b["revision"], 2);
        assert!(started.elapsed() < Duration::from_secs(2));
        let (_, b) = send(&api, plain("GET", "/api/v1/documents/default/wait?since=2&timeout=1", Some(TOKEN), serde_json::Value::Null)).await;
        assert_eq!(b["changed"], false);
    }
    #[tokio::test]
    async fn the_home_page_is_public_and_offers_setup() {
        let api = router(app());
        let r = api.oneshot(Request::builder().uri("/").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        let html = String::from_utf8(to_bytes(r.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
        assert!(html.contains("设置访问密码") && html.contains("客户端尚未发布"));
    }
}
