//! The document: a base of tables, each with fields, records and views,
//! and dashboards. Everything is plain data (serde), cloned for undo.

pub mod formula;
pub mod query;
pub mod sample;
pub mod value;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub use value::Value;

pub type Id = String;

/// A new unique id with `prefix` (`fld`, `rec`, `tbl`, ...).
pub fn new_id(prefix: &str) -> Id {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let t = chrono::Utc::now().timestamp_micros() as u64;
    format!("{prefix}{:x}{:03x}", t & 0xFFFF_FFFF_FFFF, n & 0xFFF)
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Member {
    pub id: Id,
    pub name: String,
    pub color: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Base {
    pub name: String,
    pub tables: Vec<Table>,
    #[serde(default)]
    pub dashboards: Vec<Dashboard>,
    /// The sidebar's order: table and dashboard ids.
    #[serde(default)]
    pub order: Vec<Id>,
    #[serde(default)]
    pub members: Vec<Member>,
    /// Who "I" am (the current user) among the members.
    #[serde(default)]
    pub me: Id,
    #[serde(default)]
    pub workflows: Vec<Workflow>,
    /// 高级权限: roles decide who may edit, read or see each table.
    #[serde(default)]
    pub advanced_perm: bool,
    #[serde(default)]
    pub roles: Vec<Role>,
    /// Member -> role (members without one edit everything).
    #[serde(default)]
    pub member_roles: HashMap<Id, Id>,
}

/// What a role may do with a table.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Perm {
    /// 可编辑
    Edit,
    /// 可阅读
    Read,
    /// 不可见
    Hidden,
}

impl Perm {
    pub const ALL: [Perm; 3] = [Perm::Edit, Perm::Read, Perm::Hidden];
    pub fn label(self) -> &'static str {
        match self {
            Perm::Edit => "可编辑",
            Perm::Read => "可阅读",
            Perm::Hidden => "不可见",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Role {
    pub id: Id,
    pub name: String,
    /// Per table; tables not listed get `default`.
    pub tables: HashMap<Id, Perm>,
    pub default: Perm,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SelectOption {
    pub id: Id,
    pub name: String,
    pub color: u8,
}

/// How a date shows.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum DateFormat {
    /// 2024/01/30
    YmdSlash,
    /// 2024-01-30
    YmdDash,
    /// 01-30
    Md,
    /// 01/30/2024
    MdyUs,
    /// 30/01/2024
    Dmy,
}

impl DateFormat {
    pub const ALL: [DateFormat; 5] = [
        DateFormat::YmdSlash,
        DateFormat::YmdDash,
        DateFormat::Md,
        DateFormat::MdyUs,
        DateFormat::Dmy,
    ];
    pub fn pattern(self) -> &'static str {
        match self {
            DateFormat::YmdSlash => "%Y/%m/%d",
            DateFormat::YmdDash => "%Y-%m-%d",
            DateFormat::Md => "%m-%d",
            DateFormat::MdyUs => "%m/%d/%Y",
            DateFormat::Dmy => "%d/%m/%Y",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            DateFormat::YmdSlash => "2024/01/30",
            DateFormat::YmdDash => "2024-01-30",
            DateFormat::Md => "01-30",
            DateFormat::MdyUs => "01/30/2024",
            DateFormat::Dmy => "30/01/2024",
        }
    }
}

/// Aggregation of a lookup (查找引用) or a link summary.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Aggregate {
    /// 原始值
    Original,
    /// 去重
    Unique,
    /// 求和
    Sum,
    /// 平均值
    Average,
    /// 最大值
    Max,
    /// 最小值
    Min,
    /// 计数
    Count,
    /// 去重计数
    CountUnique,
}

impl Aggregate {
    pub const ALL: [Aggregate; 8] = [
        Aggregate::Original,
        Aggregate::Unique,
        Aggregate::Sum,
        Aggregate::Average,
        Aggregate::Max,
        Aggregate::Min,
        Aggregate::Count,
        Aggregate::CountUnique,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Aggregate::Original => "原始值",
            Aggregate::Unique => "去重",
            Aggregate::Sum => "求和",
            Aggregate::Average => "平均值",
            Aggregate::Max => "最大值",
            Aggregate::Min => "最小值",
            Aggregate::Count => "计数",
            Aggregate::CountUnique => "去重计数",
        }
    }
}

/// A field's type and its settings.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum FieldKind {
    /// 文本 (多行)
    Text,
    /// 数字
    Number {
        precision: u8,
        #[serde(default)]
        thousands: bool,
        #[serde(default)]
        percent: bool,
    },
    /// 货币
    Currency { symbol: String, precision: u8 },
    /// 进度
    Progress { color: u8 },
    /// 评分
    Rating { max: u8, icon: u8 },
    /// 单选
    SingleSelect { options: Vec<SelectOption> },
    /// 多选
    MultiSelect { options: Vec<SelectOption> },
    /// 日期
    Date { format: DateFormat, time: bool },
    /// 复选框
    Checkbox,
    /// 人员
    User { multiple: bool },
    /// 电话号码
    Phone,
    /// 超链接
    Url,
    /// 邮箱
    Email,
    /// 附件
    Attachment,
    /// 单向关联 / 双向关联
    Link {
        table: Id,
        #[serde(default)]
        multiple: bool,
        /// The field of the other table that links back (双向关联).
        #[serde(default)]
        pair: Option<Id>,
    },
    /// 查找引用
    Lookup {
        link: Id,
        target: Id,
        aggregate: Aggregate,
    },
    /// 公式
    Formula { expr: String },
    /// 地理位置
    Location,
    /// 自动编号
    AutoNumber,
    /// 创建时间
    CreatedTime { format: DateFormat, time: bool },
    /// 最后更新时间
    ModifiedTime { format: DateFormat, time: bool },
    /// 创建人
    CreatedBy,
    /// 修改人
    ModifiedBy,
    /// 条码
    Barcode,
    /// 按钮: a press runs its action on the record.
    Button { label: String, color: u8, action: Action },
}

/// The type tags, in the order of the type menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KindTag {
    Text,
    SingleSelect,
    MultiSelect,
    User,
    Date,
    Attachment,
    Number,
    Checkbox,
    Link,
    DuplexLink,
    Lookup,
    Formula,
    Url,
    Phone,
    Email,
    Location,
    AutoNumber,
    Currency,
    Progress,
    Rating,
    Barcode,
    CreatedTime,
    ModifiedTime,
    CreatedBy,
    ModifiedBy,
    Button,
}

impl KindTag {
    /// Groups of the type menu: 基础, 高级, 系统.
    pub const MENU: [&'static [KindTag]; 3] = [
        &[
            KindTag::Text,
            KindTag::SingleSelect,
            KindTag::MultiSelect,
            KindTag::User,
            KindTag::Date,
            KindTag::Attachment,
            KindTag::Number,
            KindTag::Checkbox,
            KindTag::Url,
            KindTag::Phone,
            KindTag::Email,
        ],
        &[
            KindTag::Link,
            KindTag::DuplexLink,
            KindTag::Lookup,
            KindTag::Formula,
            KindTag::Currency,
            KindTag::Progress,
            KindTag::Rating,
            KindTag::Location,
            KindTag::Barcode,
            KindTag::AutoNumber,
            KindTag::Button,
        ],
        &[
            KindTag::CreatedTime,
            KindTag::ModifiedTime,
            KindTag::CreatedBy,
            KindTag::ModifiedBy,
        ],
    ];

    pub fn label(self) -> &'static str {
        match self {
            KindTag::Text => "文本",
            KindTag::SingleSelect => "单选",
            KindTag::MultiSelect => "多选",
            KindTag::User => "人员",
            KindTag::Date => "日期",
            KindTag::Attachment => "附件",
            KindTag::Number => "数字",
            KindTag::Checkbox => "复选框",
            KindTag::Link => "单向关联",
            KindTag::DuplexLink => "双向关联",
            KindTag::Lookup => "查找引用",
            KindTag::Formula => "公式",
            KindTag::Url => "超链接",
            KindTag::Phone => "电话号码",
            KindTag::Email => "邮箱",
            KindTag::Location => "地理位置",
            KindTag::AutoNumber => "自动编号",
            KindTag::Currency => "货币",
            KindTag::Progress => "进度",
            KindTag::Rating => "评分",
            KindTag::Barcode => "条码",
            KindTag::CreatedTime => "创建时间",
            KindTag::ModifiedTime => "最后更新时间",
            KindTag::CreatedBy => "创建人",
            KindTag::ModifiedBy => "修改人",
            KindTag::Button => "按钮",
        }
    }

    /// The settings a new field of this type starts with.
    pub fn default_kind(self, tables: &[Table], this_table: &str) -> FieldKind {
        let other = tables
            .iter()
            .find(|t| t.id != this_table)
            .or_else(|| tables.first())
            .map(|t| t.id.clone())
            .unwrap_or_default();
        match self {
            KindTag::Text => FieldKind::Text,
            KindTag::SingleSelect => FieldKind::SingleSelect { options: vec![] },
            KindTag::MultiSelect => FieldKind::MultiSelect { options: vec![] },
            KindTag::User => FieldKind::User { multiple: true },
            KindTag::Date => FieldKind::Date { format: DateFormat::YmdSlash, time: false },
            KindTag::Attachment => FieldKind::Attachment,
            KindTag::Number => FieldKind::Number { precision: 0, thousands: false, percent: false },
            KindTag::Checkbox => FieldKind::Checkbox,
            KindTag::Link => FieldKind::Link { table: other, multiple: true, pair: None },
            KindTag::DuplexLink => FieldKind::Link { table: other, multiple: true, pair: Some(String::new()) },
            KindTag::Lookup => FieldKind::Lookup {
                link: String::new(),
                target: String::new(),
                aggregate: Aggregate::Original,
            },
            KindTag::Formula => FieldKind::Formula { expr: String::new() },
            KindTag::Url => FieldKind::Url,
            KindTag::Phone => FieldKind::Phone,
            KindTag::Email => FieldKind::Email,
            KindTag::Location => FieldKind::Location,
            KindTag::AutoNumber => FieldKind::AutoNumber,
            KindTag::Currency => FieldKind::Currency { symbol: "¥".into(), precision: 2 },
            KindTag::Progress => FieldKind::Progress { color: 0 },
            KindTag::Rating => FieldKind::Rating { max: 5, icon: 0 },
            KindTag::Barcode => FieldKind::Barcode,
            KindTag::Button => FieldKind::Button {
                label: "点击".into(),
                color: 0,
                action: Action::Notify { text: "按钮已点击".into() },
            },
            KindTag::CreatedTime => FieldKind::CreatedTime { format: DateFormat::YmdSlash, time: true },
            KindTag::ModifiedTime => FieldKind::ModifiedTime { format: DateFormat::YmdSlash, time: true },
            KindTag::CreatedBy => FieldKind::CreatedBy,
            KindTag::ModifiedBy => FieldKind::ModifiedBy,
        }
    }
}

impl FieldKind {
    pub fn tag(&self) -> KindTag {
        match self {
            FieldKind::Text => KindTag::Text,
            FieldKind::Number { .. } => KindTag::Number,
            FieldKind::Currency { .. } => KindTag::Currency,
            FieldKind::Progress { .. } => KindTag::Progress,
            FieldKind::Rating { .. } => KindTag::Rating,
            FieldKind::SingleSelect { .. } => KindTag::SingleSelect,
            FieldKind::MultiSelect { .. } => KindTag::MultiSelect,
            FieldKind::Date { .. } => KindTag::Date,
            FieldKind::Checkbox => KindTag::Checkbox,
            FieldKind::User { .. } => KindTag::User,
            FieldKind::Phone => KindTag::Phone,
            FieldKind::Url => KindTag::Url,
            FieldKind::Email => KindTag::Email,
            FieldKind::Attachment => KindTag::Attachment,
            FieldKind::Link { pair, .. } => {
                if pair.is_some() {
                    KindTag::DuplexLink
                } else {
                    KindTag::Link
                }
            }
            FieldKind::Lookup { .. } => KindTag::Lookup,
            FieldKind::Formula { .. } => KindTag::Formula,
            FieldKind::Location => KindTag::Location,
            FieldKind::AutoNumber => KindTag::AutoNumber,
            FieldKind::CreatedTime { .. } => KindTag::CreatedTime,
            FieldKind::ModifiedTime { .. } => KindTag::ModifiedTime,
            FieldKind::CreatedBy => KindTag::CreatedBy,
            FieldKind::ModifiedBy => KindTag::ModifiedBy,
            FieldKind::Barcode => KindTag::Barcode,
            FieldKind::Button { .. } => KindTag::Button,
        }
    }

    /// Computed by the app: the user cannot type into its cells.
    pub fn is_computed(&self) -> bool {
        matches!(
            self,
            FieldKind::Lookup { .. }
                | FieldKind::Formula { .. }
                | FieldKind::AutoNumber
                | FieldKind::CreatedTime { .. }
                | FieldKind::ModifiedTime { .. }
                | FieldKind::CreatedBy
                | FieldKind::ModifiedBy
                | FieldKind::Button { .. }
        )
    }

    pub fn options(&self) -> Option<&Vec<SelectOption>> {
        match self {
            FieldKind::SingleSelect { options } | FieldKind::MultiSelect { options } => Some(options),
            _ => None,
        }
    }

    pub fn options_mut(&mut self) -> Option<&mut Vec<SelectOption>> {
        match self {
            FieldKind::SingleSelect { options } | FieldKind::MultiSelect { options } => Some(options),
            _ => None,
        }
    }

    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            FieldKind::Number { .. }
                | FieldKind::Currency { .. }
                | FieldKind::Progress { .. }
                | FieldKind::Rating { .. }
                | FieldKind::AutoNumber
        )
    }

    pub fn is_date(&self) -> bool {
        matches!(
            self,
            FieldKind::Date { .. } | FieldKind::CreatedTime { .. } | FieldKind::ModifiedTime { .. }
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Field {
    pub id: Id,
    pub name: String,
    pub kind: FieldKind,
    #[serde(default)]
    pub description: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Attachment {
    pub name: String,
    /// The copy in the app's data folder.
    pub path: String,
    pub size: u64,
}

impl Attachment {
    pub fn is_image(&self) -> bool {
        let n = self.name.to_lowercase();
        [".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp"].iter().any(|e| n.ends_with(e))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Comment {
    pub id: Id,
    pub author: Id,
    pub text: String,
    pub at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Record {
    pub id: Id,
    #[serde(default)]
    pub cells: HashMap<Id, Value>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub created_by: Id,
    #[serde(default)]
    pub updated_by: Id,
    /// 自动编号
    #[serde(default)]
    pub seq: u64,
    #[serde(default)]
    pub comments: Vec<Comment>,
    /// Changes to the record (修改记录): (time, who, field name, old, new).
    #[serde(default)]
    pub history: Vec<(i64, Id, String, String, String)>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ViewKind {
    /// 表格视图
    Grid,
    /// 看板视图
    Kanban,
    /// 画册视图
    Gallery,
    /// 甘特视图
    Gantt,
    /// 日历视图
    Calendar,
    /// 表单视图
    Form,
}

impl ViewKind {
    pub const ALL: [ViewKind; 6] = [
        ViewKind::Grid,
        ViewKind::Kanban,
        ViewKind::Gallery,
        ViewKind::Gantt,
        ViewKind::Calendar,
        ViewKind::Form,
    ];
    pub fn label(self) -> &'static str {
        match self {
            ViewKind::Grid => "表格视图",
            ViewKind::Kanban => "看板视图",
            ViewKind::Gallery => "画册视图",
            ViewKind::Gantt => "甘特视图",
            ViewKind::Calendar => "日历视图",
            ViewKind::Form => "表单视图",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            ViewKind::Grid => "表格",
            ViewKind::Kanban => "看板",
            ViewKind::Gallery => "画册",
            ViewKind::Gantt => "甘特",
            ViewKind::Calendar => "日历",
            ViewKind::Form => "表单",
        }
    }
}

/// A filter's operator.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Op {
    /// 等于
    Is,
    /// 不等于
    IsNot,
    /// 包含
    Contains,
    /// 不包含
    NotContains,
    /// 为空
    Empty,
    /// 不为空
    NotEmpty,
    /// 大于
    Gt,
    /// 大于等于
    Gte,
    /// 小于
    Lt,
    /// 小于等于
    Lte,
    /// 早于 (dates)
    Before,
    /// 晚于 (dates)
    After,
}

impl Op {
    pub fn label(self, date: bool) -> &'static str {
        match self {
            Op::Is => "等于",
            Op::IsNot => "不等于",
            Op::Contains => "包含",
            Op::NotContains => "不包含",
            Op::Empty => "为空",
            Op::NotEmpty => "不为空",
            Op::Gt => {
                if date {
                    "晚于"
                } else {
                    "大于"
                }
            }
            Op::Gte => "大于等于",
            Op::Lt => {
                if date {
                    "早于"
                } else {
                    "小于"
                }
            }
            Op::Lte => "小于等于",
            Op::Before => "早于",
            Op::After => "晚于",
        }
    }

    /// The operators a field offers.
    pub fn for_kind(kind: &FieldKind) -> Vec<Op> {
        use Op::*;
        match kind {
            FieldKind::Checkbox => vec![Is],
            k if k.is_numeric() => vec![Is, IsNot, Gt, Gte, Lt, Lte, Empty, NotEmpty],
            k if k.is_date() => vec![Is, Before, After, Empty, NotEmpty],
            FieldKind::SingleSelect { .. } => vec![Is, IsNot, Contains, NotContains, Empty, NotEmpty],
            FieldKind::MultiSelect { .. } | FieldKind::User { .. } | FieldKind::Link { .. } => {
                vec![Is, IsNot, Contains, NotContains, Empty, NotEmpty]
            }
            FieldKind::Attachment => vec![Empty, NotEmpty],
            _ => vec![Is, IsNot, Contains, NotContains, Empty, NotEmpty],
        }
    }

    pub fn needs_value(self) -> bool {
        !matches!(self, Op::Empty | Op::NotEmpty)
    }
}

/// A date a filter compares with.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum DateRef {
    Today,
    Tomorrow,
    Yesterday,
    ThisWeek,
    LastWeek,
    ThisMonth,
    LastMonth,
    Past7,
    Next7,
    Past30,
    Next30,
    Exact(i64),
}

impl DateRef {
    pub const RELATIVE: [DateRef; 11] = [
        DateRef::Today,
        DateRef::Tomorrow,
        DateRef::Yesterday,
        DateRef::ThisWeek,
        DateRef::LastWeek,
        DateRef::ThisMonth,
        DateRef::LastMonth,
        DateRef::Past7,
        DateRef::Next7,
        DateRef::Past30,
        DateRef::Next30,
    ];
    pub fn label(&self) -> String {
        match self {
            DateRef::Today => "今天".into(),
            DateRef::Tomorrow => "明天".into(),
            DateRef::Yesterday => "昨天".into(),
            DateRef::ThisWeek => "本周".into(),
            DateRef::LastWeek => "上周".into(),
            DateRef::ThisMonth => "本月".into(),
            DateRef::LastMonth => "上月".into(),
            DateRef::Past7 => "过去 7 天内".into(),
            DateRef::Next7 => "未来 7 天内".into(),
            DateRef::Past30 => "过去 30 天内".into(),
            DateRef::Next30 => "未来 30 天内".into(),
            DateRef::Exact(ms) => value::format_date(*ms, DateFormat::YmdSlash, false),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum FilterValue {
    None,
    Text(String),
    Number(f64),
    Bool(bool),
    /// Option, user or record ids.
    Ids(Vec<Id>),
    Date(DateRef),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Condition {
    pub id: Id,
    pub field: Id,
    pub op: Op,
    pub value: FilterValue,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct Filter {
    /// 所有条件 (and) or 任一条件 (or).
    pub any: bool,
    pub conditions: Vec<Condition>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SortRule {
    pub field: Id,
    pub desc: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RowHeight {
    /// 低
    Short,
    /// 中等
    Medium,
    /// 高
    Tall,
    /// 超高
    ExtraTall,
}

impl RowHeight {
    pub const ALL: [RowHeight; 4] = [RowHeight::Short, RowHeight::Medium, RowHeight::Tall, RowHeight::ExtraTall];
    pub fn px(self) -> f32 {
        match self {
            RowHeight::Short => 33.0,
            RowHeight::Medium => 57.0,
            RowHeight::Tall => 89.0,
            RowHeight::ExtraTall => 129.0,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            RowHeight::Short => "低",
            RowHeight::Medium => "中等",
            RowHeight::Tall => "高",
            RowHeight::ExtraTall => "超高",
        }
    }
}

/// A column's summary at the bottom of the grid (统计).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Stat {
    None,
    /// 记录总数 (the first column always shows the count)
    Count,
    /// 已填写
    Filled,
    /// 未填写
    Empty,
    /// 已填写占比
    FilledPct,
    /// 未填写占比
    EmptyPct,
    /// 唯一值
    Unique,
    Sum,
    Average,
    Max,
    Min,
    /// 极差
    Range,
    /// 已勾选
    Checked,
    /// 未勾选
    Unchecked,
    /// 已勾选占比
    CheckedPct,
    /// 最早时间
    Earliest,
    /// 最晚时间
    Latest,
}

impl Stat {
    pub fn label(self) -> &'static str {
        match self {
            Stat::None => "不展示",
            Stat::Count => "记录总数",
            Stat::Filled => "已填写",
            Stat::Empty => "未填写",
            Stat::FilledPct => "已填写占比",
            Stat::EmptyPct => "未填写占比",
            Stat::Unique => "唯一值",
            Stat::Sum => "求和",
            Stat::Average => "平均值",
            Stat::Max => "最大值",
            Stat::Min => "最小值",
            Stat::Range => "极差",
            Stat::Checked => "已勾选",
            Stat::Unchecked => "未勾选",
            Stat::CheckedPct => "已勾选占比",
            Stat::Earliest => "最早时间",
            Stat::Latest => "最晚时间",
        }
    }

    pub fn for_kind(kind: &FieldKind) -> Vec<Stat> {
        use Stat::*;
        let mut v = vec![None, Count, Filled, Empty, FilledPct, EmptyPct, Unique];
        if kind.is_numeric() || matches!(kind, FieldKind::Formula { .. }) {
            v.extend([Sum, Average, Max, Min, Range]);
        }
        if matches!(kind, FieldKind::Checkbox) {
            v = vec![None, Count, Checked, Unchecked, CheckedPct];
        }
        if kind.is_date() {
            v.extend([Earliest, Latest]);
        }
        v
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FormConfig {
    pub title: String,
    pub description: String,
    /// The fields asked, in order, and whether each is required.
    pub fields: Vec<(Id, bool)>,
    #[serde(default)]
    pub submit_text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct View {
    pub id: Id,
    pub name: String,
    pub kind: ViewKind,
    #[serde(default)]
    pub filter: Filter,
    #[serde(default)]
    pub sorts: Vec<SortRule>,
    /// 自动排序: the sort applies as records change (else on request).
    #[serde(default = "yes")]
    pub auto_sort: bool,
    #[serde(default)]
    pub groups: Vec<SortRule>,
    /// Group values collapsed (by "field\u{1}value" keys).
    #[serde(default)]
    pub collapsed: Vec<String>,
    /// The fields shown, in order (fields not listed show at the end).
    #[serde(default)]
    pub field_order: Vec<Id>,
    #[serde(default)]
    pub hidden: Vec<Id>,
    #[serde(default)]
    pub widths: HashMap<Id, f32>,
    /// Columns frozen at the left.
    #[serde(default = "one")]
    pub frozen: usize,
    #[serde(default = "short")]
    pub row_height: RowHeight,
    #[serde(default)]
    pub stats: HashMap<Id, Stat>,
    /// Manual record order of this view.
    #[serde(default)]
    pub row_order: Vec<Id>,
    /// Kanban: the single select (or user) field that makes the columns.
    #[serde(default)]
    pub group_field: Option<Id>,
    /// Gallery / kanban: the attachment field shown as the cover.
    #[serde(default)]
    pub cover_field: Option<Id>,
    /// Calendar / gantt: the start and end dates.
    #[serde(default)]
    pub start_field: Option<Id>,
    #[serde(default)]
    pub end_field: Option<Id>,
    /// Form views.
    #[serde(default)]
    pub form: Option<FormConfig>,
    /// Field colors of rows (填色): (condition, color).
    #[serde(default)]
    pub row_colors: Vec<(Condition, u8)>,
    /// Kanban columns (option ids) hidden.
    #[serde(default)]
    pub hidden_groups: Vec<Id>,
    /// Wraps long text in cells (自动换行).
    #[serde(default)]
    pub wrap: bool,
    /// Kanban / gallery: cards without field names (紧凑模式).
    #[serde(default)]
    pub compact: bool,
}

fn yes() -> bool {
    true
}
fn one() -> usize {
    1
}
fn short() -> RowHeight {
    RowHeight::Short
}

impl View {
    pub fn new(name: &str, kind: ViewKind) -> View {
        View {
            id: new_id("vew"),
            name: name.into(),
            kind,
            filter: Filter::default(),
            sorts: vec![],
            auto_sort: true,
            groups: vec![],
            collapsed: vec![],
            field_order: vec![],
            hidden: vec![],
            widths: HashMap::new(),
            frozen: 1,
            row_height: RowHeight::Short,
            stats: HashMap::new(),
            row_order: vec![],
            group_field: None,
            cover_field: None,
            start_field: None,
            end_field: None,
            form: None,
            row_colors: vec![],
            hidden_groups: vec![],
            wrap: false,
            compact: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Table {
    pub id: Id,
    pub name: String,
    pub fields: Vec<Field>,
    pub records: Vec<Record>,
    pub views: Vec<View>,
    #[serde(default)]
    pub seq: u64,
    /// Select fields whose options come from another table's field
    /// (引用选项): field -> (table, field).
    #[serde(default)]
    pub option_sources: HashMap<Id, (Id, Id)>,
}

impl Table {
    pub fn field(&self, id: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.id == id)
    }
    pub fn field_mut(&mut self, id: &str) -> Option<&mut Field> {
        self.fields.iter_mut().find(|f| f.id == id)
    }
    pub fn field_by_name(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.name == name)
    }
    pub fn record(&self, id: &str) -> Option<&Record> {
        self.records.iter().find(|r| r.id == id)
    }
    pub fn record_mut(&mut self, id: &str) -> Option<&mut Record> {
        self.records.iter_mut().find(|r| r.id == id)
    }
    pub fn view(&self, id: &str) -> Option<&View> {
        self.views.iter().find(|v| v.id == id)
    }
    pub fn view_mut(&mut self, id: &str) -> Option<&mut View> {
        self.views.iter_mut().find(|v| v.id == id)
    }
    /// The primary field (索引列): the first field.
    pub fn primary(&self) -> Option<&Field> {
        self.fields.first()
    }

    /// A field name not used yet: `base`, `base 2`, ...
    pub fn unique_field_name(&self, base: &str) -> String {
        if self.field_by_name(base).is_none() {
            return base.into();
        }
        (2..).map(|i| format!("{base} {i}")).find(|n| self.field_by_name(n).is_none()).unwrap()
    }

    /// The fields of view `v`, in its order: (field, hidden).
    pub fn view_fields<'a>(&'a self, v: &View) -> Vec<(&'a Field, bool)> {
        let mut out: Vec<(&Field, bool)> = Vec::new();
        // The primary field always leads.
        if let Some(p) = self.primary() {
            out.push((p, false));
        }
        for id in &v.field_order {
            if let Some(f) = self.field(id) {
                if !out.iter().any(|(o, _)| o.id == f.id) {
                    out.push((f, v.hidden.contains(&f.id)));
                }
            }
        }
        for f in &self.fields {
            if !out.iter().any(|(o, _)| o.id == f.id) {
                out.push((f, v.hidden.contains(&f.id)));
            }
        }
        out
    }

    pub fn new_record(&mut self, me: &str) -> Record {
        self.seq += 1;
        let now = now_ms();
        Record {
            id: new_id("rec"),
            cells: HashMap::new(),
            created_at: now,
            updated_at: now,
            created_by: me.into(),
            updated_by: me.into(),
            seq: self.seq,
            comments: vec![],
            history: vec![],
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChartKind {
    /// 指标卡
    Metric,
    /// 柱状图
    Bar,
    /// 条形图
    HBar,
    /// 折线图
    Line,
    /// 饼图
    Pie,
    /// 环形图
    Donut,
}

impl ChartKind {
    pub const ALL: [ChartKind; 6] = [
        ChartKind::Metric,
        ChartKind::Bar,
        ChartKind::HBar,
        ChartKind::Line,
        ChartKind::Pie,
        ChartKind::Donut,
    ];
    pub fn label(self) -> &'static str {
        match self {
            ChartKind::Metric => "指标卡",
            ChartKind::Bar => "柱状图",
            ChartKind::HBar => "条形图",
            ChartKind::Line => "折线图",
            ChartKind::Pie => "饼图",
            ChartKind::Donut => "环形图",
        }
    }
}

/// How a chart's numbers are made from records.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Measure {
    /// 记录总数
    Count,
    Sum,
    Average,
    Max,
    Min,
}

impl Measure {
    pub const ALL: [Measure; 5] = [Measure::Count, Measure::Sum, Measure::Average, Measure::Max, Measure::Min];
    pub fn label(self) -> &'static str {
        match self {
            Measure::Count => "记录总数",
            Measure::Sum => "求和",
            Measure::Average => "平均值",
            Measure::Max => "最大值",
            Measure::Min => "最小值",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Chart {
    pub id: Id,
    pub title: String,
    pub kind: ChartKind,
    pub table: Id,
    /// The field the records are grouped by (横轴 / 分类), not for metrics.
    pub group_by: Option<Id>,
    pub measure: Measure,
    /// The numeric field summed / averaged.
    pub value_field: Option<Id>,
    #[serde(default)]
    pub filter: Filter,
    /// Grid placement: column span (1..=4) and height in rows.
    pub span: u8,
    pub rows: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Dashboard {
    pub id: Id,
    pub name: String,
    pub charts: Vec<Chart>,
}

/// An automation (自动化流程): when a trigger fires, do an action.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Workflow {
    pub id: Id,
    pub name: String,
    pub enabled: bool,
    pub table: Id,
    pub trigger: Trigger,
    pub action: Action,
    #[serde(default)]
    pub runs: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Trigger {
    /// 新增记录时
    RecordAdded,
    /// 记录满足条件时 (a field changes to match)
    FieldChanged { field: Id },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Action {
    /// 修改记录: set a field to a value
    SetField { field: Id, value: Value },
    /// 发送通知 (shown in the app's notification list)
    Notify { text: String },
    /// 打开链接 (`{字段}` is replaced by the record's value)
    OpenUrl { url: String },
}

impl Base {
    /// The owner: the first member, who always edits everything.
    pub fn owner(&self) -> Option<&Id> {
        self.members.first().map(|m| &m.id)
    }

    /// What `member` may do with `table`.
    pub fn perm(&self, member: &str, table: &str) -> Perm {
        if !self.advanced_perm || self.owner().is_some_and(|o| o == member) {
            return Perm::Edit;
        }
        let Some(role) = self.member_roles.get(member).and_then(|r| self.roles.iter().find(|x| &x.id == r)) else {
            return Perm::Edit;
        };
        role.tables.get(table).copied().unwrap_or(role.default)
    }

    pub fn table(&self, id: &str) -> Option<&Table> {
        self.tables.iter().find(|t| t.id == id)
    }
    pub fn table_mut(&mut self, id: &str) -> Option<&mut Table> {
        self.tables.iter_mut().find(|t| t.id == id)
    }
    pub fn dashboard(&self, id: &str) -> Option<&Dashboard> {
        self.dashboards.iter().find(|t| t.id == id)
    }
    pub fn dashboard_mut(&mut self, id: &str) -> Option<&mut Dashboard> {
        self.dashboards.iter_mut().find(|t| t.id == id)
    }
    pub fn member(&self, id: &str) -> Option<&Member> {
        self.members.iter().find(|m| m.id == id)
    }
    pub fn member_name(&self, id: &str) -> String {
        self.member(id).map(|m| m.name.clone()).unwrap_or_default()
    }

    /// The sidebar items in order: ids of tables and dashboards.
    pub fn sidebar(&self) -> Vec<Id> {
        let mut out: Vec<Id> = self
            .order
            .iter()
            .filter(|id| self.table(id).is_some() || self.dashboard(id).is_some())
            .cloned()
            .collect();
        for t in &self.tables {
            if !out.contains(&t.id) {
                out.push(t.id.clone());
            }
        }
        for d in &self.dashboards {
            if !out.contains(&d.id) {
                out.push(d.id.clone());
            }
        }
        out
    }

    pub fn unique_table_name(&self, base: &str) -> String {
        let used = |n: &str| self.tables.iter().any(|t| t.name == n) || self.dashboards.iter().any(|d| d.name == n);
        if !used(base) {
            return base.into();
        }
        (2..).map(|i| format!("{base}{i}")).find(|n| !used(n)).unwrap()
    }

    /// A new table with the default fields and a grid view, as 新建数据表 makes it.
    pub fn new_table(&mut self, name: &str) -> Id {
        let mut t = Table {
            id: new_id("tbl"),
            name: self.unique_table_name(name),
            fields: vec![
                Field { id: new_id("fld"), name: "文本".into(), kind: FieldKind::Text, description: String::new() },
                Field {
                    id: new_id("fld"),
                    name: "单选".into(),
                    kind: FieldKind::SingleSelect { options: vec![] },
                    description: String::new(),
                },
                Field {
                    id: new_id("fld"),
                    name: "日期".into(),
                    kind: FieldKind::Date { format: DateFormat::YmdSlash, time: false },
                    description: String::new(),
                },
                Field {
                    id: new_id("fld"),
                    name: "附件".into(),
                    kind: FieldKind::Attachment,
                    description: String::new(),
                },
            ],
            records: vec![],
            views: vec![View::new("表格", ViewKind::Grid)],
            seq: 0,
            option_sources: HashMap::new(),
        };
        let me = self.me.clone();
        for _ in 0..5 {
            let r = t.new_record(&me);
            t.records.push(r);
        }
        let id = t.id.clone();
        self.tables.push(t);
        self.order.push(id.clone());
        id
    }

    /// Adds to select fields with a source (引用选项) the values of their
    /// source field that they do not have yet.
    pub fn sync_option_sources(&mut self) {
        let mut adds: Vec<(Id, Id, Vec<String>)> = vec![];
        for t in &self.tables {
            for (fid, (st, sf)) in &t.option_sources {
                let Some(src_t) = self.table(st) else { continue };
                let Some(src_f) = src_t.field(sf) else { continue };
                let mut names: Vec<String> = vec![];
                for r in &src_t.records {
                    if let Some(v) = query::cell(self, src_t, r, src_f) {
                        for n in value::display(self, src_t, &src_f.kind, &v).split(',') {
                            let n = n.trim().to_string();
                            if !n.is_empty() && !names.contains(&n) {
                                names.push(n);
                            }
                        }
                    }
                }
                adds.push((t.id.clone(), fid.clone(), names));
            }
        }
        for (tid, fid, names) in adds {
            if let Some(opts) = self.table_mut(&tid).and_then(|t| t.field_mut(&fid)).and_then(|f| f.kind.options_mut()) {
                for n in names {
                    if !opts.iter().any(|o| o.name == n) {
                        let c = (opts.len() % value::TAG_COLORS.len()) as u8;
                        opts.push(SelectOption { id: new_id("opt"), name: n, color: c });
                    }
                }
            }
        }
    }

    /// Removes the links to records of `table` that no longer exist and
    /// keeps the pairs of duplex links in step.
    pub fn sync_links(&mut self) {
        // Record ids of every table.
        let ids: HashMap<Id, std::collections::HashSet<Id>> = self
            .tables
            .iter()
            .map(|t| (t.id.clone(), t.records.iter().map(|r| r.id.clone()).collect()))
            .collect();
        for t in &mut self.tables {
            let links: Vec<(Id, Id)> = t
                .fields
                .iter()
                .filter_map(|f| match &f.kind {
                    FieldKind::Link { table, .. } => Some((f.id.clone(), table.clone())),
                    _ => None,
                })
                .collect();
            for r in &mut t.records {
                for (fid, other) in &links {
                    if let Some(Value::Links(l)) = r.cells.get_mut(fid) {
                        if let Some(set) = ids.get(other) {
                            l.retain(|x| set.contains(x));
                        }
                    }
                }
            }
        }
    }

    /// Sets the other side of a duplex link after `record`'s cell of
    /// `field` (in `table`) changed from `old` to `new`.
    pub fn mirror_link(&mut self, table: &str, field: &str, record: &str, old: &[Id], new: &[Id]) {
        let Some((other, pair)) = self.table(table).and_then(|t| t.field(field)).and_then(|f| match &f.kind {
            FieldKind::Link { table, pair: Some(p), .. } if !p.is_empty() => Some((table.clone(), p.clone())),
            _ => None,
        }) else {
            return;
        };
        let Some(ot) = self.table_mut(&other) else { return };
        for r in &mut ot.records {
            let was = old.contains(&r.id);
            let is = new.contains(&r.id);
            if was == is {
                continue;
            }
            let cell = r.cells.entry(pair.clone()).or_insert_with(|| Value::Links(vec![]));
            if let Value::Links(l) = cell {
                if is && !l.iter().any(|x| x == record) {
                    l.push(record.to_string());
                }
                if !is {
                    l.retain(|x| x != record);
                }
            }
        }
    }
}
