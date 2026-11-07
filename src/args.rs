//! Command line: `--automation [port]` opens the automation port,
//! `--data <file>` picks the document, `--maximized` maximizes the window.

fn args() -> Vec<String> {
    std::env::args().collect()
}

fn value(name: &str) -> Option<String> {
    let a = args();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

pub fn automation_port() -> Option<u16> {
    let a = args();
    let i = a.iter().position(|x| x == "--automation")?;
    Some(a.get(i + 1).and_then(|p| p.parse().ok()).unwrap_or(47800))
}

pub fn maximized() -> bool {
    args().iter().any(|x| x == "--maximized")
}

/// The server: `--api`, else the address last logged in to on this computer,
/// else `RUTABLE_API_URL`, else the public server. (Documents themselves are
/// never kept locally.)
pub fn api_url() -> String {
    value("--api")
        .or_else(crate::login::saved_server)
        .or_else(|| std::env::var("RUTABLE_API_URL").ok())
        .unwrap_or("https://rutable.ych.show".into())
}
pub fn document_id() -> String {value("--document").or_else(||std::env::var("RUTABLE_DOCUMENT_ID").ok()).unwrap_or("default".into())}
/// A base named on the command line (scripts, tests): opened straight away.
/// Only the argument counts — an old RUTABLE_DOCUMENT_ID in the environment
/// must not skip the list of bases or the password.
pub fn explicit_document() -> Option<String> {value("--document")}
