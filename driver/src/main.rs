//! rutable-driver: drives a running Rutable window as a user's hands would.
//!
//! Commands come as arguments (`;`-separated) or one per line on stdin
//! (`-`). Coordinates are client points of the window (96 DPI units).
//!
//!   launch <exe> [args..]       start the app (waits for its window)
//!   focus                       bring the window to the front
//!   click X Y | dclick X Y | rclick X Y | move X Y
//!   drag X1 Y1 X2 Y2            press, move in steps, release
//!   down X Y | up X Y           left button down / up
//!   wheel X Y DY                wheel turns (+ = up) over a point
//!   key ctrl+shift+a | key enter | key down ...
//!   type TEXT                   unicode text (rest of the line)
//!   wait MS
//!   shot FILE.png               the window's client area
//!   state [QUERY]               the app's state over the automation port
//!   close                       closes the window

use std::{
    io::{BufRead, BufReader, Write},
    time::Duration,
};

use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, POINT, RECT, WPARAM},
        Graphics::Gdi::*,
        Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow},
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext},
            Input::KeyboardAndMouse::*,
            WindowsAndMessaging::*,
        },
    },
    core::BOOL,
};

const TITLE_PREFIX: &str = "Rutable";

fn find_window() -> Option<HWND> {
    struct Found(Option<HWND>);
    unsafe extern "system" fn each(hwnd: HWND, l: LPARAM) -> BOOL {
        let found = unsafe { &mut *(l.0 as *mut Found) };
        let mut buf = [0u16; 256];
        let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
        let title = String::from_utf16_lossy(&buf[..n as usize]);
        if title.starts_with(TITLE_PREFIX) && unsafe { IsWindowVisible(hwnd) }.as_bool() {
            let mut class = [0u16; 256];
            let n = unsafe { GetClassNameW(hwnd, &mut class) };
            let class = String::from_utf16_lossy(&class[..n as usize]);
            if !class.contains("Console") && !class.contains("CASCADIA") {
                found.0 = Some(hwnd);
                return false.into();
            }
        }
        true.into()
    }
    let mut found = Found(None);
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut found as *mut _ as isize));
    }
    found.0
}

struct Win {
    hwnd: HWND,
    scale: f32,
}

impl Win {
    fn get() -> Win {
        for _ in 0..100 {
            if let Some(hwnd) = find_window() {
                let scale = unsafe { GetDpiForWindow(hwnd) } as f32 / 96.0;
                return Win { hwnd, scale };
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        eprintln!("no Rutable window");
        std::process::exit(2);
    }

    /// Screen pixels of client point (x, y).
    fn screen(&self, x: f32, y: f32) -> (i32, i32) {
        let mut pt = POINT {
            x: (x * self.scale).round() as i32,
            y: (y * self.scale).round() as i32,
        };
        unsafe {
            let _ = ClientToScreen(self.hwnd, &mut pt);
        }
        (pt.x, pt.y)
    }
}

fn send(inputs: &[INPUT]) {
    unsafe {
        SendInput(inputs, size_of::<INPUT>() as i32);
    }
}

fn mouse(flags: MOUSE_EVENT_FLAGS, data: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: data as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn move_to(w: &Win, x: f32, y: f32) {
    let (sx, sy) = w.screen(x, y);
    unsafe {
        let _ = SetCursorPos(sx, sy);
    }
    // A relative move of zero makes the system send WM_MOUSEMOVE.
    send(&[mouse(MOUSEEVENTF_MOVE, 0)]);
    std::thread::sleep(Duration::from_millis(30));
}

fn key_input(vk: u16, up: bool) -> INPUT {
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    if [0x25u16, 0x26, 0x27, 0x28, 0x2D, 0x2E, 0x24, 0x23, 0x21, 0x22].contains(&vk) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) } as u16,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn vk_of(name: &str) -> Option<u16> {
    let n = name.to_lowercase();
    Some(match n.as_str() {
        "ctrl" | "control" => 0x11,
        "shift" => 0x10,
        "alt" => 0x12,
        "enter" | "return" => 0x0D,
        "tab" => 0x09,
        "esc" | "escape" => 0x1B,
        "space" => 0x20,
        "backspace" => 0x08,
        "delete" | "del" => 0x2E,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        "f2" => 0x71,
        s if s.len() == 1 => {
            let c = s.chars().next()?.to_ascii_uppercase();
            if c.is_ascii_alphanumeric() {
                c as u16
            } else {
                return None;
            }
        }
        s if s.starts_with('f') => 0x6F + s[1..].parse::<u16>().ok()?,
        _ => return None,
    })
}

fn press_combo(combo: &str) {
    let keys: Vec<u16> = combo.split('+').filter_map(vk_of).collect();
    let mut inputs: Vec<INPUT> = keys.iter().map(|k| key_input(*k, false)).collect();
    inputs.extend(keys.iter().rev().map(|k| key_input(*k, true)));
    send(&inputs);
}

fn type_text(text: &str) {
    for unit in text.encode_utf16() {
        let make = |up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: unit,
                    dwFlags: KEYEVENTF_UNICODE | if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        send(&[make(false), make(true)]);
        std::thread::sleep(Duration::from_millis(8));
    }
}

fn screenshot(w: &Win, file: &str) {
    unsafe {
        let mut rc = RECT::default();
        let _ = GetClientRect(w.hwnd, &mut rc);
        let (cw, ch) = (rc.right - rc.left, rc.bottom - rc.top);
        let mut wr = RECT::default();
        let _ = GetWindowRect(w.hwnd, &mut wr);
        let (ww, wh) = (wr.right - wr.left, wr.bottom - wr.top);
        let screen = GetDC(None);
        let dc = CreateCompatibleDC(Some(screen));
        let bmp = CreateCompatibleBitmap(screen, ww, wh);
        let old = SelectObject(dc, bmp.into());
        // PW_RENDERFULLCONTENT (2) captures DirectComposition content.
        let _ = PrintWindow(w.hwnd, dc, PRINT_WINDOW_FLAGS(2));
        let mut origin = POINT::default();
        let _ = ClientToScreen(w.hwnd, &mut origin);
        let (ox, oy) = (origin.x - wr.left, origin.y - wr.top);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: ww,
                biHeight: -wh,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; (ww * wh * 4) as usize];
        GetDIBits(dc, bmp, 0, wh as u32, Some(pixels.as_mut_ptr() as *mut _), &mut info, DIB_RGB_COLORS);
        SelectObject(dc, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(dc);
        ReleaseDC(None, screen);
        let mut rgb = Vec::with_capacity((cw * ch * 3) as usize);
        for y in 0..ch {
            for x in 0..cw {
                let (sx, sy) = (x + ox, y + oy);
                if sx >= ww || sy >= wh {
                    rgb.extend_from_slice(&[0, 0, 0]);
                    continue;
                }
                let i = ((sy * ww + sx) * 4) as usize;
                rgb.extend_from_slice(&[pixels[i + 2], pixels[i + 1], pixels[i]]);
            }
        }
        let f = std::fs::File::create(file).expect("create png");
        let mut enc = png::Encoder::new(std::io::BufWriter::new(f), cw as u32, ch as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&rgb).unwrap();
        println!("shot {file} {}x{} (scale {})", cw, ch, w.scale);
    }
}

fn state(query: &str, port: u16) {
    match std::net::TcpStream::connect(("127.0.0.1", port)) {
        Ok(mut s) => {
            let _ = writeln!(s, "{}", if query.is_empty() { "state" } else { query });
            let mut line = String::new();
            let _ = BufReader::new(s).read_line(&mut line);
            println!("{}", line.trim_end());
        }
        Err(e) => println!("{{\"error\":\"no automation port: {e}\"}}"),
    }
}

fn run(line: &str, port: u16) {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return;
    }
    let (cmd, rest) = line.split_once(' ').unwrap_or((line, ""));
    let nums: Vec<f32> = rest.split_whitespace().filter_map(|n| n.parse().ok()).collect();
    let n = |i: usize| nums.get(i).copied().unwrap_or(0.0);
    match cmd {
        "launch" => {
            let mut parts = rest.split_whitespace();
            let exe = parts.next().expect("exe");
            std::process::Command::new(exe)
                .args(parts)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("launch");
            let w = Win::get();
            std::thread::sleep(Duration::from_millis(800));
            println!("launched hwnd={:?} scale={}", w.hwnd.0, w.scale);
        }
        "focus" => {
            let w = Win::get();
            unsafe {
                // A key event lets this process take the foreground.
                send(&[key_input(0x12, false), key_input(0x12, true)]);
                let _ = SetForegroundWindow(w.hwnd);
            }
        }
        "click" | "dclick" | "rclick" => {
            let w = Win::get();
            move_to(&w, n(0), n(1));
            let (d, u) = if cmd == "rclick" {
                (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP)
            } else {
                (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP)
            };
            let times = if cmd == "dclick" { 2 } else { 1 };
            for _ in 0..times {
                send(&[mouse(d, 0)]);
                std::thread::sleep(Duration::from_millis(40));
                send(&[mouse(u, 0)]);
                std::thread::sleep(Duration::from_millis(60));
            }
        }
        "move" => move_to(&Win::get(), n(0), n(1)),
        "down" => {
            let w = Win::get();
            move_to(&w, n(0), n(1));
            send(&[mouse(MOUSEEVENTF_LEFTDOWN, 0)]);
        }
        "up" => {
            let w = Win::get();
            move_to(&w, n(0), n(1));
            send(&[mouse(MOUSEEVENTF_LEFTUP, 0)]);
        }
        "drag" => {
            let w = Win::get();
            move_to(&w, n(0), n(1));
            send(&[mouse(MOUSEEVENTF_LEFTDOWN, 0)]);
            std::thread::sleep(Duration::from_millis(80));
            let steps = 12;
            for i in 1..=steps {
                let t = i as f32 / steps as f32;
                move_to(&w, n(0) + (n(2) - n(0)) * t, n(1) + (n(3) - n(1)) * t);
            }
            std::thread::sleep(Duration::from_millis(80));
            send(&[mouse(MOUSEEVENTF_LEFTUP, 0)]);
        }
        "wheel" => {
            let w = Win::get();
            move_to(&w, n(0), n(1));
            send(&[mouse(MOUSEEVENTF_WHEEL, (n(2) * 120.0) as i32)]);
        }
        "key" => {
            for combo in rest.split_whitespace() {
                press_combo(combo);
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        "type" => type_text(rest),
        "wait" => std::thread::sleep(Duration::from_millis(n(0) as u64)),
        "shot" => {
            std::thread::sleep(Duration::from_millis(250));
            screenshot(&Win::get(), rest.trim());
        }
        "state" => state(rest.trim(), port),
        "close" => {
            if let Some(h) = find_window() {
                unsafe {
                    let _ = PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
        }
        other => eprintln!("unknown command {other}"),
    }
    // Let the app catch up between steps.
    if !matches!(cmd, "wait" | "state" | "shot") {
        std::thread::sleep(Duration::from_millis(120));
    }
}

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let port = std::env::var("RUTABLE_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(47800);
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("-") {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            run(&line, port);
            let _ = std::io::stdout().flush();
        }
    } else {
        for line in args.join(" ").split(';') {
            run(line, port);
        }
    }
}
