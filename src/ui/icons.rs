//! Line icons on a 24-point grid (stroked SVG path data), drawn in canvases.

use nsh_core::prelude::*;

use crate::model::{FieldKind, KindTag, ViewKind};

/// Path data of each icon (stroked, round caps and joins, width 2 on 24).
pub fn path(name: &str) -> &'static str {
    match name {
        "text" => "M4 7V5h16v2 M9 19h6 M12 5v14",
        "single" => "M12 2a10 10 0 1 0 0.01 0 M8 12l3 3 5-6",
        "multi" => "M9 6h11 M9 12h11 M9 18h11 M4 5.5l1 1 2-2 M4 11.5l1 1 2-2 M4 17.5l1 1 2-2",
        "user" => "M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2 M12 3a4 4 0 1 0 0.01 0",
        "users" => "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2 M9 3a4 4 0 1 0 0.01 0 M22 21v-2a4 4 0 0 0-3-3.87 M16 3.13a4 4 0 0 1 0 7.75",
        "date" => "M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z M16 2v4 M8 2v4 M3 10h18",
        "attachment" => "M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48",
        "number" => "M4 9h16 M4 15h16 M10 3L8 21 M16 3l-2 18",
        "checkbox" => "M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z M8 12l3 3 5-6",
        "link" => "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71 M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71",
        "duplex" => "M7 17L17 7 M17 7h-7 M17 7v7 M3 12h3 M18 12h3",
        "lookup" => "M11 3a8 8 0 1 0 0.01 0 M21 21l-4.35-4.35 M8 11h6",
        "formula" => "M9 4h6 M10 4c-1 0-2 1-2 3v10c0 2-1 3-2 3 M6 11h6 M14 12l6 7 M20 12l-6 7",
        "url" => "M15 3h6v6 M10 14L21 3 M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6",
        "phone" => "M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72c.13.96.36 1.9.7 2.81a2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45c.91.34 1.85.57 2.81.7A2 2 0 0 1 22 16.92z",
        "email" => "M4 4h16a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z M22 6l-10 7L2 6",
        "location" => "M20 10c0 6-8 12-8 12s-8-6-8-12a8 8 0 0 1 16 0z M12 7a3 3 0 1 0 0.01 0",
        "autonumber" => "M10 6h11 M10 12h11 M10 18h11 M4 6h1v4 M4 10h2 M6 18H4c0-1 2-2 2-3s-1-1.5-2-1",
        "currency" => "M6 3l6 7 6-7 M12 10v11 M7 13h10 M7 17h10",
        "progress" => "M3 8h18a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H3a1 1 0 0 1-1-1V9a1 1 0 0 1 1-1z M6 12h7",
        "rating" => "M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z",
        "barcode" => "M3 5v14 M7 5v14 M10 5v14 M14 5v14 M17 5v14 M21 5v14",
        "clock" => "M12 2a10 10 0 1 0 0.01 0 M12 6v6l4 2",
        "userpen" => "M11 15H8a4 4 0 0 0-4 4v2 M10 3a4 4 0 1 0 0.01 0 M17.5 13.5l3 3L15 22h-3v-3z",
        "plus" => "M12 5v14 M5 12h14",
        "minus" => "M5 12h14",
        "x" => "M18 6L6 18 M6 6l12 12",
        "check" => "M20 6L9 17l-5-5",
        "chevron-down" => "M6 9l6 6 6-6",
        "chevron-up" => "M18 15l-6-6-6 6",
        "chevron-right" => "M9 18l6-6-6-6",
        "chevron-left" => "M15 18l-6-6 6-6",
        "search" => "M11 3a8 8 0 1 0 0.01 0 M21 21l-4.35-4.35",
        "filter" => "M22 3H2l8 9.46V19l4 2v-8.54L22 3z",
        "sort" => "M3 16l4 4 4-4 M7 20V4 M21 8l-4-4-4 4 M17 4v16",
        "group" => "M12 2L2 7l10 5 10-5-10-5z M2 17l10 5 10-5 M2 12l10 5 10-5",
        "rowheight" => "M3 6h18 M3 12h18 M3 18h18",
        "eye" => "M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z M12 9a3 3 0 1 0 0.01 0",
        "eye-off" => "M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94 M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19 M14.12 14.12a3 3 0 1 1-4.24-4.24 M1 1l22 22",
        "fields" => "M3 3h18v18H3z M9 3v18 M15 3v18",
        "more" => "M5 12h.01 M12 12h.01 M19 12h.01",
        "more-v" => "M12 5h.01 M12 12h.01 M12 19h.01",
        "expand" => "M15 3h6v6 M9 21H3v-6 M21 3l-7 7 M3 21l7-7",
        "trash" => "M3 6h18 M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6 M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2 M10 11v6 M14 11v6",
        "copy" => "M9 9h11a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H11a2 2 0 0 1-2-2v-9a2 2 0 0 1 2-2z M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1",
        "edit" => "M12 20h9 M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z",
        "grip" => "M9 5h.01 M9 12h.01 M9 19h.01 M15 5h.01 M15 12h.01 M15 19h.01",
        "table" => "M3 3h18v18H3z M3 9h18 M3 15h18 M9 3v18",
        "grid" => "M3 3h18v18H3z M3 9h18 M3 15h18 M9 3v18",
        "kanban" => "M3 3h18v18H3z M9 3v18 M15 3v18 M3 3",
        "gallery" => "M3 3h7v7H3z M14 3h7v7h-7z M14 14h7v7h-7z M3 14h7v7H3z",
        "gantt" => "M3 3v18h18 M7 7h8 M10 12h8 M7 17h6",
        "calendar" => "M5 4h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z M16 2v4 M8 2v4 M3 10h18",
        "form" => "M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2 M9 2h6v4H9z M8 11h8 M8 16h5",
        "dashboard" => "M3 3h7v9H3z M14 3h7v5h-7z M14 12h7v9h-7z M3 16h7v5H3z",
        "folder" => "M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z",
        "undo" => "M3 7v6h6 M21 17a9 9 0 0 0-15-6.7L3 13",
        "redo" => "M21 7v6h-6 M3 17a9 9 0 0 1 15-6.7L21 13",
        "share" => "M4 12v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8 M16 6l-4-4-4 4 M12 2v13",
        "star" => "M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z",
        "bell" => "M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9 M13.73 21a2 2 0 0 1-3.46 0",
        "sidebar" => "M3 3h18v18H3z M9 3v18",
        "upload" => "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4 M17 8l-5-5-5 5 M12 3v12",
        "download" => "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4 M7 10l5 5 5-5 M12 15V3",
        "insert-left" => "M14 3h7v18h-7z M3 12h7 M6.5 8.5v7",
        "insert-right" => "M3 3h7v18H3z M14 12h7 M17.5 8.5v7",
        "insert-above" => "M3 14h18v7H3z M12 3v7 M8.5 6.5h7",
        "insert-below" => "M3 3h18v7H3z M12 14v7 M8.5 17.5h7",
        "sort-asc" => "M3 8l4-4 4 4 M7 4v16 M13 12h8 M13 16h6 M13 20h4",
        "sort-desc" => "M3 16l4 4 4-4 M7 20V4 M13 4h8 M13 8h6 M13 12h4",
        "freeze" => "M12 2v20 M17 5l-5 5-5-5 M17 19l-5-5-5 5",
        "info" => "M12 2a10 10 0 1 0 0.01 0 M12 16v-4 M12 8h.01",
        "comment" => "M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z",
        "history" => "M3 3v5h5 M3.05 13A9 9 0 1 0 6 5.3L3 8 M12 7v5l4 2",
        "robot" => "M5 8h14a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-9a2 2 0 0 1 2-2z M12 4v4 M9 14h.01 M15 14h.01",
        "palette" => "M12 2a10 10 0 1 0 0 20c1 0 2-1 2-2s-1-1-1-2 1-2 2-2h2a5 5 0 0 0 5-5c0-5-4.5-9-10-9z M7.5 10.5h.01 M10.5 6.5h.01 M15.5 7.5h.01",
        "file" => "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z M14 2v6h6",
        "image" => "M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z M8.5 7a1.5 1.5 0 1 0 0.01 0 M21 15l-5-5L5 21",
        "home" => "M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z M9 22V12h6v10",
        "keyboard" => "M4 6h16a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2z M6 10h.01 M10 10h.01 M14 10h.01 M18 10h.01 M7 14h10",
        "wrap" => "M3 6h18 M3 12h15a3 3 0 1 1 0 6h-4 M16 16l-2 2 2 2 M3 18h7",
        _ => "M4 4h16v16H4z",
    }
}

pub fn kind_icon(tag: KindTag) -> &'static str {
    match tag {
        KindTag::Text => "text",
        KindTag::SingleSelect => "single",
        KindTag::MultiSelect => "multi",
        KindTag::User => "user",
        KindTag::Date => "date",
        KindTag::Attachment => "attachment",
        KindTag::Number => "number",
        KindTag::Checkbox => "checkbox",
        KindTag::Link => "link",
        KindTag::DuplexLink => "duplex",
        KindTag::Lookup => "lookup",
        KindTag::Formula => "formula",
        KindTag::Url => "url",
        KindTag::Phone => "phone",
        KindTag::Email => "email",
        KindTag::Location => "location",
        KindTag::AutoNumber => "autonumber",
        KindTag::Currency => "currency",
        KindTag::Progress => "progress",
        KindTag::Rating => "rating",
        KindTag::Barcode => "barcode",
        KindTag::CreatedTime | KindTag::ModifiedTime => "clock",
        KindTag::CreatedBy | KindTag::ModifiedBy => "userpen",
    }
}

pub fn field_icon(kind: &FieldKind) -> &'static str {
    kind_icon(kind.tag())
}

pub fn view_icon(kind: ViewKind) -> &'static str {
    match kind {
        ViewKind::Grid => "grid",
        ViewKind::Kanban => "kanban",
        ViewKind::Gallery => "gallery",
        ViewKind::Gantt => "gantt",
        ViewKind::Calendar => "calendar",
        ViewKind::Form => "form",
    }
}

/// Draws icon `name` at (x, y), `size` points wide, into `p`.
pub fn draw(p: Painter, name: &str, x: f32, y: f32, size: f32, color: u32) -> Painter {
    let s = size / 24.0;
    p.save()
        .translate(x, y)
        .scale(s, s)
        .stroke_color(rgb(color))
        .line_width(2.0_f32.max(1.6 / s.max(0.1) * 0.75))
        .line_cap(nsh_core::canvas::LineCap::Round)
        .line_join(nsh_core::canvas::LineJoin::Round)
        .stroke_svg(path(name))
        .restore()
}

/// An icon element.
pub fn icon(name: &'static str, size: f32, color: u32) -> CanvasView {
    canvas(move || draw(Painter::new(), name, 0.0, 0.0, size, color)).size(px(size)).flex_shrink_0()
}

/// An icon element whose name and color follow signals.
pub fn icon_dyn(name: impl Fn() -> &'static str + 'static, size: f32, color: impl Fn() -> u32 + 'static) -> CanvasView {
    canvas(move || draw(Painter::new(), name(), 0.0, 0.0, size, color())).size(px(size)).flex_shrink_0()
}
