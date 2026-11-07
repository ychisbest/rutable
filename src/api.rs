//! The desktop's only document storage is the authenticated server API.
use crate::model::sync::{self, Delta, Patch};
use crate::model::{Attachment, Base};
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone)]
pub struct Config {
    pub url: String,
    pub document: String,
    pub token: String,
    agent: ureq::Agent,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub revision: u64,
    pub document: Base,
}
#[derive(Debug)]
pub enum Error {
    Message(String),
    Conflict,
    NotFound,
    /// The login ended (expired, revoked, or the password changed).
    Unauthorized,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(s) => f.write_str(s),
            Self::Conflict => f.write_str("服务器数据同时发生变化，请重试"),
            Self::NotFound => f.write_str("服务器中没有此多维表格（可能已被删除）"),
            Self::Unauthorized => f.write_str("登录已失效，请重新输入密码"),
        }
    }
}
type Result<T> = std::result::Result<T, Error>;
impl Config {
    pub fn new(url: &str, document: &str, token: String) -> Result<Self> {
        let u = url::Url::parse(url.trim()).map_err(|_| Error::Message("服务器地址无效".into()))?;
        let local = matches!(u.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
        if !(u.scheme() == "https" || u.scheme() == "http" && local)
            || !u.username().is_empty()
            || u.password().is_some()
            || u.query().is_some()
            || u.fragment().is_some()
            || u.path() != "/"
        {
            return Err(Error::Message("请使用 HTTPS 服务器根地址（本机调试可使用 HTTP）".into()));
        }
        if document.is_empty() || document.len() > 80 || !document.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')) {
            return Err(Error::Message("文档 ID 无效".into()));
        }
        if token.trim().is_empty() {
            return Err(Error::Message("请输入服务器访问密钥，或设置 RUTABLE_API_TOKEN".into()));
        }
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(12)))
            .max_redirects(0)
            .http_status_as_error(false)
            .build()
            .into();
        Ok(Self { url: u.as_str().trim_end_matches('/').into(), document: document.into(), token, agent })
    }
    pub fn from_env() -> Result<Self> {
        Self::new(&crate::args::api_url(), &crate::args::document_id(), std::env::var("RUTABLE_API_TOKEN").unwrap_or_default())
    }
    fn agent(&self) -> &ureq::Agent {
        &self.agent
    }
    fn endpoint(&self) -> String {
        format!("{}/api/v1/documents/{}", self.url, self.document)
    }
    fn auth(&self) -> String {
        format!("Bearer {}", self.token)
    }
    pub fn get(&self, revision: Option<u64>) -> Result<Option<Snapshot>> {
        let agent = self.agent();
        // The whole document can take a while on a slow line.
        let mut request = agent
            .get(self.endpoint())
            .header("Authorization", self.auth())
            .config()
            .timeout_global(Some(Duration::from_secs(120)))
            .build();
        if let Some(revision) = revision {
            request = request.header("If-None-Match", format!("\"{revision}\""));
        }
        let mut response = request.call().map_err(network)?;
        if response.status().as_u16() == 304 {
            return Ok(None);
        }
        check(&response)?;
        json(&mut response)
            .map(Some)
            .map_err(|_| Error::Message("服务器文档响应无效".into()))
    }
    /// What changed after `since`; `None` when nothing did.
    pub fn changes(&self, since: u64) -> Result<Option<Delta>> {
        let mut response = self
            .agent()
            .get(format!("{}/changes?since={since}", self.endpoint()))
            .header("Authorization", self.auth())
            .header("If-None-Match", format!("\"{since}\""))
            .call()
            .map_err(network)?;
        if response.status().as_u16() == 304 {
            return Ok(None);
        }
        check(&response)?;
        json(&mut response).map(Some).map_err(|_| Error::Message("服务器变更响应无效".into()))
    }
    /// Saves the changes made since `base_revision`; returns everything changed since then.
    pub fn push(&self, base_revision: u64, patch: &Patch) -> Result<Delta> {
        let mut response = self
            .agent()
            .post(format!("{}/changes", self.endpoint()))
            .header("Authorization", self.auth())
            .send_json(serde_json::json!({"base_revision":base_revision,"patch":patch}))
            .map_err(network)?;
        if response.status().as_u16() == 409 {
            let body: serde_json::Value = response.body_mut().read_json().unwrap_or_default();
            let reason = body["error"].as_str().unwrap_or("");
            return Err(if reason.starts_with("revision") || reason.is_empty() {
                Error::Conflict
            } else {
                Error::Message(format!("{reason}；修改仍在内存中，请处理后重试或重新读取服务器"))
            });
        }
        check(&response)?;
        json(&mut response).map_err(|_| Error::Message("服务器保存响应无效".into()))
    }
    /// The same server and login, another document.
    pub fn with_document(&self, document: &str) -> Config {
        Config { document: document.into(), ..self.clone() }
    }
    fn api(&self, path: &str) -> String {
        format!("{}/api/v1{path}", self.url)
    }
    /// A JSON call to `path` (under /api/v1); errors carry the server's message.
    fn call<T: serde::de::DeserializeOwned>(&self, method: &str, path: &str, body: Option<serde_json::Value>) -> Result<T> {
        call(&self.agent, method, &self.api(path), Some(&self.token), body)
    }
    pub fn bases(&self) -> Result<Vec<BaseEntry>> {
        self.call("GET", "/documents", None)
    }
    pub fn trash(&self) -> Result<Vec<BaseEntry>> {
        self.call("GET", "/trash", None)
    }
    pub fn create_base(&self, name: &str, icon: &str) -> Result<BaseEntry> {
        self.call("POST", "/documents", Some(serde_json::json!({"name": name, "icon": icon})))
    }
    pub fn update_base(&self, id: &str, name: Option<&str>, icon: Option<&str>) -> Result<BaseEntry> {
        self.call("PATCH", &format!("/documents/{id}"), Some(serde_json::json!({"name": name, "icon": icon})))
    }
    pub fn delete_base(&self, id: &str) -> Result<()> {
        self.call::<serde_json::Value>("DELETE", &format!("/documents/{id}"), None).map(|_| ())
    }
    pub fn restore_base(&self, id: &str) -> Result<BaseEntry> {
        self.call("POST", &format!("/documents/{id}/restore"), None)
    }
    pub fn reorder_bases(&self, ids: &[String]) -> Result<Vec<BaseEntry>> {
        self.call("PUT", "/workspace/order", Some(serde_json::json!({"ids": ids})))
    }
    /// A new key for an AI tool (MCP); the secret is shown only now.
    pub fn create_ai_key(&self, name: &str) -> Result<String> {
        let v: serde_json::Value = self.call("POST", "/auth/tokens", Some(serde_json::json!({"name": name})))?;
        v["token"].as_str().map(str::to_string).ok_or_else(|| Error::Message("服务器没有返回密钥".into()))
    }
    pub fn ai_keys(&self) -> Result<Vec<serde_json::Value>> {
        self.call("GET", "/auth/tokens", None)
    }
    pub fn delete_ai_key(&self, id: &str) -> Result<()> {
        self.call::<serde_json::Value>("DELETE", &format!("/auth/tokens/{id}"), None).map(|_| ())
    }
    pub fn change_password(&self, current: &str, new: &str) -> Result<()> {
        self.call::<serde_json::Value>("POST", "/auth/password", Some(serde_json::json!({"current": current, "new": new}))).map(|_| ())
    }
    pub fn logout(&self) -> Result<()> {
        self.call::<serde_json::Value>("POST", "/auth/logout", None).map(|_| ())
    }
    /// Waits (up to 25 s) until the document is past `since`; its revision then.
    pub fn wait(&self, since: u64) -> Result<u64> {
        let mut response = self
            .agent
            .get(format!("{}/wait?since={since}&timeout=25", self.endpoint()))
            .header("Authorization", self.auth())
            .config()
            .timeout_global(Some(Duration::from_secs(40)))
            .build()
            .call()
            .map_err(network)?;
        check(&response)?;
        let v: serde_json::Value = json(&mut response).map_err(|_| Error::Message("服务器响应无效".into()))?;
        Ok(v["revision"].as_u64().unwrap_or(since))
    }
    pub fn upload(&self, path: &std::path::Path) -> Result<Attachment> {
        let metadata = std::fs::metadata(path).map_err(|_| Error::Message("无法读取所选文件".into()))?;
        if metadata.len() > 64 * 1024 * 1024 {
            return Err(Error::Message("单个附件不能超过 64 MiB".into()));
        }
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).ok_or_else(|| Error::Message("文件名无效".into()))?;
        let bytes = std::fs::read(path).map_err(|_| Error::Message("无法读取所选文件".into()))?;
        let mut url = url::Url::parse(&format!("{}/attachments", self.endpoint())).unwrap();
        url.query_pairs_mut().append_pair("name", &name);
        let mut response = self
            .agent()
            .post(url.as_str())
            .header("Authorization", self.auth())
            .header("Content-Type", "application/octet-stream")
            .config()
            .timeout_global(Some(Duration::from_secs(120)))
            .build()
            .send(bytes.as_slice())
            .map_err(network)?;
        check(&response)?;
        response.body_mut().read_json().map_err(|_| Error::Message("附件上传响应无效".into()))
    }
    pub fn attachment_link(&self, path: &str) -> Result<String> {
        let id = path
            .strip_prefix("rutable://attachment/")
            .filter(|id| id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
            .ok_or_else(|| Error::Message("服务器附件地址无效".into()))?;
        let mut response = self
            .agent()
            .post(format!("{}/attachments/{id}/link", self.endpoint()))
            .header("Authorization", self.auth())
            .send_empty()
            .map_err(network)?;
        check(&response)?;
        let body: serde_json::Value = response.body_mut().read_json().map_err(|_| Error::Message("附件链接响应无效".into()))?;
        let path = body["path"]
            .as_str()
            .filter(|p| p.starts_with("/attachments/") && !p.starts_with("//"))
            .ok_or_else(|| Error::Message("服务器附件链接无效".into()))?;
        Ok(format!("{}{path}", self.url))
    }
}
/// Reads the whole body, then parses it: parsing straight from the
/// (decompressing) stream goes byte by byte and is many times slower.
fn json<T: serde::de::DeserializeOwned>(response: &mut ureq::http::Response<ureq::Body>) -> std::result::Result<T, ()> {
    let bytes = response.body_mut().with_config().limit(64 * 1024 * 1024).read_to_vec().map_err(|_| ())?;
    serde_json::from_slice(&bytes).map_err(|_| ())
}
/// A base in the list.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct BaseEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub icon: String,
    pub revision: u64,
    pub updated_at: i64,
    pub tables: usize,
    pub records: usize,
    #[serde(default)]
    pub deleted_at: Option<i64>,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(15))).max_redirects(0).http_status_as_error(false).build().into()
}

fn call<T: serde::de::DeserializeOwned>(agent: &ureq::Agent, method: &str, url: &str, token: Option<&str>, body: Option<serde_json::Value>) -> Result<T> {
    let mut req = ureq::http::Request::builder().method(method).uri(url);
    if let Some(t) = token {
        req = req.header("Authorization", format!("Bearer {t}"));
    }
    let body = body.map(|b| b.to_string()).unwrap_or_default();
    let req = req.header("Content-Type", "application/json").body(body).map_err(|_| Error::Message("请求无效".into()))?;
    let mut response = agent.run(req).map_err(network)?;
    let code = response.status().as_u16();
    if code == 204 {
        return serde_json::from_value(serde_json::Value::Null).map_err(|_| Error::Message("服务器响应无效".into()));
    }
    if !(200..300).contains(&code) {
        // The server explains (wrong password, locked out, ...).
        let msg = json::<serde_json::Value>(&mut response).ok().and_then(|v| v["error"].as_str().map(str::to_string));
        return Err(match (code, msg) {
            (401, Some(m)) if m != "unauthorized" => Error::Message(m),
            (401 | 403, _) => Error::Unauthorized,
            (404, _) => Error::NotFound,
            (_, Some(m)) => Error::Message(m),
            _ => Error::Message(format!("服务器请求失败（HTTP {code}）")),
        });
    }
    json(&mut response).map_err(|_| Error::Message("服务器响应无效".into()))
}

/// Before logging in: has the server's password been set?
pub fn password_set(url: &str) -> Result<bool> {
    let v: serde_json::Value = call(&agent(), "GET", &format!("{}/api/v1/auth/status", url.trim_end_matches('/')), None, None)?;
    Ok(v["password_set"].as_bool().unwrap_or(false))
}

fn device() -> String {
    std::env::var("COMPUTERNAME").map(|c| format!("Windows · {c}")).unwrap_or_else(|_| "Windows".into())
}

/// Sets the first password (the server key proves ownership); a login credential.
pub fn setup(url: &str, server_key: &str, password: &str) -> Result<String> {
    let v: serde_json::Value = call(
        &agent(),
        "POST",
        &format!("{}/api/v1/auth/setup", url.trim_end_matches('/')),
        None,
        Some(serde_json::json!({"setup_key": server_key, "password": password, "device": device()})),
    )?;
    v["token"].as_str().map(str::to_string).ok_or_else(|| Error::Message("服务器没有返回登录凭证".into()))
}

pub fn login(url: &str, password: &str) -> Result<String> {
    let v: serde_json::Value = call(
        &agent(),
        "POST",
        &format!("{}/api/v1/auth/login", url.trim_end_matches('/')),
        None,
        Some(serde_json::json!({"password": password, "device": device()})),
    )?;
    v["token"].as_str().map(str::to_string).ok_or_else(|| Error::Message("服务器没有返回登录凭证".into()))
}

fn network(_: ureq::Error) -> Error {
    Error::Message("无法连接服务器，未保存的修改仍在当前窗口内存中".into())
}
fn check(response: &ureq::http::Response<ureq::Body>) -> Result<()> {
    let code = response.status().as_u16();
    match code {
        200..=299 => Ok(()),
        401 | 403 => Err(Error::Unauthorized),
        409 => Err(Error::Conflict),
        404 => Err(Error::NotFound),
        413 => Err(Error::Message("提交内容过大".into())),
        _ => Err(Error::Message(format!("服务器请求失败（HTTP {code}），修改尚未保存"))),
    }
}
/// Three-way merge, item by item: `local`'s changes since `base` applied onto `remote`.
pub fn merge(base: &Base, local: &Base, remote: &Base) -> Result<Base> {
    sync::merge(base, local, remote).map_err(|e| Error::Message(format!("{e}；修改仍在内存中，请处理后重试或重新读取服务器")))
}
pub enum Outcome {
    Unchanged,
    Loaded(Snapshot),
    /// The last copy this computer kept; a sync with the server follows at once.
    Cached(Snapshot),
}
/// The whole document from the server, kept for the next start.
fn full(config: &Config) -> Result<Snapshot> {
    let snapshot = config.get(None)?.ok_or_else(|| Error::Message("服务器没有返回文档".into()))?;
    cache::write(config, &snapshot, true);
    Ok(snapshot)
}
/// One round with the server. The first round shows the copy kept on this
/// computer (or downloads the whole document); later rounds move only the
/// records and settings that changed, in either direction.
pub fn sync(config: &Config, baseline: Option<&Snapshot>, local: &Base, dirty: bool) -> Result<Outcome> {
    let Some(baseline) = baseline else {
        if let Some(cached) = cache::read(config) {
            return Ok(Outcome::Cached(cached));
        }
        return full(config).map(Outcome::Loaded);
    };
    let patch = if dirty { sync::diff(&baseline.document, local) } else { Patch::default() };
    let delta = if patch.is_empty() {
        match config.changes(baseline.revision) {
            // Marked unsaved, yet nothing differs from the server (an edit
            // undone, a value set to what it was): answer with the server's
            // version, so the window sees it is saved — and can close.
            Ok(None) if dirty => return Ok(Outcome::Loaded(baseline.clone())),
            Ok(None) => return Ok(Outcome::Unchanged),
            Ok(Some(d)) => d,
            // The kept copy is newer than the server (restored from a backup): start over.
            Err(Error::Conflict) => return full(config).map(Outcome::Loaded),
            Err(Error::NotFound) => {
                cache::remove(config);
                return Err(Error::NotFound);
            }
            Err(e) => return Err(e),
        }
    } else {
        config.push(baseline.revision, &patch)?
    };
    let mut document = baseline.document.clone();
    match sync::apply_delta(&mut document, &delta) {
        Ok(()) => {
            let snapshot = Snapshot { revision: delta.revision, document };
            cache::write(config, &snapshot, false);
            Ok(Outcome::Loaded(snapshot))
        }
        // Out of step: start over from the whole document.
        Err(_) => full(config).map(Outcome::Loaded),
    }
}

/// The last server version of a document, kept on this computer so a start
/// shows it at once and downloads only what changed since. It only ever
/// holds what the server confirmed: unsaved edits stay in memory.
pub mod cache {
    use super::{Config, Snapshot};
    use std::{collections::HashMap, path::PathBuf, sync::Mutex, time::Instant};

    static WRITTEN: Mutex<Option<HashMap<PathBuf, Instant>>> = Mutex::new(None);

    pub fn enabled() -> bool {
        !std::env::args().any(|a| a == "--no-cache")
    }

    fn path(config: &Config) -> Option<PathBuf> {
        if !enabled() {
            return None;
        }
        let dir = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?.join("Rutable").join("cache");
        // FNV-1a of the server and document: one file per document per server.
        let mut h: u64 = 0xcbf29ce484222325;
        for b in config.url.bytes().chain([0]).chain(config.document.bytes()) {
            h = (h ^ b as u64).wrapping_mul(0x100000001b3);
        }
        Some(dir.join(format!("{}-{h:016x}.json", config.document)))
    }

    pub fn read(config: &Config) -> Option<Snapshot> {
        let bytes = std::fs::read(path(config)?).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Saves `snapshot`; unless `now`, at most every 30 seconds (a stale copy
    /// only means a bigger first sync).
    pub fn write(config: &Config, snapshot: &Snapshot, now: bool) {
        let Some(path) = path(config) else { return };
        {
            let mut written = WRITTEN.lock().unwrap_or_else(|e| e.into_inner());
            let written = written.get_or_insert_with(HashMap::new);
            if !now && written.get(&path).is_some_and(|t| t.elapsed().as_secs() < 30) {
                return;
            }
            written.insert(path.clone(), Instant::now());
        }
        let Ok(body) = serde_json::to_vec(snapshot) else { return };
        let Some(dir) = path.parent() else { return };
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
        // Written aside, then moved over: a crash never leaves half a file.
        let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
        if std::fs::write(&tmp, body).is_ok() && std::fs::rename(&tmp, &path).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
    }

    pub fn remove(config: &Config) {
        if let Some(p) = path(config) {
            let _ = std::fs::remove_file(p);
        }
    }
}
