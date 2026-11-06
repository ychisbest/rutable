//! Rutable: a multi-dimensional table (多维表格) for Windows, built with NSH.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#![allow(dead_code)]

mod args;
mod model;
mod ui;
mod win;

fn main() {
    win::run();
}
