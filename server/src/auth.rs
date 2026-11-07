//! Access: the server key (RUTABLE_API_TOKEN, needed once to set the
//! password), the password (argon2), and the credentials handed out for it —
//! a login per device, and keys for AI tools (MCP), each revocable. Only
//! SHA-256 hashes of credentials are stored.

use super::*;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use sha2::{Digest, Sha256};
use std::time::Instant;

const SESSION_DAYS: i64 = 180;
const DAY_MS: i64 = 86_400_000;

/// Wrong passwords: after 5 in a row, logins wait 30 seconds.
#[derive(Default)]
pub struct Limiter {
    fails: u32,
    until: Option<Instant>,
}

impl Limiter {
    fn check(&mut self) -> Result<()> {
        if let Some(t) = self.until {
            if Instant::now() < t {
                let s = (t - Instant::now()).as_secs() + 1;
                return Err(error(StatusCode::TOO_MANY_REQUESTS, format!("密码错误次数过多，请 {s} 秒后再试")));
            }
            self.until = None;
        }
        Ok(())
    }
    fn failed(&mut self) {
        self.fails += 1;
        if self.fails >= 5 {
            self.fails = 0;
            self.until = Some(Instant::now() + Duration::from_secs(30));
        }
    }
    fn ok(&mut self) {
        self.fails = 0;
    }
}

fn hash(secret: &str) -> String {
    Sha256::digest(secret.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

fn bearer(headers: &HeaderMap) -> &str {
    headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).unwrap_or("").trim()
}

fn password_hash(conn: &Connection) -> Result<Option<String>> {
    Ok(conn.query_row("SELECT value FROM settings WHERE key='password'", [], |r| r.get(0)).optional()?)
}

/// A new credential; its secret is returned once and only its hash kept.
pub(crate) fn issue(conn: &Connection, kind: &str, name: &str) -> Result<(String, String)> {
    let prefix = if kind == "api" { "rtk" } else { "rts" };
    let secret = format!("{prefix}_{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
    let now = rutable_model::now_ms();
    let expires = (kind == "session").then_some(now + SESSION_DAYS * DAY_MS);
    conn.execute(
        "INSERT INTO credentials(id,hash,kind,name,created_at,last_used,expires_at) VALUES (?1,?2,?3,?4,?5,?5,?6)",
        params![id, hash(&secret), kind, name.chars().take(60).collect::<String>(), now, expires],
    )?;
    Ok((id, secret))
}

/// Every API call: the server key, or a live credential.
pub async fn require(State(state): State<App>, request: axum::extract::Request, next: Next) -> Response {
    let provided = bearer(request.headers()).to_string();
    if !provided.is_empty() && same(&provided, &state.token) {
        return next.run(request).await;
    }
    if provided.starts_with("rts_") || provided.starts_with("rtk_") {
        let h = hash(&provided);
        let ok = db(state.clone(), move |store| {
            let now = rutable_model::now_ms();
            let n = store.conn.execute(
                "UPDATE credentials SET last_used=?2 WHERE hash=?1 AND (expires_at IS NULL OR expires_at>?2)",
                params![h, now],
            )?;
            Ok(n > 0)
        })
        .await
        .unwrap_or(false);
        if ok {
            return next.run(request).await;
        }
    }
    error(StatusCode::UNAUTHORIZED, "unauthorized").into_response()
}

pub async fn status(State(state): State<App>) -> Result<Json<serde_json::Value>> {
    let set = db(state, |store| Ok(password_hash(&store.conn)?.is_some())).await?;
    Ok(Json(serde_json::json!({"password_set": set})))
}

fn check_password(p: &str) -> Result<()> {
    if p.chars().count() < 8 || p.len() > 200 {
        return Err(error(StatusCode::BAD_REQUEST, "密码至少 8 个字符"));
    }
    Ok(())
}

async fn hash_password(p: String) -> Result<String> {
    tokio::task::spawn_blocking(move || {
        // 16 random bytes (uuid v4 draws them from the OS).
        let salt = SaltString::encode_b64(uuid::Uuid::new_v4().as_bytes()).map_err(|_| argon2::password_hash::Error::SaltInvalid(argon2::password_hash::errors::InvalidValue::Malformed))?;
        Argon2::default().hash_password(p.as_bytes(), &salt).map(|h| h.to_string())
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "密码处理失败"))?
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "密码处理失败"))
}

async fn verify(state: &App, password: String) -> Result<()> {
    state.logins.lock().map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "锁异常"))?.check()?;
    let stored = db(state.clone(), |store| password_hash(&store.conn)).await?.ok_or_else(|| error(StatusCode::CONFLICT, "尚未设置密码"))?;
    let ok = tokio::task::spawn_blocking(move || {
        PasswordHash::new(&stored).map(|h| Argon2::default().verify_password(password.as_bytes(), &h).is_ok()).unwrap_or(false)
    })
    .await
    .unwrap_or(false);
    let mut limiter = state.logins.lock().map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "锁异常"))?;
    if ok {
        limiter.ok();
        Ok(())
    } else {
        limiter.failed();
        Err(error(StatusCode::UNAUTHORIZED, "密码错误"))
    }
}

#[derive(Deserialize)]
pub struct Setup {
    pub setup_key: String,
    pub password: String,
    pub device: Option<String>,
}

/// The first password. The server key proves this is the server's owner,
/// so a stranger reaching a fresh server first cannot claim it.
pub async fn setup(State(state): State<App>, Json(req): Json<Setup>) -> Result<Json<serde_json::Value>> {
    check_password(&req.password)?;
    state.logins.lock().map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "锁异常"))?.check()?;
    if !same(req.setup_key.trim(), &state.token) {
        state.logins.lock().map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "锁异常"))?.failed();
        return Err(error(StatusCode::UNAUTHORIZED, "服务器密钥不正确"));
    }
    let h = hash_password(req.password).await?;
    let device = req.device.unwrap_or_else(|| "首次设置".into());
    db(state, move |store| {
        let tx = store.conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if password_hash(&tx)?.is_some() {
            return Err(error(StatusCode::CONFLICT, "密码已设置，请直接登录"));
        }
        tx.execute("INSERT INTO settings(key,value) VALUES ('password',?1)", [h])?;
        let (_, token) = issue(&tx, "session", &device)?;
        tx.commit()?;
        Ok(Json(serde_json::json!({"token": token})))
    })
    .await
}

#[derive(Deserialize)]
pub struct Login {
    pub password: String,
    pub device: Option<String>,
}

pub async fn login(State(state): State<App>, Json(req): Json<Login>) -> Result<Json<serde_json::Value>> {
    verify(&state, req.password).await?;
    let device = req.device.unwrap_or_else(|| "设备".into());
    db(state, move |store| {
        let (_, token) = issue(&store.conn, "session", &device)?;
        Ok(Json(serde_json::json!({"token": token, "expires_days": SESSION_DAYS})))
    })
    .await
}

pub async fn logout(State(state): State<App>, headers: HeaderMap) -> Result<StatusCode> {
    let h = hash(bearer(&headers));
    db(state, move |store| {
        store.conn.execute("DELETE FROM credentials WHERE hash=?1 AND kind='session'", [h])?;
        Ok(StatusCode::NO_CONTENT)
    })
    .await
}

#[derive(Deserialize)]
pub struct ChangePassword {
    pub current: String,
    pub new: String,
}

/// A new password; every other device's login ends, AI keys stay.
pub async fn change_password(State(state): State<App>, headers: HeaderMap, Json(req): Json<ChangePassword>) -> Result<StatusCode> {
    check_password(&req.new)?;
    verify(&state, req.current).await?;
    let h = hash_password(req.new).await?;
    let mine = hash(bearer(&headers));
    db(state, move |store| {
        store.conn.execute("UPDATE settings SET value=?1 WHERE key='password'", [h])?;
        store.conn.execute("DELETE FROM credentials WHERE kind='session' AND hash<>?1", [mine])?;
        Ok(StatusCode::NO_CONTENT)
    })
    .await
}

pub async fn list_tokens(State(state): State<App>) -> Result<Json<Vec<serde_json::Value>>> {
    db(state, |store| {
        let mut q = store.conn.prepare("SELECT id,name,created_at,last_used FROM credentials WHERE kind='api' ORDER BY created_at DESC")?;
        let rows = q.query_map([], |r| {
            Ok(serde_json::json!({"id": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)?, "created_at": r.get::<_, i64>(2)?, "last_used": r.get::<_, i64>(3)?}))
        })?;
        Ok(Json(rows.collect::<std::result::Result<_, _>>()?))
    })
    .await
}

#[derive(Deserialize)]
pub struct NewToken {
    pub name: Option<String>,
}

pub async fn create_token(State(state): State<App>, Json(req): Json<NewToken>) -> Result<Json<serde_json::Value>> {
    db(state, move |store| {
        let name = req.name.unwrap_or_else(|| "AI".into());
        let (id, token) = issue(&store.conn, "api", &name)?;
        Ok(Json(serde_json::json!({"id": id, "name": name, "token": token})))
    })
    .await
}

pub async fn delete_token(State(state): State<App>, Path(id): Path<String>) -> Result<StatusCode> {
    db(state, move |store| {
        let n = store.conn.execute("DELETE FROM credentials WHERE id=?1 AND kind='api'", [id])?;
        Ok(if n > 0 { StatusCode::NO_CONTENT } else { StatusCode::NOT_FOUND })
    })
    .await
}
