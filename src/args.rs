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

pub fn data_file() -> Option<String> {
    value("--data")
}

pub fn maximized() -> bool {
    args().iter().any(|x| x == "--maximized")
}

/// Starts from an empty document instead of the sample one.
pub fn fresh() -> bool {
    args().iter().any(|x| x == "--fresh")
}
