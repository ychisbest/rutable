//! Colors, fonts and sizes of the app.

pub const PRIMARY: u32 = 0x3370FF;
pub const PRIMARY_HOVER: u32 = 0x4E83FD;
pub const PRIMARY_LIGHT: u32 = 0xE1EAFF;
pub const TEXT: u32 = 0x1F2329;
pub const TEXT2: u32 = 0x646A73;
pub const TEXT3: u32 = 0x8F959E;
pub const BORDER: u32 = 0xDEE0E3;
pub const LINE: u32 = 0xE4E5E7;
pub const BG: u32 = 0xFFFFFF;
pub const BG_GREY: u32 = 0xF5F6F7;
pub const BG_SIDEBAR: u32 = 0xF8F9FA;
pub const HOVER: u32 = 0xF2F3F5;
pub const ACTIVE: u32 = 0xE1EAFF;
pub const DANGER: u32 = 0xF54A45;
pub const SUCCESS: u32 = 0x34C724;

pub const FONT: &str = "Microsoft YaHei UI";

pub fn font(size: f32) -> String {
    format!("{size}px {FONT}")
}

pub fn bold(size: f32) -> String {
    format!("600 {size}px {FONT}")
}

pub const TOPBAR_H: f32 = 52.0;
pub const SIDEBAR_W: f32 = 240.0;
pub const TABS_H: f32 = 44.0;
pub const TOOLBAR_H: f32 = 44.0;
