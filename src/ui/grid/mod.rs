//! The grid view (表格视图): a canvas that draws the visible cells, with
//! selection, editing, column resizing and moving, row dragging, fill,
//! grouping, statistics and the keyboard.

mod draw;
pub mod edit;
mod input;
pub mod keys;

use nsh_core::prelude::*;

use super::state::st;
use crate::model::{FieldKind, Id, Stat, query::Row};

pub const HEAD: f32 = 36.0;
pub const RH: f32 = 72.0;
pub const STATS: f32 = 34.0;
pub const ADD_COL: f32 = 96.0;
pub const GROUP_H: f32 = 42.0;
pub const ADD_H: f32 = 34.0;
pub const SCROLLBAR: f32 = 10.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Col {
    pub field: Id,
    pub name: String,
    pub kind: FieldKind,
    /// Offset from the first column's start.
    pub x: f32,
    pub w: f32,
    pub stat: Stat,
    pub desc: String,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Layout {
    pub table: Id,
    pub view: Id,
    pub cols: Vec<Col>,
    pub frozen: usize,
    pub frozen_w: f32,
    pub rows: Vec<Row>,
    pub tops: Vec<f32>,
    pub heights: Vec<f32>,
    pub content_h: f32,
    pub content_w: f32,
    pub row_h: f32,
    /// The records shown, in order (for statistics and the count).
    pub records: Vec<usize>,
    pub record_ids: Vec<Id>,
    pub sorted: bool,
    pub grouped: bool,
    pub wrap: bool,
    /// Cells holding the search text: (row, column).
    pub matches: Vec<(usize, usize)>,
}

impl Layout {
    pub fn record_at(&self, row: usize) -> Option<usize> {
        match self.rows.get(row) {
            Some(Row::Record(i)) => Some(*i),
            _ => None,
        }
    }
    pub fn is_record(&self, row: usize) -> bool {
        self.record_at(row).is_some()
    }
    pub fn row_at_y(&self, y: f32) -> Option<usize> {
        if y < 0.0 {
            return None;
        }
        let i = self.tops.partition_point(|t| *t <= y);
        if i == 0 {
            return None;
        }
        let r = i - 1;
        (y < self.tops[r] + self.heights[r]).then_some(r)
    }
    /// The nearest record row at or after (`dir` 1) / before (-1) `row`.
    pub fn next_record(&self, row: isize, dir: isize) -> Option<usize> {
        let mut r = row;
        while r >= 0 && (r as usize) < self.rows.len() {
            if self.is_record(r as usize) {
                return Some(r as usize);
            }
            r += dir;
        }
        None
    }
    pub fn first_record(&self) -> Option<usize> {
        self.next_record(0, 1)
    }
    pub fn last_record(&self) -> Option<usize> {
        self.next_record(self.rows.len() as isize - 1, -1)
    }
}

/// The selection: the active cell (anchor) and the range's other corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sel {
    pub ar: usize,
    pub ac: usize,
    pub fr: usize,
    pub fc: usize,
    /// Whole columns selected (a header pressed).
    pub cols: bool,
}

impl Sel {
    pub fn cell(r: usize, c: usize) -> Sel {
        Sel { ar: r, ac: c, fr: r, fc: c, cols: false }
    }
    pub fn rows(&self) -> (usize, usize) {
        (self.ar.min(self.fr), self.ar.max(self.fr))
    }
    pub fn cols(&self) -> (usize, usize) {
        (self.ac.min(self.fc), self.ac.max(self.fc))
    }
    pub fn single(&self) -> bool {
        self.ar == self.fr && self.ac == self.fc
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditMode {
    Text,
    Select,
    Date,
    User,
    Link,
    Attachment,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Editing {
    pub row: usize,
    pub col: usize,
    pub record: Id,
    pub field: Id,
    pub mode: EditMode,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Drag {
    None,
    Select,
    Resize { col: usize, start_x: f32, start_w: f32 },
    MoveCol { col: usize, start_x: f32, to: usize, moved: bool },
    MoveRow { row: usize, to: usize, start_y: f32, moved: bool },
    Fill { to_row: usize },
    VBar { start_y: f32, start_sy: f32 },
    HBar { start_x: f32, start_sx: f32 },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    None,
    Corner,
    Header(usize),
    HeaderMenu(usize),
    Resize(usize),
    AddField,
    RowCheck(usize),
    /// The "查看" button of the primary cell under the pointer.
    View(usize),
    RowGrip(usize),
    RowNum(usize),
    Cell(usize, usize),
    /// Clickable URL text or attachment thumbnail (file index).
    CellOpen(usize, usize, Option<usize>),
    CellCheckbox(usize, usize),
    CellAdd(usize, usize),
    CellRating(usize, usize, u8),
    CellButton(usize, usize),
    AddRow(usize),
    Group(usize),
    StatCount,
    Stat(usize),
    Fill,
    VBar,
    HBar,
}

#[derive(Clone, Copy)]
pub struct Grid {
    pub layout: Memo<Layout>,
    pub sx: Signal<f32>,
    pub sy: Signal<f32>,
    pub size: Signal<(f32, f32)>,
    /// The canvas's top left in the window.
    pub origin: Signal<(f32, f32)>,
    pub sel: Signal<Option<Sel>>,
    pub editing: Signal<Option<Editing>>,
    pub edit_text: Signal<String>,
    pub hover: Signal<Hit>,
    pub hover_row: Signal<Option<usize>>,
    pub drag: Signal<Drag>,
    pub resize_w: Signal<Option<(usize, f32)>>,
    pub focused: Signal<bool>,
    pub input: ElementRef,
    pub last_press: Stored<(f64, Hit)>,
    /// Search text typed into an option list editor.
    pub list_query: Signal<String>,
    pub list_index: Signal<usize>,
    /// The search match the grid is at.
    pub match_at: Signal<usize>,
}

thread_local! {
    static GRID: std::cell::RefCell<Option<Grid>> = const { std::cell::RefCell::new(None) };
    static GRID_ID: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

pub fn grid() -> Option<Grid> {
    GRID.with(|g| *g.borrow())
}

fn make_layout(resize: Option<(usize, f32)>) -> Layout {
    let s = st();
    let search = s.search.get();
    s.with_view(|b, t, v| {
        let fields: Vec<_> = t.view_fields(v).into_iter().filter(|(_, h)| !h).map(|(f, _)| f).collect();
        let mut cols = vec![];
        let mut x = 0.0;
        for (i, f) in fields.iter().enumerate() {
            let mut w = v.widths.get(&f.id).copied().unwrap_or(if i == 0 { 180.0 } else { 150.0 });
            if let Some((c, rw)) = resize {
                if c == i {
                    w = rw;
                }
            }
            let default_stat = Stat::None;
            cols.push(Col {
                field: f.id.clone(),
                name: f.name.clone(),
                kind: f.kind.clone(),
                x,
                w,
                stat: v.stats.get(&f.id).copied().unwrap_or(default_stat),
                desc: f.description.clone(),
            });
            x += w;
        }
        let frozen = v.frozen.min(cols.len());
        let frozen_w = cols.iter().take(frozen).map(|c| c.w).sum();
        // The grid's search highlights; it does not filter.
        let rows = crate::model::query::rows(b, t, v, "");
        let row_h = v.row_height.px();
        let mut tops = Vec::with_capacity(rows.len());
        let mut heights = Vec::with_capacity(rows.len());
        let mut y = 0.0;
        let mut records = vec![];
        for r in &rows {
            let h = match r {
                Row::Record(i) => {
                    records.push(*i);
                    row_h
                }
                Row::Group { .. } => GROUP_H,
                Row::Add { .. } => ADD_H,
            };
            tops.push(y);
            heights.push(h);
            y += h;
        }
        let record_ids = records.iter().map(|&i| t.records[i].id.clone()).collect();
        let mut matches = vec![];
        let q = search.trim().to_lowercase();
        if !q.is_empty() {
            for (ri, row) in rows.iter().enumerate() {
                if let Row::Record(i) = row {
                    let rec = &t.records[*i];
                    for (ci, f) in fields.iter().enumerate() {
                        let hit = crate::model::query::cell(b, t, rec, f)
                            .map(|v| crate::model::value::display(b, t, &f.kind, &v).to_lowercase().contains(&q))
                            .unwrap_or(false);
                        if hit {
                            matches.push((ri, ci));
                        }
                    }
                }
            }
        }
        Layout {
            table: t.id.clone(),
            view: v.id.clone(),
            cols,
            frozen,
            frozen_w,
            rows,
            tops,
            heights,
            content_h: y,
            content_w: x + ADD_COL,
            row_h,
            records,
            record_ids,
            sorted: !v.sorts.is_empty(),
            grouped: !v.groups.is_empty(),
            wrap: v.wrap,
            matches,
        }
    })
    .unwrap_or_default()
}

impl Grid {
    pub fn new() -> Grid {
        let resize_w = signal(None);
        let g = Grid {
            layout: computed(move || make_layout(resize_w.get())),
            sx: signal(0.0),
            sy: signal(0.0),
            size: signal((0.0, 0.0)),
            origin: signal((0.0, 0.0)),
            sel: signal(None),
            editing: signal(None),
            edit_text: signal(String::new()),
            hover: signal(Hit::None),
            hover_row: signal(None),
            drag: signal(Drag::None),
            resize_w,
            focused: signal(false),
            input: element_ref(),
            last_press: stored((0.0, Hit::None)),
            list_query: signal(String::new()),
            list_index: signal(0),
            match_at: signal(0),
        };
        GRID.with(|x| *x.borrow_mut() = Some(g));
        let id = GRID_ID.with(|i| {
            i.set(i.get() + 1);
            i.get()
        });
        cleanup(move || {
            if GRID_ID.with(|i| i.get()) == id {
                GRID.with(|x| *x.borrow_mut() = None);
                crate::win::set_key_mode(crate::win::mode::FREE);
            }
        });
        g
    }

    pub fn body_h(&self) -> f32 {
        (self.size.get_untracked().1 - HEAD - STATS).max(0.0)
    }

    pub fn max_sy(&self, l: &Layout) -> f32 {
        (l.content_h + 40.0 - self.body_h()).max(0.0)
    }

    pub fn max_sx(&self, l: &Layout) -> f32 {
        let scroll_w = (self.size.get_untracked().0 - RH - l.frozen_w).max(0.0);
        (l.content_w - l.frozen_w - scroll_w + 40.0).max(0.0)
    }

    pub fn scroll_by(&self, dx: f32, dy: f32) {
        let l = self.layout.get_untracked();
        let nx = (self.sx.get_untracked() + dx).clamp(0.0, self.max_sx(&l));
        let ny = (self.sy.get_untracked() + dy).clamp(0.0, self.max_sy(&l));
        if nx != self.sx.get_untracked() {
            self.sx.set(nx);
        }
        if ny != self.sy.get_untracked() {
            self.sy.set(ny);
        }
    }

    /// The screen x of column `c` (canvas coordinates).
    pub fn col_x(&self, l: &Layout, c: usize) -> f32 {
        let col = &l.cols[c];
        if c < l.frozen { RH + col.x } else { RH + col.x - self.sx.get_untracked() }
    }

    pub fn row_y(&self, l: &Layout, r: usize) -> f32 {
        HEAD + l.tops[r] - self.sy.get_untracked()
    }

    /// The box of cell (r, c) in canvas coordinates.
    pub fn cell_rect(&self, l: &Layout, r: usize, c: usize) -> (f32, f32, f32, f32) {
        (self.col_x(l, c), self.row_y(l, r), l.cols[c].w, l.heights[r])
    }

    /// The column under screen x (`None` past the last).
    pub fn col_at_x(&self, l: &Layout, x: f32) -> Option<usize> {
        if x < RH {
            return None;
        }
        let fx = x - RH;
        for c in 0..l.frozen {
            let col = &l.cols[c];
            if fx >= col.x && fx < col.x + col.w {
                return Some(c);
            }
        }
        if fx < l.frozen_w {
            return None;
        }
        let sx = fx + self.sx.get_untracked();
        (l.frozen..l.cols.len()).find(|&c| sx >= l.cols[c].x && sx < l.cols[c].x + l.cols[c].w)
    }

    pub fn hit(&self, x: f32, y: f32) -> Hit {
        let l = self.layout.get_untracked();
        let (w, h) = self.size.get_untracked();
        // Scroll bars.
        if x > w - SCROLLBAR && y > HEAD && y < h - STATS && self.max_sy(&l) > 0.0 {
            return Hit::VBar;
        }
        if y > h - STATS - SCROLLBAR && y < h - STATS && x > RH + l.frozen_w && self.max_sx(&l) > 0.0 {
            return Hit::HBar;
        }
        if y >= h - STATS {
            if x < RH + l.cols.first().map(|c| c.w).unwrap_or(0.0) {
                return Hit::StatCount;
            }
            return self.col_at_x(&l, x).map(Hit::Stat).unwrap_or(Hit::None);
        }
        if y < HEAD {
            if x < RH {
                return Hit::Corner;
            }
            // Resize edges first (4 points either side of a column's end).
            for c in 0..l.cols.len() {
                let right = self.col_x(&l, c) + l.cols[c].w;
                if c >= l.frozen && right < RH + l.frozen_w + 4.0 {
                    continue;
                }
                if (x - right).abs() <= 4.0 {
                    return Hit::Resize(c);
                }
            }
            if let Some(c) = self.col_at_x(&l, x) {
                let right = self.col_x(&l, c) + l.cols[c].w;
                if x > right - 28.0 {
                    return Hit::HeaderMenu(c);
                }
                return Hit::Header(c);
            }
            let end = if l.cols.len() > l.frozen {
                RH + l.cols.last().map(|c| c.x + c.w).unwrap_or(0.0) - self.sx.get_untracked()
            } else {
                RH + l.frozen_w
            };
            if x >= end && x < end + ADD_COL {
                return Hit::AddField;
            }
            return Hit::None;
        }
        let cy = y - HEAD + self.sy.get_untracked();
        let Some(r) = l.row_at_y(cy) else { return Hit::None };
        match &l.rows[r] {
            Row::Group { .. } => return Hit::Group(r),
            Row::Add { .. } => {
                return if x < RH + l.content_w - ADD_COL - self.sx.get_untracked() + l.frozen_w.min(0.0) {
                    Hit::AddRow(r)
                } else {
                    Hit::None
                };
            }
            Row::Record(_) => {}
        }
        // The fill handle at the selection's bottom right.
        if let Some(sel) = self.sel.get_untracked() {
            if self.editing.get_untracked().is_none() {
                let (_, r1) = sel.rows();
                let (_, c1) = sel.cols();
                if r1 < l.rows.len() && c1 < l.cols.len() {
                    let (cx, cy0, cw, ch) = self.cell_rect(&l, r1, c1);
                    if (x - (cx + cw)).abs() <= 5.0 && (y - (cy0 + ch)).abs() <= 5.0 {
                        return Hit::Fill;
                    }
                }
            }
        }
        if x < RH {
            let ry = self.row_y(&l, r);
            let _ = ry;
            if x < 18.0 {
                return Hit::RowGrip(r);
            }
            if x < 46.0 {
                return Hit::RowCheck(r);
            }
            return Hit::RowNum(r);
        }
        let Some(c) = self.col_at_x(&l, x) else { return Hit::None };
        let (cx, cy0, cw, ch) = self.cell_rect(&l, r, c);
        if c == 0 && self.hover_row.get_untracked() == Some(r) && x > cx + cw - 62.0 && x < cx + cw - 8.0 && (y - (cy0 + ch.min(33.0) / 2.0)).abs() <= 11.0 {
            return Hit::View(r);
        }
        let mid = cy0 + ch.min(33.0) / 2.0;
        match &l.cols[c].kind {
            FieldKind::Checkbox => {
                if x >= cx + 6.0 && x <= cx + 28.0 && (y - mid).abs() <= 11.0 {
                    return Hit::CellCheckbox(r, c);
                }
            }
            FieldKind::Button { label, .. } => {
                let w = crate::ui::cells::text_w(label, 12.0) + 24.0;
                if x >= cx + 8.0 && x <= cx + 8.0 + w && (y - mid).abs() <= 12.0 {
                    return Hit::CellButton(r, c);
                }
            }
            FieldKind::Rating { max, .. } => {
                let rx = x - (cx + 8.0);
                if rx >= 0.0 && (y - mid).abs() <= 10.0 {
                    let i = (rx / 20.0) as u8;
                    if i < *max {
                        return Hit::CellRating(r, c, i + 1);
                    }
                }
            }
            FieldKind::Attachment | FieldKind::Link { .. } | FieldKind::User { .. } => {
                // The "+" shown in the selected cell.
                let selected = self.sel.get_untracked().is_some_and(|s| s.ar == r && s.ac == c && s.single());
                if selected && x > cx + cw - 30.0 && (y - mid).abs() <= 12.0 {
                    return Hit::CellAdd(r, c);
                }
            }
            _ => {}
        }
        // Only the rendered link text / thumbnail opens a resource; blank space
        // keeps the normal cell selection and editing behavior.
        if self.editing.get_untracked().is_none() && matches!(l.cols[c].kind, FieldKind::Url | FieldKind::Attachment) {
            let resource = st().peek(|b| {
                let t = b.table(&l.table)?;
                let rec = t.records.get(l.record_at(r)?)?;
                let f = t.field(&l.cols[c].field)?;
                let v = crate::model::query::cell(b, t, rec, f)?;
                crate::ui::cells::resource_at(&v, x - cx, y - cy0, cw, ch)
            });
            if let Some(file) = resource {
                return Hit::CellOpen(r, c, file);
            }
        }
        Hit::Cell(r, c)
    }

    /// Moves to the next (or previous) search match.
    pub fn next_match(&self, back: bool) {
        let l = self.layout.get_untracked();
        let n = l.matches.len();
        if n == 0 {
            return;
        }
        let i = self.match_at.get_untracked().min(n - 1);
        let ni = if back { (i + n - 1) % n } else { (i + 1) % n };
        self.match_at.set(ni);
        let (r, c) = l.matches[ni];
        self.sel.set(Some(Sel::cell(r, c)));
        self.reveal(r, c);
    }

    /// Scrolls so cell (r, c) shows.
    pub fn reveal(&self, r: usize, c: usize) {
        let l = self.layout.get_untracked();
        if r >= l.rows.len() {
            return;
        }
        let body = self.body_h() - SCROLLBAR;
        let top = l.tops[r];
        let bottom = top + l.heights[r];
        let sy = self.sy.get_untracked();
        if top < sy {
            self.sy.set(top);
        } else if bottom > sy + body {
            self.sy.set((bottom - body).min(self.max_sy(&l)).max(0.0));
        }
        if c >= l.frozen && c < l.cols.len() {
            let (w, _) = self.size.get_untracked();
            let view_w = w - RH - l.frozen_w - SCROLLBAR;
            let left = l.cols[c].x - l.frozen_w;
            let right = left + l.cols[c].w;
            let sx = self.sx.get_untracked();
            if left < sx {
                self.sx.set(left.max(0.0));
            } else if right > sx + view_w {
                self.sx.set((right - view_w).min(self.max_sx(&l)).max(0.0));
            }
        }
    }

    /// The active cell's record and field.
    pub fn active(&self) -> Option<(Id, Id, usize, usize)> {
        let sel = self.sel.get_untracked()?;
        let l = self.layout.get_untracked();
        let ri = l.record_at(sel.ar)?;
        let col = l.cols.get(sel.ac)?;
        let rid = st().peek(|b| b.table(&l.table).map(|t| t.records[ri].id.clone()))?;
        Some((rid, col.field.clone(), sel.ar, sel.ac))
    }

    /// Records in the selected rows (or the ticked ones).
    pub fn selected_records(&self) -> Vec<Id> {
        let checked = st().checked.get_untracked();
        if !checked.is_empty() {
            return checked;
        }
        let Some(sel) = self.sel.get_untracked() else { return vec![] };
        let l = self.layout.get_untracked();
        let (r0, r1) = sel.rows();
        let ids = st().peek(|b| {
            b.table(&l.table)
                .map(|t| (r0..=r1).filter_map(|r| l.record_at(r)).map(|i| t.records[i].id.clone()).collect::<Vec<_>>())
                .unwrap_or_default()
        });
        ids
    }

    /// Selects the cell of `record` and `field` if it shows.
    pub fn select_record(&self, record: &str, col: usize) {
        let l = self.layout.get_untracked();
        if let Some(r) = l.rows.iter().position(|row| matches!(row, Row::Record(i) if l.record_ids.get(l.records.iter().position(|x| x == i).unwrap_or(usize::MAX)).is_some_and(|id| id == record))) {
            self.sel.set(Some(Sel::cell(r, col.min(l.cols.len().saturating_sub(1)))));
            self.reveal(r, col);
        }
    }
}

pub use input::grid_view;
