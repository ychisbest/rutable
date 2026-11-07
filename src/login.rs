//! What this computer remembers: the server address, and for that server the
//! credential it issued for the password (never the password itself),
//! encrypted with Windows DPAPI so only this Windows account can read it.

use std::path::PathBuf;

use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Cryptography::{CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData};

fn dir() -> Option<PathBuf> {
    Some(std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?.join("Rutable"))
}

/// The server address last logged in to (plain text: it is no secret).
pub fn saved_server() -> Option<String> {
    let s = std::fs::read_to_string(dir()?.join("server.txt")).ok()?;
    Some(s.trim().to_string()).filter(|s| !s.is_empty())
}

pub fn save_server(url: &str) {
    let Some(d) = dir() else { return };
    let _ = std::fs::create_dir_all(&d);
    let _ = std::fs::write(d.join("server.txt"), url.trim().trim_end_matches('/'));
}

fn path(url: &str) -> Option<PathBuf> {
    let dir = dir()?;
    let mut h: u64 = 0xcbf29ce484222325;
    for b in url.trim_end_matches('/').bytes() {
        h = (h ^ b as u64).wrapping_mul(0x100000001b3);
    }
    Some(dir.join(format!("login-{h:016x}.bin")))
}

fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 }
}

fn take(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
    let v = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
    unsafe {
        let _ = LocalFree(Some(HLOCAL(out.pbData as _)));
    }
    v
}

fn protect(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = CRYPT_INTEGER_BLOB::default();
    unsafe { CryptProtectData(&blob(data), windows::core::w!("Rutable login"), None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out) }.ok()?;
    Some(take(out))
}

fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = CRYPT_INTEGER_BLOB::default();
    unsafe { CryptUnprotectData(&blob(data), None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out) }.ok()?;
    Some(take(out))
}

pub fn load(url: &str) -> Option<String> {
    let sealed = std::fs::read(path(url)?).ok()?;
    String::from_utf8(unprotect(&sealed)?).ok().filter(|t| !t.is_empty())
}

pub fn save(url: &str, token: &str) {
    let (Some(p), Some(sealed)) = (path(url), protect(token.as_bytes())) else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(p, sealed);
}

pub fn forget(url: &str) {
    if let Some(p) = path(url) {
        let _ = std::fs::remove_file(p);
    }
}
