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
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(s) => f.write_str(s),
            Self::Conflict => f.write_str("服务器数据同时发生变化，请重试"),
            Self::NotFound => f.write_str("服务器中没有此文档，请先导入数据"),
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
fn network(_: ureq::Error) -> Error {
    Error::Message("无法连接服务器，未保存的修改仍在当前窗口内存中".into())
}
fn check(response: &ureq::http::Response<ureq::Body>) -> Result<()> {
    let code = response.status().as_u16();
    match code {
        200..=299 => Ok(()),
        401 | 403 => Err(Error::Message("服务器访问密钥无效或无访问权限".into())),
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
