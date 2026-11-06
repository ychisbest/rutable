//! Small shared pieces: buttons, menus, checkboxes, tags, avatars, a
//! calendar, switches, and the system's file dialogs.

use chrono::Datelike;
use nsh_core::prelude::*;

use super::{icons, theme::*};
use crate::model::value::{self, strong_color};

pub fn check_box(on: bool) -> impl IntoElement {
    view()
        .size(px(16.0))
        .rounded(px(3.0))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .bg(if on { rgb(PRIMARY) } else { rgb(BG) })
        .border_1()
        .border_color(if on { rgb(PRIMARY) } else { rgb(0xBBBFC4) })
        .child(if on { icons::icon("check", 12.0, 0xFFFFFF).into_element() } else { view().into_element() })
}

pub fn check_box_dyn(on: impl Fn() -> bool + Clone + 'static) -> impl IntoElement {
    let (on1, on2, on3) = (on.clone(), on.clone(), on);
    view()
        .size(px(16.0))
        .rounded(px(3.0))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .bg(move || if on1() { rgb(PRIMARY) } else { rgb(BG) })
        .border_1()
        .border_color(move || if on2() { rgb(PRIMARY) } else { rgb(0xBBBFC4) })
        .child(icons::icon_dyn(|| "check", 12.0, move || if on3() { 0xFFFFFF } else { BG }))
}

pub fn tag_el(name: &str, bg: u32, fg: u32) -> impl IntoElement {
    view()
        .h(px(22.0))
        .px(px(8.0))
        .rounded(px(11.0))
        .bg(rgb(bg))
        .justify_center()
        .flex_shrink(1.0)
        .child(text(name.to_string()).text_size(px(12.0)).text_color(rgb(fg)).number_of_lines(1u32))
}

pub fn avatar_el(name: &str, color: u8, size_pt: f32) -> impl IntoElement {
    let initial: String = name.chars().last().map(|c| c.to_string()).unwrap_or_default();
    view()
        .size(px(size_pt))
        .rounded(px(size_pt / 2.0))
        .bg(rgb(strong_color(color)))
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .child(text(initial).text_size(px(size_pt * 0.5)).text_color(rgb(0xFFFFFF)))
}

pub fn link_button(label: &'static str, on: impl Fn() + 'static) -> impl IntoElement {
    view()
        .px(px(6.0))
        .py(px(4.0))
        .rounded(px(4.0))
        .hover(|s| s.bg(rgb(HOVER)))
        .on_press(on)
        .child(text(label).text_size(px(13.0)).text_color(rgb(PRIMARY)))
}

/// A primary (blue) button.
pub fn primary_button(label: impl Into<String>, on: impl Fn() + 'static) -> impl IntoElement {
    view()
        .h(px(32.0))
        .px(px(14.0))
        .rounded(px(6.0))
        .bg(rgb(PRIMARY))
        .hover(|s| s.bg(rgb(PRIMARY_HOVER)))
        .active(|s| s.opacity(0.85))
        .flex_row()
        .items_center()
        .justify_center()
        .gap(px(4.0))
        .on_press(on)
        .child(text(label.into()).text_size(px(14.0)).text_color(rgb(0xFFFFFF)))
}

/// A secondary (outlined) button.
pub fn button(label: impl Into<String>, on: impl Fn() + 'static) -> impl IntoElement {
    view()
        .h(px(32.0))
        .px(px(14.0))
        .rounded(px(6.0))
        .bg(rgb(BG))
        .border_1()
        .border_color(rgb(BORDER))
        .hover(|s| s.bg(rgb(HOVER)))
        .flex_row()
        .items_center()
        .justify_center()
        .on_press(on)
        .child(text(label.into()).text_size(px(14.0)).text_color(rgb(TEXT)))
}

/// Runs `f` with the bottom left of the element `at` in the window.
pub fn below(at: ElementRef, f: impl Fn(f32, f32) + 'static) {
    spawn(async move {
        if let Some(m) = at.measure().await {
            f(m.page.x.0, m.page.y.0 + m.frame.size.height.0);
        }
    });
}

/// A toolbar's icon + label button; `on` gets the point under it.
pub fn tool_button(
    icon: &'static str,
    label: impl Fn() -> String + 'static,
    active: impl Fn() -> bool + Copy + 'static,
    on: impl Fn(f32, f32) + Clone + 'static,
) -> impl IntoElement {
    let at = element_ref();
    view()
        .element_ref(&at)
        .h(px(30.0))
        .px(px(8.0))
        .rounded(px(6.0))
        .flex_row()
        .items_center()
        .gap(px(4.0))
        .bg(move || if active() { rgb(0xE1EAFF) } else { rgba(0xFFFFFF00) })
        .hover(|s| s.bg(rgb(HOVER)))
        .on_press(move || below(at, on.clone()))
        .child(icons::icon_dyn(move || icon, 16.0, move || if active() { PRIMARY } else { TEXT2 }))
        .child(text(label).text_size(px(14.0)).text_color(move || if active() { rgb(PRIMARY) } else { rgb(TEXT) }))
}

pub fn menu_item(icon: &'static str, label: impl Into<String>, on: impl Fn() + 'static) -> impl IntoElement {
    menu_item_full(icon, label.into(), String::new(), false, on)
}

pub fn menu_item_danger(icon: &'static str, label: impl Into<String>, on: impl Fn() + 'static) -> impl IntoElement {
    menu_item_full(icon, label.into(), String::new(), true, on)
}

pub fn menu_item_full(icon: &'static str, label: String, hint: String, danger: bool, on: impl Fn() + 'static) -> impl IntoElement {
    let color = if danger { DANGER } else { TEXT };
    view()
        .h(px(34.0))
        .mx(px(4.0))
        .px(px(10.0))
        .rounded(px(6.0))
        .flex_row()
        .items_center()
        .gap(px(10.0))
        .hover(|s| s.bg(rgb(HOVER)))
        .on_press(on)
        .child(if icon.is_empty() { view().w(px(0.0)).into_element() } else { icons::icon(icon, 16.0, if danger { DANGER } else { TEXT2 }).into_element() })
        .child(text(label).text_size(px(14.0)).text_color(rgb(color)).flex_1().number_of_lines(1u32))
        .child(text(hint).text_size(px(12.0)).text_color(rgb(TEXT3)))
}

pub fn separator() -> impl IntoElement {
    view().h(px(1.0)).mx(px(8.0)).my(px(4.0)).bg(rgb(LINE))
}

pub fn menu_title(label: impl Into<String>) -> impl IntoElement {
    view().px(px(14.0)).pt(px(8.0)).pb(px(4.0)).child(text(label.into()).text_size(px(12.0)).text_color(rgb(TEXT3)))
}

/// A popup surface at (x, y) of the window, closed by a press outside or
/// Escape.
pub fn popover_at(x: f32, y: f32, width: f32, close: impl Fn() + Clone + 'static, content: impl IntoElement) -> Overlay {
    let c2 = close.clone();
    overlay()
        .anchor_point(move || point(px(x), px(y)))
        .placement(Placement::Bottom)
        .align(Align::Start)
        .offset(px(4.0))
        .margin(px(8.0))
        .on_press_outside(move |_: &Point<Points>| close())
        .on_cancel(move || c2())
        .w(px(width))
        .max_h(px(620.0))
        .bg(rgb(BG))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgb(BORDER))
        .shadow_lg()
        .py(px(4.0))
        .child(content)
}

/// A field-like text input.
pub fn text_field(value: Signal<String>, placeholder: &'static str) -> Input {
    nsh_core::element::input()
        .h(px(32.0))
        .px(px(10.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(rgb(BORDER))
        .text_size(px(14.0))
        .text_color(rgb(TEXT))
        .placeholder(placeholder)
        .placeholder_color(rgb(TEXT3))
        .value(move || value.get())
        .on_change(move |t: &str| value.set(t.to_string()))
}

pub fn switch(on: impl Fn() -> bool + Copy + 'static, toggle: impl Fn() + 'static) -> impl IntoElement {
    view()
        .w(px(28.0))
        .h(px(16.0))
        .rounded(px(8.0))
        .bg(move || if on() { rgb(PRIMARY) } else { rgb(0xBBBFC4) })
        .on_press(toggle)
        .child(
            view()
                .absolute()
                .top(px(2.0))
                .left(move || px(if on() { 14.0 } else { 2.0 }))
                .size(px(12.0))
                .rounded(px(6.0))
                .bg(rgb(0xFFFFFF)),
        )
}

/// A small drop-down: the current label; a press lists `options`.
pub fn dropdown(label: impl Fn() -> String + 'static, options: impl Fn() -> Vec<String> + 'static, pick: impl Fn(usize) + Clone + 'static, width: f32) -> impl IntoElement {
    let open = signal(false);
    let at = element_ref();
    let options = std::rc::Rc::new(options);
    view()
        .child(
            view()
                .element_ref(&at)
                .h(px(32.0))
                .w(px(width))
                .px(px(10.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(move || if open.get() { rgb(PRIMARY) } else { rgb(BORDER) })
                .bg(rgb(BG))
                .flex_row()
                .items_center()
                .on_press(move || open.update(|o| *o = !*o))
                .child(text(label).text_size(px(14.0)).text_color(rgb(TEXT)).flex_1().number_of_lines(1u32))
                .child(icons::icon("chevron-down", 14.0, TEXT3)),
        )
        .child(when(move || open.get(), move || {
            let options = options.clone();
            let pick = pick.clone();
            overlay()
                .anchor(&at)
                .placement(Placement::Bottom)
                .align(Align::Start)
                .offset(px(4.0))
                .margin(px(8.0))
                .on_press_outside(move |_: &Point<Points>| open.set(false))
                .on_cancel(move || open.set(false))
                .min_w(px(width))
                .bg(rgb(BG))
                .rounded(px(8.0))
                .border_1()
                .border_color(rgb(BORDER))
                .shadow_lg()
                .py(px(4.0))
                .child(scroll().max_h(px(300.0)).child(
                    options()
                        .into_iter()
                        .enumerate()
                        .map(|(i, o)| {
                            let pick = pick.clone();
                            view()
                                .h(px(32.0))
                                .mx(px(4.0))
                                .px(px(10.0))
                                .rounded(px(6.0))
                                .justify_center()
                                .hover(|s| s.bg(rgb(HOVER)))
                                .on_press(move || {
                                    open.set(false);
                                    pick(i)
                                })
                                .child(text(o).text_size(px(14.0)).text_color(rgb(TEXT)).number_of_lines(1u32))
                                .into_element()
                        })
                        .collect::<Vec<_>>(),
                ))
        }))
}

/// A month of days; `pick` gets the chosen day's start.
pub fn calendar(month_of: i64, selected: Option<i64>, pick: impl Fn(i64) + Clone + 'static) -> impl IntoElement {
    let m0 = value::local(month_of);
    let month = signal(m0.year() * 12 + m0.month() as i32 - 1);
    let sel_day = selected.map(value::day_start);
    view()
        .p(px(10.0))
        .gap(px(6.0))
        .child(
            view()
                .flex_row()
                .items_center()
                .child(view().p(px(4.0)).rounded(px(4.0)).hover(|s| s.bg(rgb(HOVER))).on_press(move || month.update(|m| *m -= 1)).child(icons::icon("chevron-left", 16.0, TEXT2)))
                .child(
                    text(move || {
                        let m = month.get();
                        format!("{} 年 {} 月", m.div_euclid(12), m.rem_euclid(12) + 1)
                    })
                    .flex_1()
                    .text_center()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(TEXT)),
                )
                .child(view().p(px(4.0)).rounded(px(4.0)).hover(|s| s.bg(rgb(HOVER))).on_press(move || month.update(|m| *m += 1)).child(icons::icon("chevron-right", 16.0, TEXT2))),
        )
        .child(
            view().flex_row().child(
                ["一", "二", "三", "四", "五", "六", "日"]
                    .iter()
                    .map(|d| view().flex_1().items_center().py(px(4.0)).child(text(*d).text_size(px(12.0)).text_color(rgb(TEXT3))).into_element())
                    .collect::<Vec<_>>(),
            ),
        )
        .child(dynamic(move || {
            let m = month.get();
            let (y, mo) = (m.div_euclid(12), m.rem_euclid(12) as u32 + 1);
            let first = value::date_ms(y, mo, 1);
            let lead = value::local(first).weekday().num_days_from_monday() as i64;
            let start = first - lead * value::DAY_MS;
            let today = value::today_ms();
            let mut weeks = vec![];
            for wk in 0..6 {
                let mut days = vec![];
                for d in 0..7 {
                    let day = value::day_start(start + (wk * 7 + d) as i64 * value::DAY_MS + 3_600_000 * 3);
                    let dt = value::local(day);
                    let in_month = dt.month() == mo;
                    let is_sel = sel_day == Some(day);
                    let is_today = day == today;
                    let pick = pick.clone();
                    days.push(
                        view()
                            .flex_1()
                            .items_center()
                            .child(
                                view()
                                    .size(px(30.0))
                                    .rounded(px(6.0))
                                    .items_center()
                                    .justify_center()
                                    .bg(if is_sel { rgb(PRIMARY) } else { rgb(BG) })
                                    .border(px(if is_today && !is_sel { 1.0 } else { 0.0 }))
                                    .border_color(rgb(PRIMARY))
                                    .hover(|s| s.bg(rgb(if is_sel { PRIMARY } else { HOVER })))
                                    .on_press(move || pick(day))
                                    .child(
                                        text(dt.day().to_string())
                                            .text_size(px(13.0))
                                            .text_color(rgb(if is_sel {
                                                0xFFFFFF
                                            } else if is_today {
                                                PRIMARY
                                            } else if in_month {
                                                TEXT
                                            } else {
                                                0xC9CDD4
                                            })),
                                    ),
                            )
                            .into_element(),
                    );
                }
                weeks.push(view().flex_row().child(days).into_element());
            }
            weeks
        }))
}

// ---------------------------------------------------------------------------
// The system's file dialogs.

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Asks for files to open; empty when cancelled.
pub async fn open_files_dialog() -> Vec<String> {
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || {
        use windows::Win32::UI::Controls::Dialogs::*;
        let mut buf = vec![0u16; 32768];
        let filter = wide("所有文件\0*.*\0图片\0*.png;*.jpg;*.jpeg;*.gif;*.webp;*.bmp\0\0");
        let title = wide("选择附件");
        let mut ofn = OPENFILENAMEW {
            lStructSize: size_of::<OPENFILENAMEW>() as u32,
            lpstrFilter: windows::core::PCWSTR(filter.as_ptr()),
            lpstrFile: windows::core::PWSTR(buf.as_mut_ptr()),
            nMaxFile: buf.len() as u32,
            lpstrTitle: windows::core::PCWSTR(title.as_ptr()),
            Flags: OFN_ALLOWMULTISELECT | OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_NOCHANGEDIR,
            ..Default::default()
        };
        let ok = unsafe { GetOpenFileNameW(&mut ofn) }.as_bool();
        let mut out = vec![];
        if ok {
            let parts: Vec<String> = buf.split(|c| *c == 0).take_while(|p| !p.is_empty()).map(String::from_utf16_lossy).collect();
            if parts.len() == 1 {
                out.push(parts[0].clone());
            } else if parts.len() > 1 {
                for f in &parts[1..] {
                    out.push(format!("{}\\{}", parts[0], f));
                }
            }
        }
        let _ = tx.send_blocking(out);
    });
    rx.recv().await.unwrap_or_default()
}

/// Asks where to save a file named `name`; `None` when cancelled.
pub async fn save_file_dialog(name: String, filter_name: &'static str, ext: &'static str) -> Option<String> {
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || {
        use windows::Win32::UI::Controls::Dialogs::*;
        let mut buf = vec![0u16; 4096];
        for (i, c) in name.encode_utf16().enumerate().take(4000) {
            buf[i] = c;
        }
        let filter = wide(&format!("{filter_name}\0*.{ext}\0\0"));
        let def = wide(ext);
        let mut ofn = OPENFILENAMEW {
            lStructSize: size_of::<OPENFILENAMEW>() as u32,
            lpstrFilter: windows::core::PCWSTR(filter.as_ptr()),
            lpstrFile: windows::core::PWSTR(buf.as_mut_ptr()),
            nMaxFile: buf.len() as u32,
            lpstrDefExt: windows::core::PCWSTR(def.as_ptr()),
            Flags: OFN_OVERWRITEPROMPT | OFN_NOCHANGEDIR,
            ..Default::default()
        };
        let ok = unsafe { GetSaveFileNameW(&mut ofn) }.as_bool();
        let out = ok.then(|| String::from_utf16_lossy(&buf[..buf.iter().position(|c| *c == 0).unwrap_or(0)]));
        let _ = tx.send_blocking(out);
    });
    rx.recv().await.ok().flatten()
}

pub async fn open_one_dialog(filter_name: &'static str, ext: &'static str) -> Option<String> {
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || {
        use windows::Win32::UI::Controls::Dialogs::*;
        let mut buf = vec![0u16; 4096];
        let filter = wide(&format!("{filter_name}\0{ext}\0\0"));
        let mut ofn = OPENFILENAMEW {
            lStructSize: size_of::<OPENFILENAMEW>() as u32,
            lpstrFilter: windows::core::PCWSTR(filter.as_ptr()),
            lpstrFile: windows::core::PWSTR(buf.as_mut_ptr()),
            nMaxFile: buf.len() as u32,
            Flags: OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_NOCHANGEDIR,
            ..Default::default()
        };
        let ok = unsafe { GetOpenFileNameW(&mut ofn) }.as_bool();
        let out = ok.then(|| String::from_utf16_lossy(&buf[..buf.iter().position(|c| *c == 0).unwrap_or(0)]));
        let _ = tx.send_blocking(out);
    });
    rx.recv().await.ok().flatten()
}

/// Opens a file or a web address with the system.
pub fn shell_open(target: &str) {
    use windows::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
    let t = wide(target);
    let op = wide("open");
    unsafe {
        ShellExecuteW(None, windows::core::PCWSTR(op.as_ptr()), windows::core::PCWSTR(t.as_ptr()), None, None, SW_SHOWNORMAL);
    }
}
