//! The desktop's only document storage is the authenticated server API.
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
        let mut request = agent.get(self.endpoint()).header("Authorization", self.auth());
        if let Some(revision) = revision {
            request = request.header("If-None-Match", format!("\"{revision}\""));
        }
        let mut response = request.call().map_err(network)?;
        if response.status().as_u16() == 304 {
            return Ok(None);
        }
        check(&response)?;
        response
            .body_mut()
            .with_config()
            .limit(32 * 1024 * 1024 + 1024)
            .read_json()
            .map(Some)
            .map_err(|_| Error::Message("服务器文档响应无效".into()))
    }
    pub fn put(&self, revision: u64, document: &Base) -> Result<Snapshot> {
        let mut response = self
            .agent()
            .put(self.endpoint())
            .header("Authorization", self.auth())
            .send_json(serde_json::json!({"expected_revision":revision,"document":document}))
            .map_err(network)?;
        check(&response)?;
        response
            .body_mut()
            .with_config()
            .limit(32 * 1024 * 1024 + 1024)
            .read_json()
            .map_err(|_| Error::Message("服务器保存响应无效".into()))
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
pub fn merge(base: &Base, local: &Base, remote: &Base) -> Result<Base> {
    let value = rutable_model::merge::merge(
        &serde_json::to_value(base).unwrap(),
        &serde_json::to_value(local).unwrap(),
        &serde_json::to_value(remote).unwrap(),
    )
    .map_err(|e| Error::Message(format!("{e}；修改仍在内存中，请处理后重试或重新读取服务器")))?;
    serde_json::from_value(value).map_err(|_| Error::Message("合并后的文档无效".into()))
}
pub enum Outcome {
    Unchanged,
    Loaded(Snapshot),
}
pub fn sync(config: &Config, baseline: Option<&Snapshot>, local: &Base, dirty: bool) -> Result<Outcome> {
    if !dirty {
        return config.get(baseline.map(|s| s.revision)).map(|s| s.map(Outcome::Loaded).unwrap_or(Outcome::Unchanged));
    }
    let baseline = baseline.ok_or_else(|| Error::Message("尚未读取服务器文档".into()))?;
    for _ in 0..3 {
        let remote = config.get(None)?.ok_or_else(|| Error::Message("服务器没有返回文档".into()))?;
        let merged = merge(&baseline.document, local, &remote.document)?;
        if serde_json::to_value(&merged).unwrap() == serde_json::to_value(&remote.document).unwrap() {
            return Ok(Outcome::Loaded(remote));
        }
        match config.put(remote.revision, &merged) {
            Ok(snapshot) => return Ok(Outcome::Loaded(snapshot)),
            Err(Error::Conflict) => continue,
            Err(e) => return Err(e),
        }
    }
    Err(Error::Conflict)
}
