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
use rutable_model::{Base, Value};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone)]
pub struct App {
    db: Arc<Mutex<Connection>>,
    token: Arc<String>,
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
type Result<T> = std::result::Result<T, Error>;
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
fn error(status: StatusCode, message: impl Into<String>) -> Error {
    Error(status, message.into())
}
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
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;
  CREATE TABLE IF NOT EXISTS documents(id TEXT PRIMARY KEY, revision INTEGER NOT NULL, body TEXT NOT NULL, updated_at INTEGER NOT NULL);
  CREATE TABLE IF NOT EXISTS history(document_id TEXT NOT NULL, revision INTEGER NOT NULL, body TEXT NOT NULL, saved_at INTEGER NOT NULL,PRIMARY KEY(document_id,revision));
  CREATE TABLE IF NOT EXISTS attachments(id TEXT PRIMARY KEY, document_id TEXT NOT NULL, name TEXT NOT NULL, mime TEXT NOT NULL, body BLOB NOT NULL, created_at INTEGER NOT NULL);
  CREATE INDEX IF NOT EXISTS attachments_document ON attachments(document_id);
  CREATE TABLE IF NOT EXISTS attachment_links(token TEXT PRIMARY KEY,attachment_id TEXT NOT NULL,expires_at INTEGER NOT NULL);
  PRAGMA user_version=1;")?;
        Ok(Self { db: Arc::new(Mutex::new(db)), token: Arc::new(token) })
    }
    pub fn import(&self, id: &str, document: Base) -> Result<()> {
        valid_id(id)?;
        validate(&document)?;
        let mut db = self.db.lock().map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "数据库锁异常"))?;
        if db.query_row("SELECT 1 FROM documents WHERE id=?1", [id], |r| r.get::<_, i64>(0)).optional()?.is_some() {
            return Err(error(StatusCode::CONFLICT, "目标文档已存在，导入不会覆盖现有数据"));
        }
        write(&mut db, id, WriteDocument { expected_revision: 0, document })?;
        Ok(())
    }
}
fn valid_id(id: &str) -> Result<()> {
    if id.is_empty() || id.len() > 80 || !id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')) {
        return Err(error(StatusCode::BAD_REQUEST, "文档 ID 无效"));
    }
    Ok(())
}
fn validate(b: &Base) -> Result<()> {
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
fn attachment_ids(v: &Value, ids: &mut Vec<String>) -> Result<()> {
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
fn write(db: &mut Connection, id: &str, request: WriteDocument) -> Result<Snapshot> {
    validate(&request.document)?;
    let body = serde_json::to_string(&request.document)?;
    if body.len() > 32 * 1024 * 1024 {
        return Err(error(StatusCode::PAYLOAD_TOO_LARGE, "文档不能超过 32 MiB"));
    }
    let transaction = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = transaction
        .query_row("SELECT revision FROM documents WHERE id=?1", [id], |r| r.get::<_, i64>(0).map(|v| v as u64))
        .optional()?
        .unwrap_or(0);
    if request.expected_revision != current {
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
    for file in files {
        if transaction
            .query_row("SELECT 1 FROM attachments WHERE id=?1 AND document_id=?2", params![file, id], |r| r.get::<_, i64>(0))
            .optional()?
            .is_none()
        {
            return Err(error(StatusCode::BAD_REQUEST, "引用的服务器附件不存在"));
        }
    }
    let revision = current.checked_add(1).ok_or_else(|| error(StatusCode::CONFLICT, "版本号超出范围"))?;
    let now = rutable_model::now_ms();
    transaction.execute("INSERT INTO documents(id,revision,body,updated_at) VALUES (?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,body=excluded.body,updated_at=excluded.updated_at",params![id,revision as i64,body,now])?;
    transaction
        .execute("INSERT INTO history(document_id,revision,body,saved_at) VALUES (?1,?2,?3,?4)", params![id, revision as i64, body, now])?;
    transaction.execute("DELETE FROM history WHERE document_id=?1 AND revision<=?2", params![id, revision.saturating_sub(100) as i64])?;
    transaction.commit()?;
    Ok(Snapshot { revision, document: request.document })
}
async fn db<T: Send + 'static>(state: App, job: impl FnOnce(&mut Connection) -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(move || {
        let mut conn = state.db.lock().map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "数据库锁异常"))?;
        job(&mut conn)
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "数据库线程中断"))?
}
async fn auth(State(state): State<App>, request: axum::extract::Request, next: Next) -> Response {
    let provided =
        request.headers().get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).unwrap_or("");
    let expected = state.token.as_bytes();
    let actual = provided.as_bytes();
    let matches = actual.len() == expected.len() && actual.iter().zip(expected).fold(0u8, |diff, (a, b)| diff | (a ^ b)) == 0;
    if !matches {
        return error(StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    next.run(request).await
}
pub fn router(state: App) -> Router {
    let api = Router::new()
        .route("/documents/{id}", get(read_document).put(save_document))
        .route("/documents/{id}/attachments", post(upload))
        .route("/documents/{id}/attachments/{file}", get(download))
        .route("/documents/{id}/attachments/{file}/link", post(create_link))
        .layer(middleware::from_fn_with_state(state.clone(), auth));
    Router::new()
        .route("/healthz", get(|| async { Json(serde_json::json!({"status":"ok","storage":"sqlite"})) }))
        .nest("/api/v1", api)
        .route("/attachments/{file}/view", get(view_attachment))
        .route("/attachments/{file}/content", get(linked_content))
        .layer(DefaultBodyLimit::max(64 * 1024 * 1024))
        .layer(middleware::from_fn(|request: axum::extract::Request, next: Next| async move {
            let mut response = next.run(request).await;
            response.headers_mut().insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
            response.headers_mut().insert("X-Content-Type-Options", "nosniff".parse().unwrap());
            response
        }))
        .with_state(state)
}
async fn read_document(State(state): State<App>, Path(id): Path<String>, headers: HeaderMap) -> Result<Response> {
    valid_id(&id)?;
    let tag = headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()).map(str::to_string);
    db(state, move |db| {
        let revision = db
            .query_row("SELECT revision FROM documents WHERE id=?1", [&id], |r| r.get::<_, i64>(0).map(|v| v as u64))
            .optional()?
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "document_not_found"))?;
        let etag = format!("\"{revision}\"");
        if tag.as_deref() == Some(etag.as_str()) {
            return Ok((StatusCode::NOT_MODIFIED, [(header::ETAG, etag)]).into_response());
        }
        let body: String = db.query_row("SELECT body FROM documents WHERE id=?1", [&id], |r| r.get(0))?;
        let snapshot = Snapshot { revision, document: serde_json::from_str(&body)? };
        Ok(([(header::ETAG, etag)], Json(snapshot)).into_response())
    })
    .await
}
async fn save_document(State(state): State<App>, Path(id): Path<String>, Json(request): Json<WriteDocument>) -> Result<Json<Snapshot>> {
    valid_id(&id)?;
    db(state, move |db| write(db, &id, request)).await.map(Json)
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
    db(state, move |db| {
        if db.query_row("SELECT 1 FROM documents WHERE id=?1", [&id], |r| r.get::<_, i64>(0)).optional()?.is_none() {
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
    db(state, move |db| {
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
    db(state, move |db| {
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
fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}
fn linked(db: &Connection, file: &str, token: &str) -> Result<(String, String, Vec<u8>)> {
    db.query_row("SELECT a.name,a.mime,a.body FROM attachments a JOIN attachment_links l ON l.attachment_id=a.id WHERE a.id=?1 AND l.token=?2 AND l.expires_at>?3",params![file,token,rutable_model::now_ms()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?.ok_or_else(||error(StatusCode::NOT_FOUND,"附件链接无效或已过期"))
}
async fn view_attachment(State(state): State<App>, Path(file): Path<String>, Query(link): Query<Link>) -> Result<Html<String>> {
    db(state,move |db| {
  let (name,mime,body)=linked(db,&file,&link.token)?;
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
    db(state, move |db| {
        let (_, mime, body) = linked(db, &file, &link.token)?;
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
        let db = state.db.lock().unwrap();
        let body: String = db.query_row("SELECT body FROM documents WHERE id='default'", [], |r| r.get(0)).unwrap();
        assert_eq!(serde_json::from_str::<Base>(&body).unwrap().name, "Persisted");
        drop(db);
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
        t.records.push(r);
        assert_eq!(state.import("default", base).unwrap_err().0, StatusCode::BAD_REQUEST);
    }
}
