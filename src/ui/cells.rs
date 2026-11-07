//! Drawing cell values into a canvas: text, tags, people, progress bars,
//! stars, checkboxes, attachments, links.

use std::{cell::RefCell, collections::HashMap};

use nsh_core::{
    canvas::{CanvasTextAlign, CanvasTextBaseline},
    prelude::*,
};

use super::{icons, theme::*};
use crate::model::{
    Base, FieldKind, Table,
    value::{self, Value, strong_color, tag_color},
};

thread_local! {
    static WIDTHS: RefCell<HashMap<(u32, String), f32>> = RefCell::new(HashMap::new());
}

/// The width of `text` at `size` (cached).
pub fn text_w(text: &str, size: f32) -> f32 {
    let key = ((size * 10.0) as u32, text.to_string());
    if let Some(w) = WIDTHS.with(|m| m.borrow().get(&key).copied()) {
        return w;
    }
    let w = measure_text(&font(size), text).0;
    WIDTHS.with(|m| {
        let mut m = m.borrow_mut();
        if m.len() > 20000 {
            m.clear();
        }
        m.insert(key, w);
    });
    w
}

/// `text` cut to `width` with "…".
pub fn ellipsize(text: &str, width: f32, size: f32) -> String {
    let text = text.replace('\n', " ");
    if width <= 0.0 {
        return String::new();
    }
    if text_w(&text, size) <= width {
        return text;
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let s: String = chars[..mid].iter().collect::<String>() + "…";
        if text_w(&s, size) <= width {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    chars[..lo].iter().collect::<String>() + "…"
}

/// `text` broken into lines of `width` (at most `max` lines).
pub fn wrap(text: &str, width: f32, size: f32, max: usize) -> Vec<String> {
    let mut lines = vec![];
    for para in text.split('\n') {
        let mut line = String::new();
        for c in para.chars() {
            let next = format!("{line}{c}");
            if text_w(&next, size) > width && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                if lines.len() >= max {
                    break;
                }
            }
            line.push(c);
        }
        if lines.len() >= max {
            break;
        }
        lines.push(line);
        if lines.len() >= max {
            break;
        }
    }
    if lines.len() == max {
        let full: usize = lines.iter().map(|l| l.chars().count()).sum();
        if full < text.chars().filter(|c| *c != '\n').count() {
            let last = lines.pop().unwrap();
            lines.push(ellipsize(&(last + "……"), width, size));
        }
    }
    lines
}

pub fn text_at(p: Painter, s: &str, x: f32, y_mid: f32, size: f32, color: u32) -> Painter {
    p.font(&font(size))
        .fill_color(rgb(color))
        .text_align(CanvasTextAlign::Left)
        .text_baseline(CanvasTextBaseline::Middle)
        .fill_text(s, x, y_mid)
}

pub fn text_right(p: Painter, s: &str, x_right: f32, y_mid: f32, size: f32, color: u32) -> Painter {
    p.font(&font(size))
        .fill_color(rgb(color))
        .text_align(CanvasTextAlign::Right)
        .text_baseline(CanvasTextBaseline::Middle)
        .fill_text(s, x_right, y_mid)
}

pub fn rrect(p: Painter, x: f32, y: f32, w: f32, h: f32, r: f32, color: u32) -> Painter {
    p.fill_color(rgb(color))
        .begin_path()
        .round_rect(bounds(point(px(x), px(y)), size(px(w), px(h))), px(r))
        .fill()
}

pub fn rrect_a(p: Painter, x: f32, y: f32, w: f32, h: f32, r: f32, color: Rgba) -> Painter {
    p.fill_color(color)
        .begin_path()
        .round_rect(bounds(point(px(x), px(y)), size(px(w), px(h))), px(r))
        .fill()
}

pub fn stroke_rrect(p: Painter, x: f32, y: f32, w: f32, h: f32, r: f32, color: u32, lw: f32) -> Painter {
    p.stroke_color(rgb(color))
        .line_width(lw)
        .begin_path()
        .round_rect(bounds(point(px(x), px(y)), size(px(w), px(h))), px(r))
        .stroke()
}

pub fn circle(p: Painter, cx: f32, cy: f32, r: f32, color: u32) -> Painter {
    p.fill_color(rgb(color)).begin_path().arc(cx, cy, r, 0.0, std::f32::consts::TAU).fill()
}

pub fn clip(p: Painter, x: f32, y: f32, w: f32, h: f32) -> Painter {
    p.save().begin_path().rect(bounds(point(px(x), px(y)), size(px(w), px(h)))).clip()
}

/// A tag (an option, a linked record): its width.
pub fn tag(p: Painter, label: &str, x: f32, y_mid: f32, max_w: f32, bg: u32, fg: u32) -> (Painter, f32) {
    let s = 12.0;
    let pad = 8.0;
    let text = ellipsize(label, (max_w - 2.0 * pad).max(8.0), s);
    let w = (text_w(&text, s) + 2.0 * pad).min(max_w.max(16.0));
    let h = 20.0;
    let p = rrect(p, x, y_mid - h / 2.0, w, h, 10.0, bg);
    let p = text_at(p, &text, x + pad, y_mid + 0.5, s, fg);
    (p, w)
}

pub fn avatar(p: Painter, name: &str, cx: f32, cy: f32, r: f32, color: u8) -> Painter {
    let p = circle(p, cx, cy, r, strong_color(color));
    let initial: String = name.chars().last().map(|c| c.to_string()).unwrap_or_default();
    p.font(&font(r * 1.05))
        .fill_color(rgb(0xFFFFFF))
        .text_align(CanvasTextAlign::Center)
        .text_baseline(CanvasTextBaseline::Middle)
        .fill_text(&initial, cx, cy + 0.5)
}

pub fn checkbox(p: Painter, x: f32, y_mid: f32, on: bool) -> Painter {
    let s = 16.0;
    let y = y_mid - s / 2.0;
    if on {
        let p = rrect(p, x, y, s, s, 3.0, PRIMARY);
        icons::draw(p, "check", x + 2.0, y + 2.0, 12.0, 0xFFFFFF)
    } else {
        let p = rrect(p, x, y, s, s, 3.0, 0xFFFFFF);
        stroke_rrect(p, x + 0.5, y + 0.5, s - 1.0, s - 1.0, 3.0, 0xBBBFC4, 1.0)
    }
}

pub fn star_path() -> &'static str {
    icons::path("rating")
}

pub fn rating(p: Painter, x: f32, y_mid: f32, n: u8, max: u8, hover: Option<u8>) -> Painter {
    let mut p = p;
    let s = 16.0;
    for i in 0..max {
        let filled = i < hover.unwrap_or(n);
        let color = if filled { 0xFFC60A } else { 0xDEE0E3 };
        let xi = x + i as f32 * (s + 4.0);
        p = p
            .save()
            .translate(xi, y_mid - s / 2.0)
            .scale(s / 24.0, s / 24.0)
            .fill_color(rgb(color))
            .fill_svg(star_path())
            .restore();
    }
    p
}

pub fn progress(p: Painter, x: f32, y_mid: f32, w: f32, v: f64, color: u8) -> Painter {
    let label = format!("{}%", (v * 100.0).round());
    let tw = 40.0;
    let bw = (w - tw - 8.0).max(20.0);
    let p = rrect(p, x, y_mid - 3.0, bw, 6.0, 3.0, 0xE5E6EB);
    let fill = (bw * v.clamp(0.0, 1.0) as f32).max(if v > 0.0 { 6.0 } else { 0.0 });
    let p = if fill > 0.0 { rrect(p, x, y_mid - 3.0, fill, 6.0, 3.0, strong_color(color)) } else { p };
    text_at(p, &label, x + bw + 8.0, y_mid, 13.0, TEXT)
}

pub fn file_thumb(p: Painter, a: &crate::model::Attachment, x: f32, y: f32, s: f32) -> Painter {
    if a.is_image() && !a.path.starts_with("rutable://") {
        let url = file_url(&a.path);
        let p = rrect(p, x, y, s, s, 3.0, 0xF2F3F5);
        p.save()
            .begin_path()
            .round_rect(bounds(point(px(x), px(y)), size(px(s), px(s))), px(3.0))
            .clip()
            .draw_image(&url, bounds(point(px(x), px(y)), size(px(s), px(s))))
            .restore()
    } else {
        let p = rrect(p, x, y, s, s, 3.0, 0xE1EAFF);
        let ext: String = a.name.rsplit('.').next().unwrap_or("").chars().take(4).collect::<String>().to_uppercase();
        let p = icons::draw(p, "file", x + s * 0.2, y + s * 0.12, s * 0.6, PRIMARY);
        p.font(&bold((s * 0.26).max(7.0)))
            .fill_color(rgb(PRIMARY))
            .text_align(CanvasTextAlign::Center)
            .text_baseline(CanvasTextBaseline::Middle)
            .fill_text(&ext, x + s / 2.0, y + s * 0.62)
    }
}

pub fn file_url(path: &str) -> String {
    let p = path.replace('\\', "/");
    if p.starts_with('/') { format!("file://{p}") } else { format!("file:///{p}") }
}

/// Draws `v` (of a field of `kind`) in the box (x, y, w, h).
#[allow(clippy::too_many_arguments)]
pub fn draw_value(
    mut p: Painter,
    base: &Base,
    table: &Table,
    kind: &FieldKind,
    v: Option<&Value>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    lines: usize,
) -> Painter {
    let Some(v) = v else { return p };
    let pad = 8.0;
    let mid = y + h.min(33.0) / 2.0;
    let inner = w - 2.0 * pad;
    let size = 14.0;
    match (v, kind) {
        (Value::Error(e), _) => text_at(p, &ellipsize(e, inner, 13.0), x + pad, mid, 13.0, DANGER),
        (Value::Bool(b), FieldKind::Checkbox) => checkbox(p, x + pad, mid, *b),
        (Value::Bool(b), _) => text_at(p, if *b { "✓" } else { "" }, x + pad, mid, size, TEXT),
        (Value::Number(n), FieldKind::Progress { color }) => progress(p, x + pad, mid, inner, *n, *color),
        (Value::Number(n), FieldKind::Rating { max, .. }) => rating(p, x + pad, mid, n.round() as u8, *max, None),
        (Value::Number(n), k) => {
            let s = if k.is_numeric() { value::format_number(*n, k) } else { value::format_plain(*n) };
            let s = ellipsize(&s, inner, size);
            text_right(p, &s, x + w - pad, mid, size, TEXT)
        }
        (Value::Options(ids), k) => {
            let opts = k.options().cloned().unwrap_or_default();
            let mut cx = x + pad;
            let mut row_y = mid;
            let line_h = 26.0;
            let mut row = 0;
            for id in ids {
                let Some(o) = opts.iter().find(|o| &o.id == id) else { continue };
                let (bg, fg) = tag_color(o.color);
                let left = x + w - pad - cx;
                let natural = text_w(&o.name, 12.0) + 16.0;
                if left < natural.min(40.0) {
                    if row + 1 >= lines {
                        break;
                    }
                    row += 1;
                    row_y += line_h;
                    cx = x + pad;
                }
                let (np, tw) = tag(p, &o.name, cx, row_y, x + w - pad - cx, bg, fg);
                p = np;
                cx += tw + 6.0;
            }
            p
        }
        (Value::Users(ids), _) => {
            let mut cx = x + pad;
            for id in ids {
                let Some(m) = base.member(id) else { continue };
                if cx > x + w - pad - 20.0 {
                    break;
                }
                p = avatar(p, &m.name, cx + 10.0, mid, 10.0, m.color);
                let nw = text_w(&m.name, 13.0);
                let name = ellipsize(&m.name, (x + w - pad - cx - 26.0).max(0.0), 13.0);
                p = text_at(p, &name, cx + 24.0, mid, 13.0, TEXT);
                cx += 24.0 + nw.min(text_w(&name, 13.0)) + 10.0;
            }
            p
        }
        (Value::Links(ids), FieldKind::Link { table: other, .. }) => {
            let Some(ot) = base.table(other) else { return p };
            let mut cx = x + pad;
            for id in ids {
                let title = value::record_title(base, ot, id);
                if cx > x + w - pad - 16.0 {
                    break;
                }
                let (np, tw) = tag(p, &title, cx, mid, x + w - pad - cx, 0xE8F0FE, 0x245BDB);
                p = np;
                cx += tw + 6.0;
            }
            p
        }
        (Value::Files(files), _) => {
            let s = (h.min(33.0) - 8.0).max(18.0).min(if lines > 1 { h - 10.0 } else { 25.0 });
            let mut cx = x + pad;
            for f in files {
                if cx + s > x + w - pad {
                    break;
                }
                p = file_thumb(p, f, cx, mid - s / 2.0, s);
                cx += s + 4.0;
            }
            p
        }
        (Value::Url { text, link }, _) => {
            let shown = if text.is_empty() { link } else { text };
            let s = ellipsize(shown, inner, size);
            let tw = text_w(&s, size);
            let p = text_at(p, &s, x + pad, mid, size, PRIMARY);
            p.fill_color(rgb(PRIMARY)).fill_rect(x + pad, mid + 8.0, tw, 1.0)
        }
        (Value::Date(ms), k) => {
            let s = match k {
                FieldKind::Date { format, time }
                | FieldKind::CreatedTime { format, time }
                | FieldKind::ModifiedTime { format, time } => value::format_date(*ms, *format, *time),
                _ => value::format_date(*ms, crate::model::DateFormat::YmdSlash, value::hour_minute(*ms) != (0, 0)),
            };
            text_at(p, &ellipsize(&s, inner, size), x + pad, mid, size, TEXT)
        }
        (Value::List(items), FieldKind::Lookup { .. }) | (Value::List(items), FieldKind::Formula { .. }) => {
            let mut cx = x + pad;
            for it in items {
                let s = value::display(base, table, &FieldKind::Text, it);
                if s.is_empty() {
                    continue;
                }
                if cx > x + w - pad - 16.0 {
                    break;
                }
                let (np, tw) = tag(p, &s, cx, mid, x + w - pad - cx, 0xEFF0F1, TEXT);
                p = np;
                cx += tw + 6.0;
            }
            p
        }
        (Value::Text(s), FieldKind::Phone | FieldKind::Email) => {
            text_at(p, &ellipsize(s, inner, size), x + pad, mid, size, PRIMARY)
        }
        _ => {
            let s = value::display(base, table, kind, v);
            if lines > 1 {
                let ls = wrap(&s, inner, size, lines);
                let mut yy = y + 16.0;
                for l in ls {
                    p = text_at(p, &l, x + pad, yy, size, TEXT);
                    yy += 22.0;
                }
                p
            } else {
                text_at(p, &ellipsize(&s, inner, size), x + pad, mid, size, TEXT)
            }
        }
    }
}

/// Hit-test the visible resource, using the same geometry as `draw_value`.
/// `Some(None)` is a URL; `Some(Some(i))` is attachment i.
pub fn resource_at(v: &Value, x: f32, y: f32, w: f32, h: f32) -> Option<Option<usize>> {
    let mid = h.min(33.0) / 2.0;
    match v {
        Value::Url { text, link } if !link.trim().is_empty() => {
            let shown = if text.is_empty() { link } else { text };
            let shown = ellipsize(shown, w - 16.0, 14.0);
            (x >= 8.0 && x < 8.0 + text_w(&shown, 14.0) && (y - mid).abs() <= 10.0).then_some(None)
        }
        Value::Files(files) => {
            let size = (h.min(33.0) - 8.0).max(18.0).min(25.0);
            for i in 0..files.len() {
                let left = 8.0 + i as f32 * (size + 4.0);
                if left + size > w - 8.0 {
                    break;
                }
                if x >= left && x < left + size && (y - mid).abs() <= size / 2.0 {
                    return Some(Some(i));
                }
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod resource_tests {
    use super::*;
    use crate::model::Attachment;

    #[test]
    fn link_whitespace_and_empty_targets_do_not_open() {
        // Supply text metrics without starting a UI surface in the unit test.
        WIDTHS.with(|widths| widths.borrow_mut().insert((140, "Docs".into()), 32.0));
        let link = Value::Url { text: "Docs".into(), link: "https://example.com".into() };
        assert_eq!(resource_at(&link, 10.0, 16.5, 200.0, 33.0), Some(None));
        assert_eq!(resource_at(&link, 180.0, 16.5, 200.0, 33.0), None);
        assert_eq!(resource_at(&link, 10.0, 50.0, 200.0, 66.0), None);
        let empty = Value::Url { text: "Docs".into(), link: " ".into() };
        assert_eq!(resource_at(&empty, 10.0, 16.5, 200.0, 33.0), None);
    }

    #[test]
    fn attachment_hits_select_the_clicked_visible_file() {
        let files = Value::Files((0..3).map(|i| Attachment {
            name: format!("{i}.txt"), path: format!(r"C:\files\{i}.txt"), size: 1,
        }).collect());
        assert_eq!(resource_at(&files, 10.0, 16.5, 100.0, 33.0), Some(Some(0)));
        assert_eq!(resource_at(&files, 40.0, 16.5, 100.0, 33.0), Some(Some(1)));
        assert_eq!(resource_at(&files, 34.0, 16.5, 100.0, 33.0), None); // thumbnail gap
        assert_eq!(resource_at(&files, 70.0, 16.5, 90.0, 33.0), None); // clipped file
        assert_eq!(resource_at(&files, 10.0, 50.0, 100.0, 66.0), None);
    }
}
