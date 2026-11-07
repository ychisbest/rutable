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

/// Connection settings are read from arguments / environment, never a local data file.
pub fn api_url() -> String {value("--api").or_else(||std::env::var("RUTABLE_API_URL").ok()).unwrap_or("https://rutable.ych.show".into())}
pub fn document_id() -> String {value("--document").or_else(||std::env::var("RUTABLE_DOCUMENT_ID").ok()).unwrap_or("default".into())}
