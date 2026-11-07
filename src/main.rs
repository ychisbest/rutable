//! Rutable: a multi-dimensional table (多维表格) for Windows, built with NSH.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#![allow(dead_code)]

mod args;
mod api;
mod login;
use rutable_model::model;
mod ui;
mod win;

fn main() {
    // A crash leaves its reason in %LOCALAPPDATA%\Rutable\crash.log (the window has no console).
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(dir) = std::env::var_os("LOCALAPPDATA").map(|d| std::path::PathBuf::from(d).join("Rutable")) {
            let _ = std::fs::create_dir_all(&dir);
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("crash.log")) {
                use std::io::Write;
                let _ = writeln!(f, "{} {info}\n{}", model::now_ms(), std::backtrace::Backtrace::force_capture());
            }
        }
        default(info);
    }));
    win::run();
}
