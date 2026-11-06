//! The Win32 side: the window around NSH's view, the message loop, the keys
//! and wheel turns the table takes before NSH sees them, and the automation
//! port (`--automation`) a driver uses to read the app's state.

use std::{
    io::{BufRead, BufReader, Write as _},
    sync::{
        OnceLock,
        atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU8, AtomicU32, Ordering},
    },
};

use nsh_platform_windows::{App, AppOptions, AppView, Appearance, Root};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, POINT, WPARAM},
        Graphics::Gdi::ScreenToClient,
        System::{Ole::OleInitialize, Threading::GetCurrentThreadId},
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext},
            Input::KeyboardAndMouse::*,
            WindowsAndMessaging::*,
        },
    },
    core::{HSTRING, w},
};

/// What the keyboard goes to.
pub mod mode {
    /// Everything goes to NSH (a text field of a dialog or a toolbar has it).
    pub const FREE: u8 = 0;
    /// The grid has a selected cell and is not editing: navigation keys are
    /// the grid's, typed text goes to the hidden editor (which starts editing).
    pub const GRID: u8 = 1;
    /// A cell is being edited: Enter, Tab and Escape are the grid's.
    pub const EDITING: u8 = 2;
    /// A popup list (a select's options, a menu) has the arrows and Enter.
    pub const POPUP: u8 = 3;
    /// A cell editor that is not text (a date, an option list): every key
    /// but typed text is the grid's.
    pub const EDITING_LIST: u8 = 4;
}

static HWND_RAW: AtomicIsize = AtomicIsize::new(0);
static UI_THREAD: AtomicU32 = AtomicU32::new(0);
static KEY_MODE: AtomicU8 = AtomicU8::new(mode::FREE);
/// Shift+Enter / Alt+Enter make a new line while editing (multi-line text).
static MULTILINE: AtomicBool = AtomicBool::new(false);
/// The grid's box in the window's client area, in points (x, y, w, h); the
/// wheel over it scrolls the grid.
static GRID_RECT: [AtomicI32; 4] = [AtomicI32::new(0), AtomicI32::new(0), AtomicI32::new(0), AtomicI32::new(0)];
/// Wheel turns go to the grid (no popup is open over it).
static GRID_WHEEL: AtomicBool = AtomicBool::new(false);

const WM_APP_QUIT: u32 = WM_APP + 1;

/// A key the window thread took for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    pub vk: u16,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

/// What the window thread and the automation port tell the UI.
#[derive(Debug)]
pub enum Cmd {
    Key(Key),
    /// The wheel over the grid: (dx, dy) in points.
    Wheel(f32, f32),
    /// An automation request; the answer goes back on the channel.
    Automation(String, std::sync::mpsc::Sender<String>),
}

fn channel() -> &'static (async_channel::Sender<Cmd>, async_channel::Receiver<Cmd>) {
    static CH: OnceLock<(async_channel::Sender<Cmd>, async_channel::Receiver<Cmd>)> = OnceLock::new();
    CH.get_or_init(async_channel::unbounded)
}

pub fn send(cmd: Cmd) {
    let _ = channel().0.try_send(cmd);
}

pub fn commands() -> async_channel::Receiver<Cmd> {
    channel().1.clone()
}

pub fn set_key_mode(m: u8) {
    KEY_MODE.store(m, Ordering::Relaxed);
}

pub fn key_mode() -> u8 {
    KEY_MODE.load(Ordering::Relaxed)
}

pub fn set_multiline(on: bool) {
    MULTILINE.store(on, Ordering::Relaxed);
}

pub fn set_grid_rect(x: f32, y: f32, w: f32, h: f32) {
    GRID_RECT[0].store(x as i32, Ordering::Relaxed);
    GRID_RECT[1].store(y as i32, Ordering::Relaxed);
    GRID_RECT[2].store(w as i32, Ordering::Relaxed);
    GRID_RECT[3].store(h as i32, Ordering::Relaxed);
}

pub fn set_grid_wheel(on: bool) {
    GRID_WHEEL.store(on, Ordering::Relaxed);
}

fn hwnd() -> HWND {
    HWND(HWND_RAW.load(Ordering::Relaxed) as *mut _)
}

/// The modifier keys held now (any thread).
pub fn modifiers() -> (bool, bool, bool) {
    unsafe {
        (
            GetAsyncKeyState(VK_CONTROL.0 as i32) < 0,
            GetAsyncKeyState(VK_SHIFT.0 as i32) < 0,
            GetAsyncKeyState(VK_MENU.0 as i32) < 0,
        )
    }
}

/// Ends the app (any thread).
pub fn quit() {
    unsafe {
        let _ = PostThreadMessageW(UI_THREAD.load(Ordering::Relaxed), WM_APP_QUIT, WPARAM(0), LPARAM(0));
    }
}

pub fn set_title(title: &str) {
    unsafe {
        let _ = SetWindowTextW(hwnd(), &HSTRING::from(title));
    }
}

fn key_down(vk: u16) -> bool {
    unsafe { GetKeyState(vk as i32) < 0 }
}

/// Keys the grid takes before NSH's text fields see them.
fn intercept_key(msg: &MSG) -> bool {
    if msg.message != WM_KEYDOWN && msg.message != WM_SYSKEYDOWN {
        return false;
    }
    let m = KEY_MODE.load(Ordering::Relaxed);
    if m == mode::FREE {
        return false;
    }
    let vk = msg.wParam.0 as u16;
    if vk == VK_PROCESSKEY.0 {
        return false; // an input method composes
    }
    let key = Key {
        vk,
        ctrl: key_down(VK_CONTROL.0),
        shift: key_down(VK_SHIFT.0),
        alt: key_down(VK_MENU.0),
    };
    let nav = [
        VK_UP.0, VK_DOWN.0, VK_LEFT.0, VK_RIGHT.0, VK_RETURN.0, VK_TAB.0, VK_ESCAPE.0,
    ];
    let take = match m {
        mode::GRID => {
            nav.contains(&vk)
                || [VK_DELETE.0, VK_BACK.0, VK_HOME.0, VK_END.0, VK_PRIOR.0, VK_NEXT.0, VK_F2.0, VK_SPACE.0]
                    .contains(&vk)
                || (key.ctrl
                    && [b'C', b'V', b'X', b'Z', b'Y', b'A', b'F', b'D'].iter().any(|c| *c as u16 == vk))
        }
        mode::EDITING => {
            if vk == VK_RETURN.0 {
                // Shift/Alt+Enter: a new line in multi-line text.
                !((key.shift || key.alt) && MULTILINE.load(Ordering::Relaxed))
            } else {
                vk == VK_TAB.0 || vk == VK_ESCAPE.0
            }
        }
        mode::EDITING_LIST | mode::POPUP => {
            nav.contains(&vk) || (m == mode::EDITING_LIST && [VK_DELETE.0, VK_BACK.0].contains(&vk) && false)
        }
        _ => false,
    };
    if take {
        send(Cmd::Key(key));
    }
    take
}

/// Wheel turns over the grid scroll it.
fn intercept_wheel(msg: &MSG) -> bool {
    if msg.message != WM_MOUSEWHEEL && msg.message != WM_MOUSEHWHEEL {
        return false;
    }
    if !GRID_WHEEL.load(Ordering::Relaxed) {
        return false;
    }
    let hwnd = hwnd();
    let mut pt = POINT {
        x: (msg.lParam.0 & 0xFFFF) as i16 as i32,
        y: ((msg.lParam.0 >> 16) & 0xFFFF) as i16 as i32,
    };
    unsafe {
        let _ = ScreenToClient(hwnd, &mut pt);
    }
    let scale = unsafe { GetDpiForWindow(hwnd) } as f32 / 96.0;
    let (x, y) = (pt.x as f32 / scale, pt.y as f32 / scale);
    let r: Vec<i32> = GRID_RECT.iter().map(|a| a.load(Ordering::Relaxed)).collect();
    if x < r[0] as f32 || y < r[1] as f32 || x > (r[0] + r[2]) as f32 || y > (r[1] + r[3]) as f32 {
        return false;
    }
    let delta = ((msg.wParam.0 >> 16) & 0xFFFF) as i16 as f32 / 120.0;
    let step = 3.0 * 36.0;
    let shift = key_down(VK_SHIFT.0);
    if msg.message == WM_MOUSEHWHEEL {
        send(Cmd::Wheel(delta * step, 0.0));
    } else if shift {
        send(Cmd::Wheel(-delta * step, 0.0));
    } else {
        send(Cmd::Wheel(0.0, -delta * step));
    }
    true
}

/// The automation port: one request per line, one answer per line.
fn start_automation(port: u16) {
    std::thread::spawn(move || {
        let Ok(listener) = std::net::TcpListener::bind(("127.0.0.1", port)) else {
            log::error!("automation port {port} is taken");
            return;
        };
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let mut out = stream.try_clone().expect("stream");
                let reader = BufReader::new(stream);
                for line in reader.lines().map_while(Result::ok) {
                    let (tx, rx) = std::sync::mpsc::channel();
                    send(Cmd::Automation(line, tx));
                    let answer = rx
                        .recv_timeout(std::time::Duration::from_secs(10))
                        .unwrap_or_else(|_| "{\"error\":\"timeout\"}".into());
                    if writeln!(out, "{}", answer.replace('\n', " ")).is_err() {
                        break;
                    }
                }
            });
        }
    });
}

pub fn run() {
    let ui = std::thread::Builder::new()
        .name("nsh-ui".into())
        .stack_size(16 << 20)
        .spawn(ui_thread)
        .expect("UI thread");
    let _ = ui.join();
}

fn ui_thread() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = OleInitialize(None);
    }
    UI_THREAD.store(unsafe { GetCurrentThreadId() }, Ordering::Relaxed);

    let options = AppOptions {
        title: "Rutable".to_owned(),
        app_id: "dev.rutable.app".to_owned(),
        size: (1440, 900),
        appearance: Appearance::Light,
        ..Default::default()
    };
    let view = match AppView::new(None, options) {
        Ok(view) => view,
        Err(err) => {
            unsafe {
                MessageBoxW(None, &HSTRING::from(format!("无法创建窗口: {err}")), w!("Rutable"), MB_OK | MB_ICONERROR);
            }
            return;
        }
    };
    let hwnd = view.hwnd();
    HWND_RAW.store(hwnd.0 as isize, Ordering::Relaxed);

    if let Some(port) = crate::args::automation_port() {
        start_automation(port);
    }

    view.load(App::Rust(Root::of::<crate::ui::app::RutableApp>()));
    view.show();
    unsafe {
        if crate::args::maximized() {
            let _ = ShowWindow(hwnd, SW_MAXIMIZE);
        }
        let _ = SetForegroundWindow(hwnd);
    }

    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.hwnd.is_invalid() && msg.message == WM_APP_QUIT {
                break;
            }
            if intercept_key(&msg) || intercept_wheel(&msg) {
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if msg.message == WM_QUIT {
                break;
            }
        }
    }
    view.stop();
    drop(view);
    unsafe { windows::Win32::System::Ole::OleUninitialize() };
}
