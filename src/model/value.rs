//! Cell values: what a cell holds, how it shows as text, how text (typed or
//! pasted) becomes a value, and how values compare.

use std::cmp::Ordering;

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, TimeZone, Timelike};
use serde::{Deserialize, Serialize};

use super::{Attachment, Base, DateFormat, FieldKind, Id, Table};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "t", content = "v")]
pub enum Value {
    Text(String),
    Number(f64),
    Bool(bool),
    /// Milliseconds since the epoch.
    Date(i64),
    /// Option ids (single select: at most one).
    Options(Vec<Id>),
    /// Member ids.
    Users(Vec<Id>),
    /// Linked record ids.
    Links(Vec<Id>),
    Files(Vec<Attachment>),
    /// A hyperlink: its text and its address.
    Url { text: String, link: String },
    /// Computed lists (lookups): the values of the linked records.
    List(Vec<Value>),
    /// A formula's error (#ERROR!, #DIV/0!...).
    Error(String),
}

impl Value {
    pub fn is_empty(&self) -> bool {
        match self {
            Value::Text(s) => s.trim().is_empty(),
            Value::Number(n) => n.is_nan(),
            Value::Bool(b) => !*b,
            Value::Date(_) => false,
            Value::Options(v) | Value::Users(v) | Value::Links(v) => v.is_empty(),
            Value::Files(v) => v.is_empty(),
            Value::Url { text, link } => text.is_empty() && link.is_empty(),
            Value::List(v) => v.iter().all(Value::is_empty),
            Value::Error(_) => false,
        }
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            Value::Text(s) => s.trim().replace(',', "").parse().ok(),
            Value::Date(d) => Some(*d as f64),
            Value::List(v) if v.len() == 1 => v[0].as_number(),
            _ => None,
        }
    }

    pub fn ids(&self) -> &[Id] {
        match self {
            Value::Options(v) | Value::Users(v) | Value::Links(v) => v,
            _ => &[],
        }
    }
}

/// Option colors (浅色背景, 文字色) of select tags.
pub const TAG_COLORS: [(u32, u32); 11] = [
    (0xDCE7FF, 0x1F2329),
    (0xD4EEFB, 0x1F2329),
    (0xD1F2EC, 0x1F2329),
    (0xD9F5D6, 0x1F2329),
    (0xEEF6C6, 0x1F2329),
    (0xFCF3C3, 0x1F2329),
    (0xFEE7CD, 0x1F2329),
    (0xFDE2E2, 0x1F2329),
    (0xFBDEF0, 0x1F2329),
    (0xECE2FE, 0x1F2329),
    (0xE9EAEC, 0x1F2329),
];

/// Strong colors: avatars, progress bars, kanban dots, charts.
pub const STRONG_COLORS: [u32; 11] = [
    0x3370FF, 0x14C0FF, 0x00B8A9, 0x34C724, 0x8FD400, 0xFFC60A, 0xFF8800, 0xF54A45, 0xF14BA9, 0x7F3BF5, 0x8F959E,
];

pub const COLOR_NAMES: [&str; 11] = ["蓝色", "天蓝", "青色", "绿色", "黄绿", "黄色", "橙色", "红色", "玫红", "紫色", "灰色"];

pub fn tag_color(i: u8) -> (u32, u32) {
    TAG_COLORS[i as usize % TAG_COLORS.len()]
}

pub fn strong_color(i: u8) -> u32 {
    STRONG_COLORS[i as usize % STRONG_COLORS.len()]
}

pub fn local(ms: i64) -> chrono::DateTime<Local> {
    Local.timestamp_millis_opt(ms).single().unwrap_or_else(|| Local.timestamp_millis_opt(0).unwrap())
}

pub fn format_date(ms: i64, format: DateFormat, time: bool) -> String {
    let d = local(ms);
    let mut s = d.format(format.pattern()).to_string();
    if time {
        s.push_str(&d.format(" %H:%M").to_string());
    }
    s
}

/// The start of the local day of `ms`.
pub fn day_start(ms: i64) -> i64 {
    let d = local(ms).date_naive();
    date_ms(d.year(), d.month(), d.day())
}

pub fn date_ms(y: i32, m: u32, d: u32) -> i64 {
    NaiveDate::from_ymd_opt(y, m, d)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .and_then(|dt| Local.from_local_datetime(&dt).single())
        .map(|d| d.timestamp_millis())
        .unwrap_or(0)
}

pub fn today_ms() -> i64 {
    day_start(super::now_ms())
}

pub const DAY_MS: i64 = 86_400_000;

/// Parses a date as people type one: 2024/1/30, 2024-01-30 14:00, 1/30,
/// 20240130, 今天, 明天, 昨天.
pub fn parse_date(s: &str) -> Option<i64> {
    let s = s.trim();
    match s {
        "今天" | "today" => return Some(today_ms()),
        "明天" | "tomorrow" => return Some(today_ms() + DAY_MS),
        "昨天" | "yesterday" => return Some(today_ms() - DAY_MS),
        _ => {}
    }
    let norm: String = s
        .replace(['年', '月', '.', '-'], "/")
        .replace('日', "")
        .replace('T', " ");
    let (date_part, time_part) = match norm.split_once(' ') {
        Some((d, t)) => (d.trim().to_string(), Some(t.trim().to_string())),
        None => (norm.trim().to_string(), None),
    };
    let parts: Vec<&str> = date_part.split('/').filter(|p| !p.is_empty()).collect();
    let now = Local::now();
    let (y, m, d) = match parts.as_slice() {
        [y, m, d] => {
            let y: i32 = y.parse().ok()?;
            let y = if y < 100 { 2000 + y } else { y };
            (y, m.parse().ok()?, d.parse().ok()?)
        }
        [m, d] => (now.year(), m.parse().ok()?, d.parse().ok()?),
        [one] if one.len() == 8 && one.chars().all(|c| c.is_ascii_digit()) => {
            (one[..4].parse().ok()?, one[4..6].parse().ok()?, one[6..].parse().ok()?)
        }
        _ => return None,
    };
    let date = NaiveDate::from_ymd_opt(y, m, d)?;
    let (h, mi) = match time_part {
        Some(t) => {
            let mut it = t.split(':');
            (it.next()?.parse().ok()?, it.next().and_then(|x| x.parse().ok()).unwrap_or(0))
        }
        None => (0, 0),
    };
    let dt = NaiveDateTime::new(date, chrono::NaiveTime::from_hms_opt(h, mi, 0)?);
    Local.from_local_datetime(&dt).single().map(|d| d.timestamp_millis())
}

pub fn hour_minute(ms: i64) -> (u32, u32) {
    let d = local(ms);
    (d.hour(), d.minute())
}

fn trim_zeros(s: String) -> String {
    s
}

/// A number as the field shows it.
pub fn format_number(n: f64, kind: &FieldKind) -> String {
    match kind {
        FieldKind::Number { precision, thousands, percent } => {
            let v = if *percent { n * 100.0 } else { n };
            let mut s = format!("{:.*}", *precision as usize, v);
            if *thousands {
                s = group_thousands(&s);
            }
            if *percent {
                s.push('%');
            }
            trim_zeros(s)
        }
        FieldKind::Currency { symbol, precision } => {
            format!("{symbol}{}", group_thousands(&format!("{:.*}", *precision as usize, n)))
        }
        FieldKind::Progress { .. } => format!("{}%", (n * 100.0).round()),
        FieldKind::Rating { .. } => format!("{}", n.round()),
        _ => format_plain(n),
    }
}

/// A number without trailing zeros (formula results, lookups).
pub fn format_plain(n: f64) -> String {
    if n.is_nan() {
        return String::new();
    }
    if n.fract() == 0.0 && n.abs() < 1e15 {
        return format!("{}", n as i64);
    }
    let s = format!("{:.6}", n);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn group_thousands(s: &str) -> String {
    let (sign, rest) = if let Some(r) = s.strip_prefix('-') { ("-", r) } else { ("", s) };
    let (int, frac) = match rest.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (rest, None),
    };
    let mut out = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    let mut s = format!("{sign}{out}");
    if let Some(f) = frac {
        s.push('.');
        s.push_str(f);
    }
    s
}

/// A value as text, as copying it or exporting it gives it.
pub fn display(base: &Base, table: &Table, kind: &FieldKind, v: &Value) -> String {
    match v {
        Value::Text(s) => s.clone(),
        Value::Number(n) => {
            if n.is_nan() {
                String::new()
            } else {
                format_number(*n, kind)
            }
        }
        Value::Bool(b) => {
            if matches!(kind, FieldKind::Checkbox) {
                if *b { "true".into() } else { "false".into() }
            } else if *b {
                "TRUE".into()
            } else {
                "FALSE".into()
            }
        }
        Value::Date(ms) => match kind {
            FieldKind::Date { format, time }
            | FieldKind::CreatedTime { format, time }
            | FieldKind::ModifiedTime { format, time } => format_date(*ms, *format, *time),
            _ => format_date(*ms, DateFormat::YmdSlash, false),
        },
        Value::Options(ids) => {
            let opts = kind.options();
            ids.iter()
                .filter_map(|id| opts.and_then(|o| o.iter().find(|x| &x.id == id)).map(|o| o.name.clone()))
                .collect::<Vec<_>>()
                .join(",")
        }
        Value::Users(ids) => ids.iter().map(|id| base.member_name(id)).collect::<Vec<_>>().join(","),
        Value::Links(ids) => {
            let other = match kind {
                FieldKind::Link { table: t, .. } => base.table(t),
                _ => None,
            };
            match other {
                Some(ot) => ids.iter().map(|id| record_title(base, ot, id)).collect::<Vec<_>>().join(","),
                None => String::new(),
            }
        }
        Value::Files(files) => files.iter().map(|f| f.name.clone()).collect::<Vec<_>>().join(","),
        Value::Url { text, link } => {
            if text.is_empty() || text == link {
                link.clone()
            } else {
                format!("{text} ({link})")
            }
        }
        Value::List(items) => {
            items.iter().map(|x| display(base, table, &FieldKind::Text, x)).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(",")
        }
        Value::Error(e) => e.clone(),
    }
}

/// The text of a record's primary field (its title), "未命名记录" if empty.
pub fn record_title(base: &Base, table: &Table, record: &str) -> String {
    let Some(r) = table.record(record) else { return String::new() };
    let Some(p) = table.primary() else { return String::new() };
    let v = super::query::cell(base, table, r, p);
    let s = v.map(|v| display(base, table, &p.kind, &v)).unwrap_or_default();
    if s.trim().is_empty() { "未命名记录".into() } else { s }
}

/// Text typed or pasted into a cell of `field`, as a value; `None` clears
/// it. Options that do not exist yet are created (in `kind`).
pub fn parse(base: &Base, kind: &mut FieldKind, text: &str) -> Option<Value> {
    let t = text.trim();
    if t.is_empty() && !matches!(kind, FieldKind::Text) {
        return None;
    }
    match kind {
        FieldKind::Text | FieldKind::Phone | FieldKind::Email | FieldKind::Barcode | FieldKind::Location => {
            if text.is_empty() {
                None
            } else {
                Some(Value::Text(text.to_string()))
            }
        }
        FieldKind::Url => Some(Value::Url { text: t.into(), link: t.into() }),
        FieldKind::Number { percent, .. } => {
            let s = t.replace([',', '，', ' '], "");
            if let Some(p) = s.strip_suffix('%') {
                return p.parse::<f64>().ok().map(|n| Value::Number(n / 100.0));
            }
            s.parse::<f64>().ok().map(|n| Value::Number(if *percent && t.ends_with('%') { n / 100.0 } else { n }))
        }
        FieldKind::Currency { .. } => {
            let s: String = t.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect();
            s.parse::<f64>().ok().map(Value::Number)
        }
        FieldKind::Progress { .. } => {
            let s = t.replace('%', "");
            s.parse::<f64>().ok().map(|n| Value::Number(if t.contains('%') || n > 1.0 { n / 100.0 } else { n }))
        }
        FieldKind::Rating { max, .. } => {
            let n = t.chars().filter(|c| *c == '⭐' || *c == '★').count();
            let v = if n > 0 { n as f64 } else { t.parse::<f64>().ok()? };
            Some(Value::Number(v.clamp(0.0, *max as f64).round()))
        }
        FieldKind::Checkbox => {
            let yes = matches!(t.to_lowercase().as_str(), "true" | "1" | "yes" | "是" | "√" | "✓" | "checked" | "已勾选");
            Some(Value::Bool(yes))
        }
        FieldKind::Date { .. } => parse_date(t).map(Value::Date),
        FieldKind::SingleSelect { .. } | FieldKind::MultiSelect { .. } => {
            let multiple = matches!(kind, FieldKind::MultiSelect { .. });
            let names: Vec<String> = if multiple {
                t.split([',', '，', '\n']).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
            } else {
                vec![t.to_string()]
            };
            let options = kind.options_mut().unwrap();
            let mut ids = vec![];
            for n in names {
                let id = match options.iter().find(|o| o.name == n) {
                    Some(o) => o.id.clone(),
                    None => {
                        let o = super::SelectOption {
                            id: super::new_id("opt"),
                            name: n.clone(),
                            color: (options.len() % TAG_COLORS.len()) as u8,
                        };
                        let id = o.id.clone();
                        options.push(o);
                        id
                    }
                };
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            if ids.is_empty() { None } else { Some(Value::Options(ids)) }
        }
        FieldKind::User { multiple } => {
            let mut ids: Vec<Id> = t
                .split([',', '，'])
                .filter_map(|n| base.members.iter().find(|m| m.name == n.trim()).map(|m| m.id.clone()))
                .collect();
            if !*multiple {
                ids.truncate(1);
            }
            if ids.is_empty() { None } else { Some(Value::Users(ids)) }
        }
        FieldKind::Link { table, .. } => {
            let ot = base.table(table)?;
            let ids: Vec<Id> = t
                .split([',', '，'])
                .filter_map(|n| {
                    ot.records.iter().find(|r| record_title(base, ot, &r.id) == n.trim()).map(|r| r.id.clone())
                })
                .collect();
            if ids.is_empty() { None } else { Some(Value::Links(ids)) }
        }
        _ => None,
    }
}

/// Converts a cell's value when its field changes type: through its text.
pub fn convert(base: &Base, table: &Table, from: &FieldKind, to: &mut FieldKind, v: &Value) -> Option<Value> {
    // Options keep their names (and colors where the new type has options).
    if let (Some(old), Some(_)) = (from.options(), to.options()) {
        if let Value::Options(ids) = v {
            let names: Vec<(String, u8)> = ids
                .iter()
                .filter_map(|id| old.iter().find(|o| &o.id == id).map(|o| (o.name.clone(), o.color)))
                .collect();
            let options = to.options_mut().unwrap();
            let mut out = vec![];
            for (n, c) in names {
                let id = match options.iter().find(|o| o.name == n) {
                    Some(o) => o.id.clone(),
                    None => {
                        let o = super::SelectOption { id: super::new_id("opt"), name: n, color: c };
                        let id = o.id.clone();
                        options.push(o);
                        id
                    }
                };
                out.push(id);
            }
            if matches!(to, FieldKind::SingleSelect { .. }) {
                out.truncate(1);
            }
            return if out.is_empty() { None } else { Some(Value::Options(out)) };
        }
    }
    match (v, &*to) {
        (Value::Number(n), k) if k.is_numeric() => Some(Value::Number(*n)),
        (Value::Date(d), FieldKind::Date { .. }) => Some(Value::Date(*d)),
        (Value::Users(u), FieldKind::User { multiple }) => {
            let mut u = u.clone();
            if !multiple {
                u.truncate(1);
            }
            Some(Value::Users(u))
        }
        (Value::Links(l), FieldKind::Link { .. }) => Some(Value::Links(l.clone())),
        (Value::Files(f), FieldKind::Attachment) => Some(Value::Files(f.clone())),
        _ => {
            let text = display(base, table, from, v);
            parse(base, to, &text)
        }
    }
}

/// How two values of `kind` sort (empty values last).
pub fn compare(base: &Base, table: &Table, kind: &FieldKind, a: Option<&Value>, b: Option<&Value>) -> Ordering {
    let a = a.filter(|v| !v.is_empty());
    let b = b.filter(|v| !v.is_empty());
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(a), Some(b)) => match (a, b) {
            (Value::Number(x), Value::Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
            (Value::Date(x), Value::Date(y)) => x.cmp(y),
            (Value::Bool(x), Value::Bool(y)) => y.cmp(x),
            (Value::Options(x), Value::Options(y)) => {
                // Options sort in their order in the field.
                let pos = |ids: &Vec<Id>| {
                    ids.first()
                        .and_then(|id| kind.options().and_then(|o| o.iter().position(|o| &o.id == id)))
                        .unwrap_or(usize::MAX)
                };
                pos(x).cmp(&pos(y))
            }
            _ => {
                if let (Some(x), Some(y)) = (a.as_number(), b.as_number()) {
                    if !matches!(a, Value::Text(_)) || !matches!(b, Value::Text(_)) {
                        return x.partial_cmp(&y).unwrap_or(Ordering::Equal);
                    }
                }
                let x = display(base, table, kind, a);
                let y = display(base, table, kind, b);
                natural_cmp(&x, &y)
            }
        },
    }
}

/// Compares texts with their numbers by value ("任务2" before "任务10").
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut nx = String::new();
                while let Some(c) = ai.peek().copied().filter(|c| c.is_ascii_digit()) {
                    nx.push(c);
                    ai.next();
                }
                let mut ny = String::new();
                while let Some(c) = bi.peek().copied().filter(|c| c.is_ascii_digit()) {
                    ny.push(c);
                    bi.next();
                }
                let o = nx.parse::<u128>().unwrap_or(0).cmp(&ny.parse::<u128>().unwrap_or(0));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                let o = x.to_lowercase().cmp(y.to_lowercase());
                if o != Ordering::Equal {
                    return o;
                }
                ai.next();
                bi.next();
            }
        }
    }
}

pub fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_parse() {
        let d = parse_date("2024/1/30").unwrap();
        assert_eq!(format_date(d, DateFormat::YmdDash, false), "2024-01-30");
        let d = parse_date("2024-01-30 14:05").unwrap();
        assert_eq!(format_date(d, DateFormat::YmdSlash, true), "2024/01/30 14:05");
        assert!(parse_date("hello").is_none());
        assert_eq!(parse_date("20240130"), parse_date("2024/01/30"));
    }

    #[test]
    fn numbers_format() {
        let k = FieldKind::Number { precision: 2, thousands: true, percent: false };
        assert_eq!(format_number(1234567.891, &k), "1,234,567.89");
        assert_eq!(format_plain(3.5), "3.5");
        assert_eq!(format_plain(4.0), "4");
    }

    #[test]
    fn natural() {
        assert_eq!(natural_cmp("任务2", "任务10"), Ordering::Less);
    }
}
