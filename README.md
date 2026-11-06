# Rutable

A multi-dimensional table (多维表格) desktop app for Windows, written in Rust with
[NSH](https://nsh.dev) (Win32 + Direct2D, no WebView). Its features and interaction model follow
Feishu Bitable (飞书多维表格): tables with typed fields, grid / kanban / gallery / gantt / calendar /
form views, filters, sorts, groups, formulas, links, lookups, dashboards and automations.

## Build and run

Needs Rust (MSVC toolchain) and Visual Studio Build Tools (C++ workload, Windows SDK).

```powershell
cargo build --release          # target\release\rutable.exe
cargo test --bin rutable       # model, formula, CSV and clipboard tests
.\target\release\rutable.exe   # opens %APPDATA%\Rutable\base.json (a sample base the first time)
```

Command line: `--data <file>` picks the document, `--fresh` starts from the sample base,
`--maximized` maximizes the window, `--automation [port]` opens the automation port (default 47800).

## Features

- **Tables** (sidebar): create, rename (double click), duplicate, delete, reorder by dragging, search;
  dashboards next to them; import a CSV as a new table, export a view as CSV.
- **Field types**: 文本, 单选, 多选, 人员, 日期 (formats, time), 附件, 数字 (decimals, thousands,
  percent), 复选框, 超链接, 电话, 邮箱, 单向/双向关联, 查找引用 (with aggregation), 公式, 货币, 进度,
  评分, 地理位置, 条码, 自动编号, 创建时间, 最后更新时间, 创建人, 修改人. Changing a field's type
  converts its values.
- **Grid view** (canvas-drawn): cell / range / column selection with mouse and keyboard, type to edit,
  editors per type (option list with create, calendar, people, record picker, attachments), fill
  handle, copy / cut / paste as TSV (adds records past the end), column resize and move, row drag,
  frozen columns, row header with checkboxes and a primary-cell 查看 button, grouping (3 levels, collapsible), statistics
  per column, row heights, row coloring (填色), search with highlighted matches, undo / redo.
- **Field menu**: edit, description, insert left / right, duplicate, sort, group, filter, freeze,
  hide, delete.
- **View toolbar**: 字段配置 (show / hide / order), 筛选 (all / any, per-type operators, relative
  dates), 分组, 排序 (auto sort), 行高, 填色, CSV, search.
- **Other views**: kanban (group by single select or person, drag cards between columns, cover,
  compact mode), gallery, gantt (drag to move, day / week / month / quarter / year scales), calendar
  (start / end fields, add on a day), form (choose questions, required ones, submit adds a record).
- Click URL text to open the link in the default browser; click attachment thumbnails to reveal and
  select the file in Explorer. Click cell whitespace or press F2 to edit.
- **Record card** (展开记录, Space): every field editable, previous / next, duplicate, delete,
  comments, change history.
- **Formulas**: `[字段]`, `[表].[字段]`, chained calls (`[表].FILTER(CurrentValue.[金额] > 100).[客户]`),
  `+ - * / % & = != > >= < <= && ||` and ~90 functions (IF, IFS, SWITCH, SUM, AVERAGE, COUNTIF,
  SUMIF, ROUND, TEXT, LEFT/MID/RIGHT, FIND, SUBSTITUTE, DATE, DATEDIF, DAYS, WORKDAY, NETWORKDAYS,
  LIST, FILTER, MAP, SORT, UNIQUE, ARRAYJOIN, REGEXMATCH, ...).
- **Dashboards**: metric cards, bar, horizontal bar, line, pie and donut charts over any table, with
  grouping and count / sum / average / max / min.
- **Automations** (自动化流程): when a record is added or a field changes, set a field or send a
  notice.
- Saved automatically (and before the window closes).

Keyboard: arrows, Shift+arrows, Ctrl+arrows, Tab, Enter / F2 (edit), Esc, Shift+Enter (new record
below), Space (expand), Delete, Ctrl+C / X / V / D / A / Z / Y / F, Ctrl+/ (all shortcuts).

## Automation

`driver/` is `rutable-driver`, which operates the app the way a user does (real `SendInput`
mouse and keys, window screenshots) and reads its state from the automation port:

```powershell
.\target\release\rutable.exe --automation --fresh --data $env:TEMP\t.json
.\target\debug\rutable-driver.exe "click 400 258; chars 新任务; key enter; state table 3"
.\target\debug\rutable-driver.exe --script tests\smoke.txt     # exits 1 when a check fails
```

Commands: `click/dclick/rclick X Y`, `drag X1 Y1 X2 Y2`, `wheel/hwheel X Y N`, `key ctrl+c`,
`type TEXT` (through the input method), `chars TEXT` (posted characters), `shot FILE.png`,
`state [state|grid|table N|record TITLE|doc]`, `expect QUERY :: TEXT`, `expect-not ...`, `wait MS`.

## Layout

- `src/model/`: the document (`mod.rs`), values (`value.rs`), formulas (`formula.rs`), queries:
  computed cells, filters, sorts, groups, statistics (`query.rs`), the sample base (`sample.rs`)
- `src/ui/`: state and undo (`state.rs`), the grid (`grid/`), views (`kanban.rs`, `gallery.rs`,
  `gantt.rs`, `calendar.rs`, `form.rs`, `dashboard.rs`), popups, field editor, record card,
  sidebar, toolbar, automations, the automation port (`automation.rs`)
- `src/win.rs`: the window, keys and wheel the grid takes before NSH, the automation port
- `driver/`: the automation driver; `tests/smoke.txt`: the smoke scenario
