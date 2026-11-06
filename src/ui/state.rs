//! The app's state: the document (with undo and saving) and what the UI
//! shows (the table and view open, the popup, the record expanded).

use std::cell::RefCell;

use nsh_core::prelude::*;

use crate::model::{Base, Id, Table, Value, View, ViewKind, query, sample};

/// A popup the app shows, placed at (x, y) of the window.
#[derive(Clone, Debug, PartialEq)]
pub enum Popup {
    FieldMenu { field: Id, x: f32, y: f32 },
    /// Creating (`field: None`, inserted at `at` among the view's fields) or
    /// editing a field.
    FieldEditor { field: Option<Id>, at: Option<usize>, x: f32, y: f32 },
    RecordMenu { x: f32, y: f32 },
    ViewMenu { view: Id, x: f32, y: f32 },
    AddView { x: f32, y: f32 },
    Filter { x: f32, y: f32 },
    Sort { x: f32, y: f32 },
    Group { x: f32, y: f32 },
    Fields { x: f32, y: f32 },
    RowHeight { x: f32, y: f32 },
    Stat { field: Id, x: f32, y: f32 },
    SidebarMenu { item: Id, x: f32, y: f32 },
    SidebarAdd { x: f32, y: f32 },
    More { x: f32, y: f32 },
    Shortcuts,
    Workflows,
    Confirm { title: String, text: String, action: Confirm },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Confirm {
    DeleteField(Id),
    DeleteRecords(Vec<Id>),
    DeleteView(Id),
    DeleteItem(Id),
}

#[derive(Clone, Copy)]
pub struct St {
    pub doc: Stored<Base>,
    /// Bumps on every change of the document.
    pub rev: Signal<u64>,
    undo: Stored<Vec<Base>>,
    redo: Stored<Vec<Base>>,
    /// The sidebar item open: a table or a dashboard.
    pub cur: Signal<Id>,
    /// The view open in the current table.
    pub view: Signal<Id>,
    /// The grid's search (Ctrl+F).
    pub search: Signal<String>,
    pub search_open: Signal<bool>,
    pub popup: Signal<Option<Popup>>,
    pub toast: Signal<Option<(String, u64)>>,
    /// The record shown in the record card: (table, record).
    pub expanded: Signal<Option<(Id, Id)>>,
    pub sidebar: Signal<bool>,
    /// Records ticked in the grid's row header.
    pub checked: Signal<Vec<Id>>,
    pub notices: Signal<Vec<String>>,
    /// The window's size in points.
    pub window: Signal<(f32, f32)>,
    saving: Stored<bool>,
}

thread_local! {
    static ST: RefCell<Option<St>> = const { RefCell::new(None) };
}

pub fn st() -> St {
    ST.with(|s| s.borrow().expect("state"))
}

pub fn data_path() -> std::path::PathBuf {
    if let Some(p) = crate::args::data_file() {
        return p.into();
    }
    let dir = std::env::var("APPDATA").map(std::path::PathBuf::from).unwrap_or_else(|_| ".".into()).join("Rutable");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("base.json")
}

/// The folder attachments are copied to.
pub fn files_dir() -> std::path::PathBuf {
    let dir = data_path().parent().map(|p| p.join("files")).unwrap_or_else(|| "files".into());
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn load() -> Base {
    if crate::args::fresh() {
        return sample::sample();
    }
    if let Ok(text) = std::fs::read_to_string(data_path()) {
        match serde_json::from_str::<Base>(&text) {
            Ok(b) if !b.tables.is_empty() => return b,
            Ok(_) => {}
            Err(e) => log::error!("cannot read the document: {e}"),
        }
    }
    sample::sample()
}

impl St {
    pub fn init() -> St {
        let base = load();
        let first = base.sidebar().first().cloned().unwrap_or_default();
        let view = base.table(&first).and_then(|t| t.views.first()).map(|v| v.id.clone()).unwrap_or_default();
        let s = St {
            doc: stored(base),
            rev: signal(0),
            undo: stored(vec![]),
            redo: stored(vec![]),
            cur: signal(first),
            view: signal(view),
            search: signal(String::new()),
            search_open: signal(false),
            popup: signal(None),
            toast: signal(None),
            expanded: signal(None),
            sidebar: signal(true),
            checked: signal(vec![]),
            notices: signal(vec![]),
            window: signal((1440.0, 900.0)),
            saving: stored(false),
        };
        ST.with(|x| *x.borrow_mut() = Some(s));
        s
    }

    /// Reads the document, tracking its changes.
    pub fn read<R>(&self, f: impl FnOnce(&Base) -> R) -> R {
        self.rev.get();
        self.doc.with(|b| f(b))
    }

    /// Reads the document without tracking.
    pub fn peek<R>(&self, f: impl FnOnce(&Base) -> R) -> R {
        self.doc.with(|b| f(b))
    }

    /// Changes the document as one undoable step.
    pub fn edit<R>(&self, f: impl FnOnce(&mut Base) -> R) -> R {
        let snapshot = self.doc.with(|b| b.clone());
        self.undo.with(|u| {
            u.push(snapshot);
            if u.len() > 100 {
                u.remove(0);
            }
        });
        self.redo.with(|r| r.clear());
        let out = self.doc.with(f);
        self.changed();
        out
    }

    fn changed(&self) {
        query::invalidate();
        self.rev.update(|r| *r += 1);
        self.schedule_save();
    }

    pub fn can_undo(&self) -> bool {
        self.rev.get();
        self.undo.with(|u| !u.is_empty())
    }

    pub fn can_redo(&self) -> bool {
        self.rev.get();
        self.redo.with(|u| !u.is_empty())
    }

    pub fn undo(&self) {
        let Some(prev) = self.undo.with(|u| u.pop()) else {
            self.toast("没有可撤销的操作");
            return;
        };
        let now = self.doc.with(|b| std::mem::replace(b, prev));
        self.redo.with(|r| r.push(now));
        self.changed();
        self.fix_selection();
    }

    pub fn redo(&self) {
        let Some(next) = self.redo.with(|u| u.pop()) else {
            self.toast("没有可重做的操作");
            return;
        };
        let now = self.doc.with(|b| std::mem::replace(b, next));
        self.undo.with(|r| r.push(now));
        self.changed();
        self.fix_selection();
    }

    /// Keeps the open table and view existing after an undo or a delete.
    pub fn fix_selection(&self) {
        let cur = self.cur.get_untracked();
        let (cur, view) = self.peek(|b| {
            let items = b.sidebar();
            let cur = if items.contains(&cur) { cur } else { items.first().cloned().unwrap_or_default() };
            let view = b.table(&cur).map(|t| {
                let v = self.view.get_untracked();
                if t.view(&v).is_some() { v } else { t.views.first().map(|v| v.id.clone()).unwrap_or_default() }
            });
            (cur, view)
        });
        if self.cur.get_untracked() != cur {
            self.cur.set(cur);
        }
        if let Some(v) = view {
            if self.view.get_untracked() != v {
                self.view.set(v);
            }
        }
    }

    fn schedule_save(&self) {
        if self.saving.with(|s| std::mem::replace(s, true)) {
            return;
        }
        let me = *self;
        set_timeout(std::time::Duration::from_millis(600), move || {
            me.saving.with(|s| *s = false);
            me.save_now();
        });
    }

    pub fn save_now(&self) {
        let json = self.doc.with(|b| serde_json::to_string(b));
        if let Ok(json) = json {
            let path = data_path();
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, json).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }

    pub fn toast(&self, text: &str) {
        let id = self.toast.get_untracked().map(|t| t.1 + 1).unwrap_or(1);
        self.toast.set(Some((text.to_string(), id)));
        let me = *self;
        set_timeout(std::time::Duration::from_millis(2200), move || {
            if me.toast.get_untracked().map(|t| t.1) == Some(id) {
                me.toast.set(None);
            }
        });
    }

    /// The current table's id ("" on a dashboard).
    pub fn table_id(&self) -> Id {
        let cur = self.cur.get();
        if self.read(|b| b.table(&cur).is_some()) { cur } else { String::new() }
    }

    pub fn with_table<R>(&self, f: impl FnOnce(&Base, &Table) -> R) -> Option<R> {
        let cur = self.cur.get();
        self.read(|b| b.table(&cur).map(|t| f(b, t)))
    }

    pub fn with_view<R>(&self, f: impl FnOnce(&Base, &Table, &View) -> R) -> Option<R> {
        let cur = self.cur.get();
        let v = self.view.get();
        self.read(|b| b.table(&cur).and_then(|t| t.view(&v).or(t.views.first()).map(|vv| f(b, t, vv))))
    }

    pub fn view_kind(&self) -> Option<ViewKind> {
        self.with_view(|_, _, v| v.kind)
    }

    /// Changes the current view (one undo step).
    pub fn edit_view(&self, f: impl FnOnce(&mut View)) {
        let cur = self.cur.get_untracked();
        let vid = self.view.get_untracked();
        self.edit(|b| {
            if let Some(v) = b.table_mut(&cur).and_then(|t| t.view_mut(&vid)) {
                f(v)
            }
        });
    }

    /// Changes the current table (one undo step).
    pub fn edit_table<R>(&self, f: impl FnOnce(&mut Table, &str) -> R) -> Option<R> {
        let cur = self.cur.get_untracked();
        self.edit(|b| {
            let me = b.me.clone();
            b.table_mut(&cur).map(|t| f(t, &me))
        })
    }

    pub fn open(&self, item: &str) {
        self.popup.set(None);
        self.cur.set(item.to_string());
        let view = self.peek(|b| b.table(item).and_then(|t| t.views.first()).map(|v| v.id.clone()));
        if let Some(v) = view {
            self.view.set(v);
        }
        self.checked.set(vec![]);
        self.search.set(String::new());
        self.search_open.set(false);
    }

    /// Sets a cell (one undo step), recording the change in the record's
    /// history and keeping duplex links in step.
    pub fn set_cell(&self, record: &str, field: &str, value: Option<Value>) {
        let table = self.cur.get_untracked();
        self.set_cells(&table, &[(record.to_string(), field.to_string(), value)]);
    }

    /// Sets several cells of `table` as one step.
    pub fn set_cells(&self, table: &str, cells: &[(Id, Id, Option<Value>)]) {
        let table = table.to_string();
        let mut fired = vec![];
        self.edit(|b| {
            let me = b.me.clone();
            let mut mirrors = vec![];
            {
                let Some(t) = b.tables.iter().find(|t| t.id == table) else { return };
                let t = t.clone();
                for (rid, fid, v) in cells {
                    let Some(f) = t.field(fid) else { continue };
                    if f.kind.is_computed() {
                        continue;
                    }
                    if let crate::model::FieldKind::Link { pair: Some(p), .. } = &f.kind {
                        if !p.is_empty() {
                            let old: Vec<Id> = t.record(rid).and_then(|r| r.cells.get(fid)).map(|v| v.ids().to_vec()).unwrap_or_default();
                            let new: Vec<Id> = v.as_ref().map(|v| v.ids().to_vec()).unwrap_or_default();
                            mirrors.push((fid.clone(), rid.clone(), old, new));
                        }
                    }
                }
            }
            let base_snapshot = b.clone();
            if let Some(t) = b.table_mut(&table) {
                let tsnap = base_snapshot.table(&table).unwrap();
                for (rid, fid, v) in cells {
                    let Some(f) = tsnap.field(fid) else { continue };
                    if f.kind.is_computed() {
                        continue;
                    }
                    let Some(r) = t.record_mut(rid) else { continue };
                    let old = r.cells.get(fid).cloned();
                    if old == *v {
                        continue;
                    }
                    let old_text = old.as_ref().map(|o| crate::model::value::display(&base_snapshot, tsnap, &f.kind, o)).unwrap_or_default();
                    let new_text = v.as_ref().map(|o| crate::model::value::display(&base_snapshot, tsnap, &f.kind, o)).unwrap_or_default();
                    match v {
                        Some(v) if !v.is_empty() || matches!(v, Value::Bool(false)) => {
                            r.cells.insert(fid.clone(), v.clone());
                        }
                        _ => {
                            r.cells.remove(fid);
                        }
                    }
                    r.updated_at = crate::model::now_ms();
                    r.updated_by = me.clone();
                    r.history.push((r.updated_at, me.clone(), f.name.clone(), old_text, new_text));
                    if r.history.len() > 200 {
                        r.history.remove(0);
                    }
                    fired.push((rid.clone(), fid.clone()));
                }
            }
            for (fid, rid, old, new) in mirrors {
                b.mirror_link(&table, &fid, &rid, &old, &new);
            }
            crate::ui::automations::on_change(b, &table, &fired);
        });
    }
}
