//! Formulas (公式): `[字段]`, `[数据表].[字段]`, chained calls
//! (`[表].FILTER(CurrentValue.[金额] > 100).[客户]`), the operators
//! `+ - * / % & = != > >= < <= && ||`, and the functions of Base formulas.

use std::fmt;

use chrono::{Datelike, Duration, Timelike};

use super::{
    Base, FieldKind, Id, Record, Table,
    value::{self, Value},
};

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Num(f64),
    Str(String),
    Bool(bool),
    /// `[name]`: a field of the row, or a table.
    Ref(String),
    CurrentValue,
    Call(String, Vec<Expr>),
    /// `x.[name]`
    Member(Box<Expr>, String),
    /// `x.FUNC(args)`: FUNC(x, args)
    Method(Box<Expr>, String, Vec<Expr>),
    Bin(Bin, Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Bin {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Cat,
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    And,
    Or,
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Ref(String),
    Ident(String),
    Op(&'static str),
    LParen,
    RParen,
    Comma,
    Dot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn lex(src: &str) -> Result<Vec<Tok>, ParseError> {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut out = vec![];
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' | '\n' | '\r' => i += 1,
            '0'..='9' => {
                let s = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    // `1.FUNC()` is not a number's dot.
                    if chars[i] == '.' && !chars.get(i + 1).is_some_and(|c| c.is_ascii_digit()) {
                        break;
                    }
                    i += 1;
                }
                let t: String = chars[s..i].iter().collect();
                out.push(Tok::Num(t.parse().map_err(|_| ParseError(format!("无效数字 {t}")))?));
            }
            '"' | '“' | '”' | '\'' => {
                let close = match c {
                    '“' => '”',
                    x => x,
                };
                i += 1;
                let mut s = String::new();
                while i < chars.len() && chars[i] != close && !(close == '”' && chars[i] == '"') {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        i += 1;
                    }
                    s.push(chars[i]);
                    i += 1;
                }
                if i >= chars.len() {
                    return Err(ParseError("字符串缺少结束引号".into()));
                }
                i += 1;
                out.push(Tok::Str(s));
            }
            '[' => {
                i += 1;
                let mut s = String::new();
                while i < chars.len() && chars[i] != ']' {
                    s.push(chars[i]);
                    i += 1;
                }
                if i >= chars.len() {
                    return Err(ParseError("字段引用缺少 ]".into()));
                }
                i += 1;
                out.push(Tok::Ref(s));
            }
            '(' | '（' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' | '）' => {
                out.push(Tok::RParen);
                i += 1;
            }
            ',' | '，' => {
                out.push(Tok::Comma);
                i += 1;
            }
            '.' => {
                out.push(Tok::Dot);
                i += 1;
            }
            _ if c.is_alphabetic() || c == '_' => {
                let s = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                out.push(Tok::Ident(chars[s..i].iter().collect()));
            }
            _ => {
                let two: String = chars[i..(i + 2).min(chars.len())].iter().collect();
                let op: &'static str = match two.as_str() {
                    ">=" => ">=",
                    "<=" => "<=",
                    "!=" => "!=",
                    "<>" => "!=",
                    "==" => "=",
                    "&&" => "&&",
                    "||" => "||",
                    _ => "",
                };
                if !op.is_empty() {
                    out.push(Tok::Op(op));
                    i += 2;
                    continue;
                }
                let op: &'static str = match c {
                    '+' => "+",
                    '-' => "-",
                    '*' => "*",
                    '/' => "/",
                    '%' => "%",
                    '&' => "&",
                    '=' => "=",
                    '>' => ">",
                    '<' => "<",
                    _ => return Err(ParseError(format!("无法识别的字符 {c}"))),
                };
                out.push(Tok::Op(op));
                i += 1;
            }
        }
    }
    Ok(out)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }
    fn expect(&mut self, t: Tok) -> Result<(), ParseError> {
        if self.next().as_ref() == Some(&t) {
            Ok(())
        } else {
            Err(ParseError("括号不匹配".into()))
        }
    }

    fn bin_op(&self) -> Option<(Bin, u8)> {
        let Tok::Op(o) = self.peek()? else { return None };
        Some(match *o {
            "||" => (Bin::Or, 1),
            "&&" => (Bin::And, 2),
            "=" => (Bin::Eq, 3),
            "!=" => (Bin::Ne, 3),
            ">" => (Bin::Gt, 3),
            ">=" => (Bin::Ge, 3),
            "<" => (Bin::Lt, 3),
            "<=" => (Bin::Le, 3),
            "&" => (Bin::Cat, 4),
            "+" => (Bin::Add, 5),
            "-" => (Bin::Sub, 5),
            "*" => (Bin::Mul, 6),
            "/" => (Bin::Div, 6),
            "%" => (Bin::Mod, 6),
            _ => return None,
        })
    }

    fn expr(&mut self, min: u8) -> Result<Expr, ParseError> {
        let mut lhs = self.unary()?;
        while let Some((op, prec)) = self.bin_op() {
            if prec < min {
                break;
            }
            self.next();
            let rhs = self.expr(prec + 1)?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        if self.peek() == Some(&Tok::Op("-")) {
            self.next();
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        if self.peek() == Some(&Tok::Op("+")) {
            self.next();
            return self.unary();
        }
        let mut e = self.primary()?;
        // Postfix chains.
        while self.peek() == Some(&Tok::Dot) {
            self.next();
            match self.next() {
                Some(Tok::Ref(name)) => e = Expr::Member(Box::new(e), name),
                Some(Tok::Ident(name)) => {
                    let args = if self.peek() == Some(&Tok::LParen) { self.args()? } else { vec![] };
                    e = Expr::Method(Box::new(e), name.to_uppercase(), args);
                }
                _ => return Err(ParseError("“.” 后应为函数或字段".into())),
            }
        }
        Ok(e)
    }

    fn args(&mut self) -> Result<Vec<Expr>, ParseError> {
        self.expect(Tok::LParen)?;
        let mut args = vec![];
        if self.peek() == Some(&Tok::RParen) {
            self.next();
            return Ok(args);
        }
        loop {
            args.push(self.expr(0)?);
            match self.next() {
                Some(Tok::Comma) => continue,
                Some(Tok::RParen) => break,
                _ => return Err(ParseError("函数参数缺少 )".into())),
            }
        }
        Ok(args)
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        match self.next() {
            Some(Tok::Num(n)) => Ok(Expr::Num(n)),
            Some(Tok::Str(s)) => Ok(Expr::Str(s)),
            Some(Tok::Ref(r)) => Ok(Expr::Ref(r)),
            Some(Tok::LParen) => {
                let e = self.expr(0)?;
                self.expect(Tok::RParen)?;
                Ok(e)
            }
            Some(Tok::Ident(name)) => {
                let up = name.to_uppercase();
                if self.peek() == Some(&Tok::LParen) {
                    let args = self.args()?;
                    return Ok(match up.as_str() {
                        "TRUE" if args.is_empty() => Expr::Bool(true),
                        "FALSE" if args.is_empty() => Expr::Bool(false),
                        _ => Expr::Call(up, args),
                    });
                }
                match up.as_str() {
                    "TRUE" => Ok(Expr::Bool(true)),
                    "FALSE" => Ok(Expr::Bool(false)),
                    "CURRENTVALUE" => Ok(Expr::CurrentValue),
                    _ => Err(ParseError(format!("未知名称 {name}，字段请写作 [{name}]"))),
                }
            }
            Some(t) => Err(ParseError(format!("意外的 {:?}", t))),
            None => Err(ParseError("公式不完整".into())),
        }
    }
}

pub fn parse(src: &str) -> Result<Expr, ParseError> {
    let src = src.trim().strip_prefix('=').unwrap_or(src.trim());
    if src.trim().is_empty() {
        return Err(ParseError("公式为空".into()));
    }
    let toks = lex(src)?;
    let mut p = Parser { toks, pos: 0 };
    let e = p.expr(0)?;
    if p.pos < p.toks.len() {
        return Err(ParseError("公式多余的内容".into()));
    }
    Ok(e)
}

/// The function names, for the formula editor's list.
pub const FUNCTIONS: &[(&str, &str, &str)] = &[
    ("IF", "IF(条件, 真值, 假值)", "逻辑"),
    ("IFS", "IFS(条件1, 值1, 条件2, 值2...)", "逻辑"),
    ("SWITCH", "SWITCH(表达式, 值1, 结果1, ..., 默认值)", "逻辑"),
    ("IFERROR", "IFERROR(表达式, 出错时的值)", "逻辑"),
    ("IFBLANK", "IFBLANK(表达式, 为空时的值)", "逻辑"),
    ("AND", "AND(条件1, 条件2...)", "逻辑"),
    ("OR", "OR(条件1, 条件2...)", "逻辑"),
    ("NOT", "NOT(条件)", "逻辑"),
    ("ISBLANK", "ISBLANK(值)", "逻辑"),
    ("ISERROR", "ISERROR(表达式)", "逻辑"),
    ("ISNUMBER", "ISNUMBER(值)", "逻辑"),
    ("CONTAIN", "CONTAIN(列表, 值...)", "逻辑"),
    ("RECORD_ID", "RECORD_ID()", "逻辑"),
    ("SUM", "SUM(数值1, 数值2...)", "数字"),
    ("AVERAGE", "AVERAGE(数值1, 数值2...)", "数字"),
    ("MAX", "MAX(数值1, 数值2...)", "数字"),
    ("MIN", "MIN(数值1, 数值2...)", "数字"),
    ("MEDIAN", "MEDIAN(数值1, 数值2...)", "数字"),
    ("COUNTA", "COUNTA(值1, 值2...)", "数字"),
    ("COUNTIF", "COUNTIF(数据范围, 条件)", "数字"),
    ("SUMIF", "SUMIF(数据范围, 条件)", "数字"),
    ("ROUND", "ROUND(数值, 位数)", "数字"),
    ("ROUNDUP", "ROUNDUP(数值, 位数)", "数字"),
    ("ROUNDDOWN", "ROUNDDOWN(数值, 位数)", "数字"),
    ("FLOOR", "FLOOR(数值, 基数)", "数字"),
    ("CEILING", "CEILING(数值, 基数)", "数字"),
    ("ABS", "ABS(数值)", "数字"),
    ("INT", "INT(数值)", "数字"),
    ("MOD", "MOD(被除数, 除数)", "数字"),
    ("POWER", "POWER(底数, 指数)", "数字"),
    ("SQRT", "SQRT(数值)", "数字"),
    ("QUOTIENT", "QUOTIENT(被除数, 除数)", "数字"),
    ("VALUE", "VALUE(文本)", "数字"),
    ("ISODD", "ISODD(数值)", "数字"),
    ("RANK", "RANK(值, 范围, 升序)", "数字"),
    ("PI", "PI()", "数字"),
    ("CONCATENATE", "CONCATENATE(文本1, 文本2...)", "文本"),
    ("LEN", "LEN(文本)", "文本"),
    ("LEFT", "LEFT(文本, 个数)", "文本"),
    ("RIGHT", "RIGHT(文本, 个数)", "文本"),
    ("MID", "MID(文本, 开始位置, 个数)", "文本"),
    ("FIND", "FIND(查找值, 文本, 开始位置)", "文本"),
    ("REPLACE", "REPLACE(文本, 开始位置, 个数, 新文本)", "文本"),
    ("SUBSTITUTE", "SUBSTITUTE(文本, 旧文本, 新文本)", "文本"),
    ("UPPER", "UPPER(文本)", "文本"),
    ("LOWER", "LOWER(文本)", "文本"),
    ("TRIM", "TRIM(文本)", "文本"),
    ("TEXT", "TEXT(值, 格式)", "文本"),
    ("CONTAINTEXT", "CONTAINTEXT(文本, 查找文本)", "文本"),
    ("SPLIT", "SPLIT(文本, 分隔符)", "文本"),
    ("TODATE", "TODATE(文本)", "文本"),
    ("CHAR", "CHAR(数值)", "文本"),
    ("FORMAT", "FORMAT(模板, 值1, 值2...)", "文本"),
    ("HYPERLINK", "HYPERLINK(链接, 文本)", "文本"),
    ("REGEXMATCH", "REGEXMATCH(文本, 正则)", "文本"),
    ("NOW", "NOW()", "日期"),
    ("TODAY", "TODAY()", "日期"),
    ("DATE", "DATE(年, 月, 日)", "日期"),
    ("YEAR", "YEAR(日期)", "日期"),
    ("MONTH", "MONTH(日期)", "日期"),
    ("DAY", "DAY(日期)", "日期"),
    ("HOUR", "HOUR(日期)", "日期"),
    ("MINUTE", "MINUTE(日期)", "日期"),
    ("SECOND", "SECOND(日期)", "日期"),
    ("WEEKDAY", "WEEKDAY(日期, 类型)", "日期"),
    ("WEEKNUM", "WEEKNUM(日期)", "日期"),
    ("DAYS", "DAYS(结束日期, 开始日期)", "日期"),
    ("DATEDIF", "DATEDIF(开始日期, 结束日期, 单位)", "日期"),
    ("EDATE", "EDATE(日期, 月数)", "日期"),
    ("EOMONTH", "EOMONTH(日期, 月数)", "日期"),
    ("WORKDAY", "WORKDAY(开始日期, 天数)", "日期"),
    ("NETWORKDAYS", "NETWORKDAYS(开始日期, 结束日期)", "日期"),
    ("LIST", "LIST(值1, 值2...)", "列表"),
    ("FIRST", "FIRST(列表)", "列表"),
    ("LAST", "LAST(列表)", "列表"),
    ("NTH", "NTH(列表, 位置)", "列表"),
    ("FILTER", "[表].FILTER(条件).[字段]", "列表"),
    ("MAP", "列表.MAP(表达式)", "列表"),
    ("SORT", "SORT(列表, 升序)", "列表"),
    ("UNIQUE", "UNIQUE(列表)", "列表"),
    ("ARRAYJOIN", "ARRAYJOIN(列表, 分隔符)", "列表"),
    ("LISTCOMBINE", "LISTCOMBINE(列表1, 列表2...)", "列表"),
    ("SEQUENCE", "SEQUENCE(开始, 结束, 步长)", "列表"),
];

/// A formula's value while it computes.
#[derive(Clone, Debug, PartialEq)]
pub enum FV {
    Null,
    Num(f64),
    Text(String),
    Bool(bool),
    Date(i64),
    List(Vec<FV>),
    /// Records of a table (a table, a filter's result, a link's records).
    Records(Id, Vec<Id>),
    /// One record (`CurrentValue` over a table).
    Row(Id, Id),
    Err(String),
}

impl FV {
    fn is_blank(&self) -> bool {
        match self {
            FV::Null => true,
            FV::Text(s) => s.is_empty(),
            FV::List(l) => l.is_empty() || l.iter().all(FV::is_blank),
            FV::Records(_, r) => r.is_empty(),
            _ => false,
        }
    }
    fn is_err(&self) -> bool {
        matches!(self, FV::Err(_))
    }
}

fn err(s: &str) -> FV {
    FV::Err(s.into())
}

pub struct Ctx<'a> {
    pub base: &'a Base,
    pub table: &'a Table,
    pub record: &'a Record,
}

impl<'a> Ctx<'a> {
    fn text(&self, v: &FV) -> String {
        match v {
            FV::Null => String::new(),
            FV::Num(n) => value::format_plain(*n),
            FV::Text(s) => s.clone(),
            FV::Bool(b) => if *b { "TRUE" } else { "FALSE" }.into(),
            FV::Date(d) => value::format_date(*d, super::DateFormat::YmdSlash, value::hour_minute(*d) != (0, 0)),
            FV::List(l) => l.iter().map(|x| self.text(x)).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(","),
            FV::Records(t, recs) => match self.base.table(t) {
                Some(tt) => recs.iter().map(|r| value::record_title(self.base, tt, r)).collect::<Vec<_>>().join(","),
                None => String::new(),
            },
            FV::Row(t, r) => self.base.table(t).map(|tt| value::record_title(self.base, tt, r)).unwrap_or_default(),
            FV::Err(e) => e.clone(),
        }
    }

    fn num(&self, v: &FV) -> Option<f64> {
        match v {
            FV::Num(n) => Some(*n),
            FV::Bool(b) => Some(*b as i32 as f64),
            FV::Text(s) => s.trim().replace(',', "").parse().ok(),
            FV::Date(d) => Some(*d as f64 / value::DAY_MS as f64),
            FV::List(l) if l.len() == 1 => self.num(&l[0]),
            FV::Null => Some(0.0),
            _ => None,
        }
    }

    fn truthy(&self, v: &FV) -> bool {
        match v {
            FV::Bool(b) => *b,
            FV::Num(n) => *n != 0.0,
            FV::Text(s) => !s.is_empty() && s.to_uppercase() != "FALSE",
            FV::Null => false,
            FV::List(l) => !l.is_empty(),
            _ => !v.is_blank(),
        }
    }

    fn date(&self, v: &FV) -> Option<i64> {
        match v {
            FV::Date(d) => Some(*d),
            FV::Text(s) => value::parse_date(s),
            FV::List(l) if l.len() == 1 => self.date(&l[0]),
            FV::Num(n) => Some((*n * value::DAY_MS as f64) as i64),
            _ => None,
        }
    }

    /// Every scalar of `v`, lists and records flattened.
    fn flat(&self, v: &FV, out: &mut Vec<FV>) {
        match v {
            FV::List(l) => l.iter().for_each(|x| self.flat(x, out)),
            FV::Records(t, recs) => {
                for r in recs {
                    out.push(FV::Row(t.clone(), r.clone()))
                }
            }
            x => out.push(x.clone()),
        }
    }

    fn flats(&self, args: &[FV]) -> Vec<FV> {
        let mut out = vec![];
        for a in args {
            self.flat(a, &mut out);
        }
        out
    }

    fn nums(&self, args: &[FV]) -> Vec<f64> {
        self.flats(args)
            .iter()
            .filter(|x| !x.is_blank())
            .filter_map(|x| match x {
                FV::Text(s) => s.trim().parse().ok(),
                other => self.num(other),
            })
            .collect()
    }

    /// The value of `field` of a record, as a formula sees it.
    fn field_value(&self, table: &Table, record: &Record, name: &str) -> FV {
        let Some(f) = table.field_by_name(name) else {
            return err("#REF!");
        };
        let v = super::query::cell(self.base, table, record, f);
        let Some(v) = v else {
            return match &f.kind {
                FieldKind::Checkbox => FV::Bool(false),
                FieldKind::Link { table: t, .. } => FV::Records(t.clone(), vec![]),
                FieldKind::MultiSelect { .. } | FieldKind::User { multiple: true } => FV::List(vec![]),
                _ => FV::Null,
            };
        };
        self.from_value(table, &f.kind, &v)
    }

    fn from_value(&self, table: &Table, kind: &FieldKind, v: &Value) -> FV {
        match (v, kind) {
            (Value::Text(s), _) => FV::Text(s.clone()),
            (Value::Number(n), _) => FV::Num(*n),
            (Value::Bool(b), _) => FV::Bool(*b),
            (Value::Date(d), _) => FV::Date(*d),
            (Value::Options(ids), FieldKind::SingleSelect { options }) => ids
                .first()
                .and_then(|id| options.iter().find(|o| &o.id == id))
                .map(|o| FV::Text(o.name.clone()))
                .unwrap_or(FV::Null),
            (Value::Options(ids), k) => FV::List(
                ids.iter()
                    .filter_map(|id| k.options().and_then(|o| o.iter().find(|o| &o.id == id)))
                    .map(|o| FV::Text(o.name.clone()))
                    .collect(),
            ),
            (Value::Users(ids), FieldKind::User { multiple: false }) => {
                ids.first().map(|id| FV::Text(self.base.member_name(id))).unwrap_or(FV::Null)
            }
            (Value::Users(ids), _) => FV::List(ids.iter().map(|id| FV::Text(self.base.member_name(id))).collect()),
            (Value::Links(ids), FieldKind::Link { table: t, .. }) => FV::Records(t.clone(), ids.clone()),
            (Value::Links(_), _) => FV::Null,
            (Value::Files(f), _) => FV::List(f.iter().map(|a| FV::Text(a.name.clone())).collect()),
            (Value::Url { link, .. }, _) => FV::Text(link.clone()),
            (Value::List(items), _) => FV::List(items.iter().map(|x| self.from_value(table, &FieldKind::Text, x)).collect()),
            (Value::Error(e), _) => FV::Err(e.clone()),
        }
    }

    fn member(&self, of: FV, name: &str) -> FV {
        match of {
            FV::Row(t, r) => {
                let Some(tt) = self.base.table(&t) else { return err("#REF!") };
                let Some(rr) = tt.record(&r) else { return FV::Null };
                self.field_value(tt, rr, name)
            }
            FV::Records(t, recs) => {
                let Some(tt) = self.base.table(&t) else { return err("#REF!") };
                if tt.field_by_name(name).is_none() {
                    return err("#REF!");
                }
                let vals: Vec<FV> = recs
                    .iter()
                    .filter_map(|r| tt.record(r))
                    .map(|rr| self.field_value(tt, rr, name))
                    .collect();
                // Links of links stay records, so chains go on.
                if vals.iter().all(|v| matches!(v, FV::Records(..))) && !vals.is_empty() {
                    let mut table = String::new();
                    let mut all = vec![];
                    for v in vals {
                        if let FV::Records(t2, rs) = v {
                            table = t2;
                            for r in rs {
                                if !all.contains(&r) {
                                    all.push(r);
                                }
                            }
                        }
                    }
                    return FV::Records(table, all);
                }
                FV::List(vals)
            }
            FV::Err(e) => FV::Err(e),
            _ => err("#VALUE!"),
        }
    }

    pub fn eval(&self, e: &Expr, cv: Option<&FV>) -> FV {
        match e {
            Expr::Num(n) => FV::Num(*n),
            Expr::Str(s) => FV::Text(s.clone()),
            Expr::Bool(b) => FV::Bool(*b),
            Expr::CurrentValue => cv.cloned().unwrap_or_else(|| err("#CURRENTVALUE!")),
            Expr::Ref(name) => {
                if self.table.field_by_name(name).is_some() {
                    self.field_value(self.table, self.record, name)
                } else if let Some(t) = self.base.tables.iter().find(|t| &t.name == name) {
                    FV::Records(t.id.clone(), t.records.iter().map(|r| r.id.clone()).collect())
                } else {
                    err("#REF!")
                }
            }
            Expr::Member(of, name) => {
                // CurrentValue.[x] reads the row the iteration is at.
                let of = self.eval(of, cv);
                self.member(of, name)
            }
            Expr::Neg(x) => match self.num(&self.eval(x, cv)) {
                Some(n) => FV::Num(-n),
                None => err("#VALUE!"),
            },
            Expr::Bin(op, a, b) => self.binary(*op, a, b, cv),
            Expr::Call(name, args) => self.call(name, None, args, cv),
            Expr::Method(of, name, args) => self.call(name, Some(of), args, cv),
        }
    }

    fn binary(&self, op: Bin, a: &Expr, b: &Expr, cv: Option<&FV>) -> FV {
        if op == Bin::And {
            let x = self.eval(a, cv);
            if !self.truthy(&x) {
                return FV::Bool(false);
            }
            return FV::Bool(self.truthy(&self.eval(b, cv)));
        }
        if op == Bin::Or {
            let x = self.eval(a, cv);
            if self.truthy(&x) {
                return FV::Bool(true);
            }
            return FV::Bool(self.truthy(&self.eval(b, cv)));
        }
        let x = self.eval(a, cv);
        let y = self.eval(b, cv);
        if let FV::Err(e) = &x {
            return FV::Err(e.clone());
        }
        if let FV::Err(e) = &y {
            return FV::Err(e.clone());
        }
        match op {
            Bin::Cat => FV::Text(format!("{}{}", self.text(&x), self.text(&y))),
            Bin::Add | Bin::Sub => {
                // Dates move by days; a date minus a date is days.
                match (&x, &y) {
                    (FV::Date(d), other) | (other, FV::Date(d)) if !matches!(other, FV::Date(_)) => {
                        let Some(n) = self.num(other) else { return err("#VALUE!") };
                        let delta = (n * value::DAY_MS as f64) as i64;
                        if op == Bin::Add {
                            FV::Date(d + delta)
                        } else if matches!(x, FV::Date(_)) {
                            FV::Date(d - delta)
                        } else {
                            err("#VALUE!")
                        }
                    }
                    (FV::Date(d1), FV::Date(d2)) if op == Bin::Sub => FV::Num((d1 - d2) as f64 / value::DAY_MS as f64),
                    _ => match (self.num(&x), self.num(&y)) {
                        (Some(p), Some(q)) => FV::Num(if op == Bin::Add { p + q } else { p - q }),
                        _ => err("#VALUE!"),
                    },
                }
            }
            Bin::Mul | Bin::Div | Bin::Mod => match (self.num(&x), self.num(&y)) {
                (Some(p), Some(q)) => match op {
                    Bin::Mul => FV::Num(p * q),
                    Bin::Div if q == 0.0 => err("#DIV/0!"),
                    Bin::Div => FV::Num(p / q),
                    _ if q == 0.0 => err("#DIV/0!"),
                    _ => FV::Num(p.rem_euclid(q)),
                },
                _ => err("#VALUE!"),
            },
            Bin::Eq | Bin::Ne => {
                let eq = self.equal(&x, &y);
                FV::Bool(if op == Bin::Eq { eq } else { !eq })
            }
            Bin::Gt | Bin::Ge | Bin::Lt | Bin::Le => {
                let ord = match (&x, &y) {
                    (FV::Date(p), FV::Date(q)) => p.partial_cmp(q),
                    (FV::Text(p), FV::Text(q)) if self.num(&x).is_none() || self.num(&y).is_none() => Some(p.cmp(q)),
                    _ => match (self.num(&x), self.num(&y)) {
                        (Some(p), Some(q)) => p.partial_cmp(&q),
                        _ => None,
                    },
                };
                let Some(o) = ord else { return err("#VALUE!") };
                use std::cmp::Ordering::*;
                FV::Bool(match op {
                    Bin::Gt => o == Greater,
                    Bin::Ge => o != Less,
                    Bin::Lt => o == Less,
                    _ => o != Greater,
                })
            }
            Bin::And | Bin::Or => unreachable!(),
        }
    }

    fn equal(&self, x: &FV, y: &FV) -> bool {
        match (x, y) {
            (FV::Null, v) | (v, FV::Null) => v.is_blank(),
            (FV::Num(_), _) | (_, FV::Num(_)) => match (self.num(x), self.num(y)) {
                (Some(p), Some(q)) => (p - q).abs() < 1e-9,
                _ => false,
            },
            (FV::Date(p), FV::Date(q)) => p == q,
            (FV::Bool(p), FV::Bool(q)) => p == q,
            _ => self.text(x) == self.text(y),
        }
    }

    /// Items of a data range, for FILTER / MAP / COUNTIF: rows of a table or
    /// the elements of a list.
    fn items(&self, v: &FV) -> Vec<FV> {
        match v {
            FV::Records(t, recs) => recs.iter().map(|r| FV::Row(t.clone(), r.clone())).collect(),
            FV::List(l) => l.clone(),
            FV::Null => vec![],
            x => vec![x.clone()],
        }
    }

    fn call(&self, name: &str, recv: Option<&Expr>, args: &[Expr], cv: Option<&FV>) -> FV {
        // Lazy forms first.
        let all_args: Vec<&Expr> = recv.into_iter().chain(args.iter()).collect();
        let ev = |i: usize| -> FV { all_args.get(i).map(|e| self.eval(e, cv)).unwrap_or(FV::Null) };
        match name {
            "IF" => {
                let c = ev(0);
                if c.is_err() {
                    return c;
                }
                return if self.truthy(&c) { ev(1) } else if all_args.len() > 2 { ev(2) } else { FV::Bool(false) };
            }
            "IFS" => {
                let mut i = 0;
                while i + 1 < all_args.len() {
                    if self.truthy(&ev(i)) {
                        return ev(i + 1);
                    }
                    i += 2;
                }
                return err("#N/A");
            }
            "SWITCH" => {
                let x = ev(0);
                let mut i = 1;
                while i + 1 < all_args.len() {
                    if self.equal(&x, &ev(i)) {
                        return ev(i + 1);
                    }
                    i += 2;
                }
                return if i < all_args.len() { ev(i) } else { err("#N/A") };
            }
            "IFERROR" => {
                let x = ev(0);
                return if x.is_err() { ev(1) } else { x };
            }
            "IFBLANK" => {
                let x = ev(0);
                return if x.is_blank() { ev(1) } else { x };
            }
            "ISERROR" => return FV::Bool(ev(0).is_err()),
            "AND" => return FV::Bool(all_args.iter().all(|e| self.truthy(&self.eval(e, cv)))),
            "OR" => return FV::Bool(all_args.iter().any(|e| self.truthy(&self.eval(e, cv)))),
            "FILTER" | "COUNTIF" | "SUMIF" | "MAP" => {
                let range = ev(0);
                if range.is_err() {
                    return range;
                }
                let Some(cond) = all_args.get(1) else { return err("#N/A") };
                let items = self.items(&range);
                return match name {
                    "MAP" => FV::List(items.iter().map(|it| self.eval(cond, Some(it))).collect()),
                    "COUNTIF" => {
                        FV::Num(items.iter().filter(|it| self.truthy(&self.eval(cond, Some(it)))).count() as f64)
                    }
                    "SUMIF" => FV::Num(
                        items
                            .iter()
                            .filter(|it| self.truthy(&self.eval(cond, Some(it))))
                            .filter_map(|it| self.num(it))
                            .sum(),
                    ),
                    _ => match &range {
                        FV::Records(t, recs) => FV::Records(
                            t.clone(),
                            recs.iter()
                                .filter(|r| {
                                    let row = FV::Row(t.clone(), (*r).clone());
                                    self.truthy(&self.eval(cond, Some(&row)))
                                })
                                .cloned()
                                .collect(),
                        ),
                        _ => FV::List(items.into_iter().filter(|it| self.truthy(&self.eval(cond, Some(it)))).collect()),
                    },
                };
            }
            "SORTBY" => {
                let range = ev(0);
                let by = ev(1);
                let asc = all_args.get(2).map(|_| self.truthy(&ev(2))).unwrap_or(true);
                if let (FV::Records(t, recs), FV::List(keys)) = (&range, &by) {
                    // Keys are of the whole table; align by record.
                    let Some(tt) = self.base.table(t) else { return err("#REF!") };
                    let mut pairs: Vec<(Id, FV)> = recs
                        .iter()
                        .map(|r| {
                            let i = tt.records.iter().position(|x| &x.id == r).unwrap_or(0);
                            (r.clone(), keys.get(i).cloned().unwrap_or(FV::Null))
                        })
                        .collect();
                    pairs.sort_by(|a, b| {
                        let o = self.order(&a.1, &b.1);
                        if asc { o } else { o.reverse() }
                    });
                    return FV::Records(t.clone(), pairs.into_iter().map(|p| p.0).collect());
                }
                return range;
            }
            _ => {}
        }
        let vals: Vec<FV> = all_args.iter().map(|e| self.eval(e, cv)).collect();
        if let Some(e) = vals.iter().find(|v| v.is_err()) {
            if !matches!(name, "ISBLANK" | "ISNUMBER") {
                return e.clone();
            }
        }
        let a = |i: usize| vals.get(i).cloned().unwrap_or(FV::Null);
        let n = |i: usize| self.num(&a(i));
        let t = |i: usize| self.text(&a(i));
        let d = |i: usize| self.date(&a(i));
        let nums = || self.nums(&vals);
        match name {
            "NOT" => FV::Bool(!self.truthy(&a(0))),
            "ISBLANK" => FV::Bool(a(0).is_blank()),
            "ISNULL" => FV::Bool(matches!(a(0), FV::Null)),
            "ISNUMBER" => FV::Bool(matches!(a(0), FV::Num(_))),
            "TRUE" => FV::Bool(true),
            "FALSE" => FV::Bool(false),
            "RECORD_ID" => FV::Text(self.record.id.clone()),
            "CONTAIN" | "CONTAINSALL" | "CONTAINSONLY" => {
                let hay: Vec<String> = self.flats(&vals[..1.min(vals.len())]).iter().map(|x| self.text(x)).collect();
                let needles: Vec<String> = self.flats(&vals[1.min(vals.len())..]).iter().map(|x| self.text(x)).collect();
                FV::Bool(match name {
                    "CONTAIN" => needles.iter().any(|x| hay.contains(x)),
                    "CONTAINSALL" => needles.iter().all(|x| hay.contains(x)),
                    _ => hay.iter().all(|x| needles.contains(x)) && !hay.is_empty(),
                })
            }
            "SUM" => FV::Num(nums().iter().sum()),
            "AVERAGE" => {
                let v = nums();
                if v.is_empty() { err("#DIV/0!") } else { FV::Num(v.iter().sum::<f64>() / v.len() as f64) }
            }
            "MAX" => nums().into_iter().reduce(f64::max).map(FV::Num).unwrap_or(FV::Num(0.0)),
            "MIN" => nums().into_iter().reduce(f64::min).map(FV::Num).unwrap_or(FV::Num(0.0)),
            "MEDIAN" => {
                let mut v = nums();
                if v.is_empty() {
                    return err("#N/A");
                }
                v.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let m = v.len() / 2;
                FV::Num(if v.len() % 2 == 0 { (v[m - 1] + v[m]) / 2.0 } else { v[m] })
            }
            "COUNTA" | "COUNT" => FV::Num(self.flats(&vals).iter().filter(|x| !x.is_blank()).count() as f64),
            "ROUND" | "ROUNDUP" | "ROUNDDOWN" => {
                let Some(x) = n(0) else { return err("#VALUE!") };
                let digits = n(1).unwrap_or(0.0) as i32;
                let f = 10f64.powi(digits);
                let y = x * f;
                FV::Num(
                    match name {
                        "ROUND" => y.round(),
                        "ROUNDUP" => {
                            if y >= 0.0 {
                                y.ceil()
                            } else {
                                y.floor()
                            }
                        }
                        _ => y.trunc(),
                    } / f,
                )
            }
            "FLOOR" | "CEILING" => {
                let Some(x) = n(0) else { return err("#VALUE!") };
                let b = n(1).filter(|b| *b != 0.0).unwrap_or(1.0);
                FV::Num(if name == "FLOOR" { (x / b).floor() * b } else { (x / b).ceil() * b })
            }
            "ABS" => n(0).map(|x| FV::Num(x.abs())).unwrap_or(err("#VALUE!")),
            "INT" => n(0).map(|x| FV::Num(x.floor())).unwrap_or(err("#VALUE!")),
            "SQRT" => n(0).filter(|x| *x >= 0.0).map(|x| FV::Num(x.sqrt())).unwrap_or(err("#NUM!")),
            "MOD" => match (n(0), n(1)) {
                (Some(_), Some(q)) if q == 0.0 => err("#DIV/0!"),
                (Some(p), Some(q)) => FV::Num(p.rem_euclid(q)),
                _ => err("#VALUE!"),
            },
            "POWER" => match (n(0), n(1)) {
                (Some(p), Some(q)) => FV::Num(p.powf(q)),
                _ => err("#VALUE!"),
            },
            "QUOTIENT" => match (n(0), n(1)) {
                (Some(_), Some(q)) if q == 0.0 => err("#DIV/0!"),
                (Some(p), Some(q)) => FV::Num((p / q).trunc()),
                _ => err("#VALUE!"),
            },
            "VALUE" => {
                let s = t(0);
                let s: String = s.chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect();
                s.parse().map(FV::Num).unwrap_or(err("#VALUE!"))
            }
            "ISODD" => FV::Bool(n(0).map(|x| (x as i64) % 2 != 0).unwrap_or(false)),
            "RANK" => {
                let Some(x) = n(0) else { return err("#VALUE!") };
                let range = self.nums(&vals[1.min(vals.len())..2.min(vals.len())]);
                let asc = vals.len() > 2 && self.truthy(&a(2));
                FV::Num(1.0 + range.iter().filter(|y| if asc { **y < x } else { **y > x }).count() as f64)
            }
            "PI" => FV::Num(std::f64::consts::PI),
            "SIN" | "COS" | "TAN" | "ASIN" | "ACOS" | "ATAN" | "LN" | "LOG10" | "EXP" => {
                let Some(x) = n(0) else { return err("#VALUE!") };
                FV::Num(match name {
                    "SIN" => x.sin(),
                    "COS" => x.cos(),
                    "TAN" => x.tan(),
                    "ASIN" => x.asin(),
                    "ACOS" => x.acos(),
                    "ATAN" => x.atan(),
                    "LN" => x.ln(),
                    "LOG10" => x.log10(),
                    _ => x.exp(),
                })
            }
            "SEQUENCE" => {
                let (Some(s), Some(e)) = (n(0), n(1)) else { return err("#VALUE!") };
                let step = n(2).filter(|x| *x != 0.0).unwrap_or(1.0);
                let mut out = vec![];
                let mut x = s;
                while (step > 0.0 && x <= e) || (step < 0.0 && x >= e) {
                    out.push(FV::Num(x));
                    x += step;
                    if out.len() > 10000 {
                        break;
                    }
                }
                FV::List(out)
            }
            "CONCATENATE" => FV::Text(self.flats(&vals).iter().map(|x| self.text(x)).collect()),
            "LEN" => FV::Num(t(0).chars().count() as f64),
            "LEFT" | "RIGHT" => {
                let s: Vec<char> = t(0).chars().collect();
                let k = (n(1).unwrap_or(1.0).max(0.0) as usize).min(s.len());
                FV::Text(if name == "LEFT" { s[..k].iter().collect() } else { s[s.len() - k..].iter().collect() })
            }
            "MID" => {
                let s: Vec<char> = t(0).chars().collect();
                let start = (n(1).unwrap_or(1.0).max(1.0) as usize - 1).min(s.len());
                let k = (n(2).unwrap_or(0.0).max(0.0) as usize).min(s.len() - start);
                FV::Text(s[start..start + k].iter().collect())
            }
            "FIND" => {
                let needle = t(0);
                let hay = t(1);
                let start = n(2).unwrap_or(1.0).max(1.0) as usize - 1;
                let hay_chars: Vec<char> = hay.chars().collect();
                let tail: String = hay_chars.iter().skip(start).collect();
                match tail.find(&needle) {
                    Some(b) => FV::Num((tail[..b].chars().count() + start + 1) as f64),
                    None => FV::Num(-1.0),
                }
            }
            "REPLACE" => {
                let s: Vec<char> = t(0).chars().collect();
                let start = (n(1).unwrap_or(1.0).max(1.0) as usize - 1).min(s.len());
                let k = (n(2).unwrap_or(0.0).max(0.0) as usize).min(s.len() - start);
                let mut out: String = s[..start].iter().collect();
                out.push_str(&t(3));
                out.extend(&s[start + k..]);
                FV::Text(out)
            }
            "SUBSTITUTE" => {
                let s = t(0);
                let old = t(1);
                if old.is_empty() {
                    return FV::Text(s);
                }
                match n(3) {
                    Some(k) if vals.len() > 3 => {
                        let k = k as usize;
                        let new = t(2);
                        let mut out = String::new();
                        let mut rest = s.as_str();
                        let mut i = 0;
                        while let Some(p) = rest.find(&old) {
                            i += 1;
                            out.push_str(&rest[..p]);
                            out.push_str(if i == k { &new } else { &old });
                            rest = &rest[p + old.len()..];
                        }
                        out.push_str(rest);
                        FV::Text(out)
                    }
                    _ => FV::Text(s.replace(&old, &t(2))),
                }
            }
            "UPPER" => FV::Text(t(0).to_uppercase()),
            "LOWER" => FV::Text(t(0).to_lowercase()),
            "TRIM" => FV::Text(t(0).trim().to_string()),
            "CONTAINTEXT" => FV::Bool(t(0).contains(&t(1))),
            "SPLIT" => {
                let sep = t(1);
                let s = t(0);
                FV::List(if sep.is_empty() {
                    s.chars().map(|c| FV::Text(c.to_string())).collect()
                } else {
                    s.split(&sep).map(|x| FV::Text(x.to_string())).collect()
                })
            }
            "TODATE" => d(0).map(FV::Date).unwrap_or(err("#VALUE!")),
            "CHAR" => n(0).and_then(|x| char::from_u32(x as u32)).map(|c| FV::Text(c.to_string())).unwrap_or(err("#VALUE!")),
            "FORMAT" => {
                let mut s = t(0);
                for i in 1..vals.len() {
                    s = s.replace(&format!("{{{i}}}"), &t(i));
                }
                FV::Text(s)
            }
            "HYPERLINK" => FV::Text(if vals.len() > 1 { t(1) } else { t(0) }),
            "ENCODEURL" => FV::Text(
                t(0).bytes()
                    .map(|b| {
                        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                            (b as char).to_string()
                        } else {
                            format!("%{b:02X}")
                        }
                    })
                    .collect(),
            ),
            "REGEXMATCH" => FV::Bool(simple_regex_match(&t(0), &t(1))),
            "TEXT" => self.text_fn(&a(0), &t(1)),
            "NOW" => FV::Date(super::now_ms()),
            "TODAY" => FV::Date(value::today_ms()),
            "DATE" => match (n(0), n(1), n(2)) {
                (Some(y), Some(m), Some(dd)) => {
                    // Months and days past their range roll over.
                    let base = value::date_ms(y as i32, 1, 1);
                    let dt = value::local(base);
                    let months = m as i32 - 1;
                    let ym = dt.year() * 12 + months;
                    let first = value::date_ms(ym.div_euclid(12), (ym.rem_euclid(12) + 1) as u32, 1);
                    FV::Date(first + ((dd as i64) - 1) * value::DAY_MS)
                }
                _ => err("#VALUE!"),
            },
            "YEAR" | "MONTH" | "DAY" | "HOUR" | "MINUTE" | "SECOND" | "WEEKDAY" | "WEEKNUM" => {
                let Some(ms) = d(0) else { return err("#VALUE!") };
                let x = value::local(ms);
                FV::Num(match name {
                    "YEAR" => x.year() as f64,
                    "MONTH" => x.month() as f64,
                    "DAY" => x.day() as f64,
                    "HOUR" => x.hour() as f64,
                    "MINUTE" => x.minute() as f64,
                    "SECOND" => x.second() as f64,
                    "WEEKDAY" => {
                        let ty = n(1).unwrap_or(1.0) as i32;
                        let sun0 = x.weekday().num_days_from_sunday() as f64;
                        match ty {
                            2 => x.weekday().number_from_monday() as f64,
                            3 => x.weekday().num_days_from_monday() as f64,
                            _ => sun0 + 1.0,
                        }
                    }
                    _ => x.iso_week().week() as f64,
                })
            }
            "DAYS" => match (d(0), d(1)) {
                (Some(e), Some(s)) => FV::Num((e - s) as f64 / value::DAY_MS as f64),
                _ => err("#VALUE!"),
            },
            "DATEDIF" => {
                let (Some(s), Some(e)) = (d(0), d(1)) else { return err("#VALUE!") };
                if s > e {
                    return err("#NUM!");
                }
                let unit = t(2).to_uppercase();
                let (a1, b1) = (value::local(s), value::local(e));
                let months = (b1.year() - a1.year()) * 12 + b1.month() as i32 - a1.month() as i32
                    - if b1.day() < a1.day() { 1 } else { 0 };
                FV::Num(match unit.as_str() {
                    "M" => months as f64,
                    "Y" => (months / 12) as f64,
                    _ => ((value::day_start(e) - value::day_start(s)) / value::DAY_MS) as f64,
                })
            }
            "DURATION" => FV::Num(n(0).unwrap_or(0.0) + n(1).unwrap_or(0.0) / 24.0 + n(2).unwrap_or(0.0) / 1440.0),
            "EDATE" | "EOMONTH" => {
                let Some(ms) = d(0) else { return err("#VALUE!") };
                let x = value::local(ms);
                let ym = x.year() * 12 + x.month() as i32 - 1 + n(1).unwrap_or(0.0) as i32;
                let (y, m) = (ym.div_euclid(12), ym.rem_euclid(12) as u32 + 1);
                if name == "EOMONTH" {
                    let next = value::date_ms(if m == 12 { y + 1 } else { y }, if m == 12 { 1 } else { m + 1 }, 1);
                    FV::Date(next - value::DAY_MS)
                } else {
                    let mut day = x.day();
                    loop {
                        let v = value::date_ms(y, m, day);
                        if v != 0 || day <= 28 {
                            break FV::Date(v);
                        }
                        day -= 1;
                    }
                }
            }
            "WORKDAY" => {
                let (Some(s), Some(k)) = (d(0), n(1)) else { return err("#VALUE!") };
                let mut x = value::local(s);
                let mut left = k.abs() as i64;
                let step = if k >= 0.0 { 1 } else { -1 };
                while left > 0 {
                    x += Duration::days(step);
                    if x.weekday().number_from_monday() <= 5 {
                        left -= 1;
                    }
                }
                FV::Date(x.timestamp_millis())
            }
            "NETWORKDAYS" => {
                let (Some(s), Some(e)) = (d(0), d(1)) else { return err("#VALUE!") };
                let (mut x, end) = (value::day_start(s.min(e)), value::day_start(s.max(e)));
                let mut c = 0;
                while x <= end {
                    if value::local(x).weekday().number_from_monday() <= 5 {
                        c += 1;
                    }
                    x += value::DAY_MS;
                }
                FV::Num(if s <= e { c as f64 } else { -(c as f64) })
            }
            "LIST" | "LISTCOMBINE" => {
                if name == "LISTCOMBINE" {
                    let mut out = vec![];
                    for v in &vals {
                        self.flat(v, &mut out);
                    }
                    FV::List(out)
                } else {
                    FV::List(vals.clone())
                }
            }
            "FIRST" | "LAST" | "NTH" => {
                let items = self.items(&a(0));
                let item = match name {
                    "FIRST" => items.first().cloned(),
                    "LAST" => items.last().cloned(),
                    _ => n(1).and_then(|k| items.get((k as usize).saturating_sub(1)).cloned()),
                };
                match item {
                    Some(FV::Row(tb, r)) => FV::Records(tb, vec![r]),
                    Some(x) => x,
                    None => FV::Null,
                }
            }
            "SORT" => {
                let mut items = self.flats(&vals[..1.min(vals.len())]);
                let asc = vals.len() < 2 || self.truthy(&a(1));
                items.sort_by(|x, y| self.order(x, y));
                if !asc {
                    items.reverse();
                }
                FV::List(items)
            }
            "UNIQUE" => {
                let mut out: Vec<FV> = vec![];
                for x in self.flats(&vals) {
                    if !out.iter().any(|y| self.equal(y, &x)) {
                        out.push(x);
                    }
                }
                FV::List(out)
            }
            "ARRAYJOIN" => {
                let sep = if vals.len() > 1 { t(1) } else { ",".into() };
                FV::Text(self.flats(&vals[..1.min(vals.len())]).iter().map(|x| self.text(x)).collect::<Vec<_>>().join(&sep))
            }
            "RANDOMBETWEEN" => {
                let (lo, hi) = (n(0).unwrap_or(0.0) as i64, n(1).unwrap_or(0.0) as i64);
                let seed = self.record.created_at as u64 ^ 0x9E37_79B9;
                FV::Num((lo + (seed % ((hi - lo + 1).max(1)) as u64) as i64) as f64)
            }
            _ => err("#NAME?"),
        }
    }

    fn order(&self, x: &FV, y: &FV) -> std::cmp::Ordering {
        match (self.num(x), self.num(y)) {
            (Some(p), Some(q)) if !matches!(x, FV::Text(_)) => p.partial_cmp(&q).unwrap_or(std::cmp::Ordering::Equal),
            _ => value::natural_cmp(&self.text(x), &self.text(y)),
        }
    }

    /// TEXT(value, format): dates with YYYY MM DD hh mm ss, numbers with 0s.
    fn text_fn(&self, v: &FV, fmt: &str) -> FV {
        if let FV::Date(ms) = v {
            let x = value::local(*ms);
            let out = fmt
                .replace("YYYY", &format!("{:04}", x.year()))
                .replace("yyyy", &format!("{:04}", x.year()))
                .replace("MM", &format!("{:02}", x.month()))
                .replace("DD", &format!("{:02}", x.day()))
                .replace("dd", &format!("{:02}", x.day()))
                .replace("hh", &format!("{:02}", x.hour()))
                .replace("HH", &format!("{:02}", x.hour()))
                .replace("mm", &format!("{:02}", x.minute()))
                .replace("ss", &format!("{:02}", x.second()));
            return FV::Text(out);
        }
        if let Some(n) = self.num(v) {
            if fmt.chars().all(|c| c == '0' || c == '.' || c == '#' || c == ',') && !fmt.is_empty() {
                let (int_part, frac) = fmt.split_once('.').unwrap_or((fmt, ""));
                let decimals = frac.len();
                let mut s = format!("{:.*}", decimals, n);
                let width = int_part.chars().filter(|c| *c == '0').count() + if decimals > 0 { decimals + 1 } else { 0 };
                while s.len() < width {
                    s.insert(0, '0');
                }
                return FV::Text(s);
            }
            if fmt.ends_with('%') {
                return FV::Text(format!("{}%", value::format_plain(n * 100.0)));
            }
        }
        FV::Text(self.text(v))
    }

    /// The final value of a formula as a cell holds it.
    pub fn to_value(&self, v: FV) -> Option<Value> {
        match v {
            FV::Null => None,
            FV::Num(n) => {
                if n.is_finite() {
                    Some(Value::Number(n))
                } else {
                    Some(Value::Error("#NUM!".into()))
                }
            }
            FV::Text(s) => {
                if s.is_empty() {
                    None
                } else {
                    Some(Value::Text(s))
                }
            }
            FV::Bool(b) => Some(Value::Bool(b)),
            FV::Date(d) => Some(Value::Date(d)),
            FV::List(l) => {
                let items: Vec<Value> = l.into_iter().filter_map(|x| self.to_value(x)).collect();
                if items.is_empty() { None } else { Some(Value::List(items)) }
            }
            r @ (FV::Records(..) | FV::Row(..)) => {
                let s = self.text(&r);
                if s.is_empty() { None } else { Some(Value::Text(s)) }
            }
            FV::Err(e) => Some(Value::Error(e)),
        }
    }
}

/// `REGEXMATCH` without a regex engine: `^`, `$`, `.`, `*`, `+`, `?`, `\d`, `\w`, `[...]` and `|`.
fn simple_regex_match(text: &str, re: &str) -> bool {
    re.split('|').any(|alt| {
        let anchored = alt.starts_with('^');
        let pat = alt.trim_start_matches('^');
        let chars: Vec<char> = text.chars().collect();
        let p: Vec<char> = pat.chars().collect();
        if anchored {
            match_here(&p, &chars).is_some()
        } else {
            (0..=chars.len()).any(|i| match_here(&p, &chars[i..]).is_some())
        }
    })
}

fn match_here(p: &[char], t: &[char]) -> Option<usize> {
    if p.is_empty() {
        return Some(0);
    }
    if p == ['$'] {
        return if t.is_empty() { Some(0) } else { None };
    }
    let (class_len, matches): (usize, Box<dyn Fn(char) -> bool>) = match p[0] {
        '\\' if p.len() > 1 => {
            let c = p[1];
            (
                2,
                Box::new(move |x: char| match c {
                    'd' => x.is_ascii_digit(),
                    'w' => x.is_alphanumeric() || x == '_',
                    's' => x.is_whitespace(),
                    other => x == other,
                }),
            )
        }
        '[' => {
            let end = p.iter().position(|c| *c == ']').unwrap_or(p.len() - 1);
            let set: Vec<char> = p[1..end].to_vec();
            (
                end + 1,
                Box::new(move |x: char| {
                    let mut i = 0;
                    while i < set.len() {
                        if i + 2 < set.len() && set[i + 1] == '-' {
                            if x >= set[i] && x <= set[i + 2] {
                                return true;
                            }
                            i += 3;
                        } else {
                            if set[i] == x {
                                return true;
                            }
                            i += 1;
                        }
                    }
                    false
                }),
            )
        }
        '.' => (1, Box::new(|_| true)),
        c => (1, Box::new(move |x| x == c)),
    };
    let rest = &p[class_len..];
    match rest.first() {
        Some('*') | Some('+') | Some('?') => {
            let q = rest[0];
            let rest = &rest[1..];
            let mut n = 0;
            while n < t.len() && matches(t[n]) && (q != '?' || n < 1) {
                n += 1;
            }
            let min = if q == '+' { 1 } else { 0 };
            for k in (min..=n).rev() {
                if let Some(m) = match_here(rest, &t[k..]) {
                    return Some(k + m);
                }
            }
            None
        }
        _ => {
            if !t.is_empty() && matches(t[0]) {
                match_here(rest, &t[1..]).map(|m| m + 1)
            } else {
                None
            }
        }
    }
}

/// Evaluates `expr` for `record` of `table`.
pub fn evaluate(base: &Base, table: &Table, record: &Record, expr: &str) -> Option<Value> {
    let ctx = Ctx { base, table, record };
    match parse(expr) {
        Ok(e) => ctx.to_value(ctx.eval(&e, None)),
        Err(_) => Some(Value::Error("#ERROR!".into())),
    }
}

/// Checks a formula against a table: the first problem, if any.
pub fn check(base: &Base, table: &Table, expr: &str) -> Result<(), String> {
    let e = parse(expr).map_err(|e| e.0)?;
    fn walk(e: &Expr, base: &Base, table: &Table, out: &mut Vec<String>) {
        match e {
            Expr::Ref(n) => {
                if table.field_by_name(n).is_none() && !base.tables.iter().any(|t| &t.name == n) {
                    out.push(format!("找不到字段或数据表 [{n}]"));
                }
            }
            Expr::Call(name, args) => {
                if !FUNCTIONS.iter().any(|f| f.0 == name)
                    && !["ISNULL", "CONTAINSALL", "CONTAINSONLY", "SORTBY", "DURATION", "ENCODEURL", "RANDOMBETWEEN", "COUNT", "SIN", "COS", "TAN", "LN", "EXP", "LOG10", "ASIN", "ACOS", "ATAN"].contains(&name.as_str())
                {
                    out.push(format!("未知函数 {name}"));
                }
                args.iter().for_each(|a| walk(a, base, table, out));
            }
            Expr::Method(of, _, args) => {
                walk(of, base, table, out);
                args.iter().for_each(|a| walk(a, base, table, out));
            }
            Expr::Member(of, _) => walk(of, base, table, out),
            Expr::Bin(_, a, b) => {
                walk(a, base, table, out);
                walk(b, base, table, out);
            }
            Expr::Neg(a) => walk(a, base, table, out),
            _ => {}
        }
    }
    let mut problems = vec![];
    walk(&e, base, table, &mut problems);
    match problems.into_iter().next() {
        Some(p) => Err(p),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::sample;

    fn eval_in(base: &Base, table: usize, expr: &str) -> Option<Value> {
        let t = &base.tables[table];
        evaluate(base, t, &t.records[0], expr)
    }

    #[test]
    fn arithmetic_and_text() {
        let b = sample::sample();
        assert_eq!(eval_in(&b, 0, "1 + 2 * 3"), Some(Value::Number(7.0)));
        assert_eq!(eval_in(&b, 0, "(1 + 2) * 3"), Some(Value::Number(9.0)));
        assert_eq!(eval_in(&b, 0, "\"a\" & 1"), Some(Value::Text("a1".into())));
        assert_eq!(eval_in(&b, 0, "IF(1 > 2, \"x\", \"y\")"), Some(Value::Text("y".into())));
        assert_eq!(eval_in(&b, 0, "10 / 0"), Some(Value::Error("#DIV/0!".into())));
        assert_eq!(eval_in(&b, 0, "LEFT(\"hello\", 2)"), Some(Value::Text("he".into())));
        assert_eq!(eval_in(&b, 0, "\"abc\".UPPER()"), Some(Value::Text("ABC".into())));
        assert_eq!(eval_in(&b, 0, "SUM(LIST(1,2,3))"), Some(Value::Number(6.0)));
        assert_eq!(eval_in(&b, 0, "LIST(3,1,2).SORT().ARRAYJOIN(\"-\")"), Some(Value::Text("1-2-3".into())));
        assert_eq!(eval_in(&b, 0, "LIST(1,2,3).FILTER(CurrentValue > 1).SUM()"), Some(Value::Number(5.0)));
        assert_eq!(eval_in(&b, 0, "TEXT(DATE(2024,1,5), \"YYYY-MM-DD\")"), Some(Value::Text("2024-01-05".into())));
        assert_eq!(eval_in(&b, 0, "DATEDIF(DATE(2024,1,1), DATE(2024,3,1), \"M\")"), Some(Value::Number(2.0)));
        assert!(matches!(eval_in(&b, 0, "[不存在]"), Some(Value::Error(_))));
        assert!(simple_regex_match("abc123", "\\d+$"));
        assert!(!simple_regex_match("abc", "^\\d"));
    }

    #[test]
    fn field_refs() {
        let b = sample::sample();
        let t = &b.tables[0];
        let name = &t.fields[0].name;
        let v = eval_in(&b, 0, &format!("LEN([{name}])"));
        assert!(matches!(v, Some(Value::Number(n)) if n > 0.0));
        // A whole table's column.
        let tname = &t.name;
        let v = eval_in(&b, 0, &format!("[{tname}].[{name}].COUNTA()"));
        assert_eq!(v, Some(Value::Number(t.records.iter().filter(|r| r.cells.contains_key(&t.fields[0].id)).count() as f64)));
    }

    /// The same records and formulas as a Feishu Base, against the values
    /// Feishu computed (tests/feishu_formulas.json).
    #[test]
    fn matches_feishu() {
        use crate::model::*;
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/feishu_formulas.json")).unwrap();
        let mut b = sample::empty();
        let opts = |names: &[&str]| -> Vec<SelectOption> {
            names.iter().enumerate().map(|(i, n)| SelectOption { id: format!("o{n}"), name: n.to_string(), color: i as u8 }).collect()
        };
        let f = |id: &str, name: &str, kind: FieldKind| Field { id: id.into(), name: name.into(), kind, description: String::new() };
        let mut t = Table {
            id: "t".into(),
            name: "对齐".into(),
            fields: vec![
                f("n", "名称", FieldKind::Text),
                f("q", "数量", FieldKind::Number { precision: 0, thousands: false, percent: false }),
                f("p", "单价", FieldKind::Number { precision: 2, thousands: false, percent: false }),
                f("d", "日期", FieldKind::Date { format: DateFormat::YmdSlash, time: false }),
                f("s", "状态", FieldKind::SingleSelect { options: opts(&["进行中", "已完成"]) }),
                f("g", "标签", FieldKind::MultiSelect { options: opts(&["前端", "后端", "设计"]) }),
            ],
            records: vec![],
            views: vec![View::new("表格", ViewKind::Grid)],
            seq: 0,
            option_sources: std::collections::HashMap::new(),
        };
        for r in fixture["records"].as_array().unwrap() {
            let mut rec = t.new_record("usr0");
            rec.cells.insert("n".into(), Value::Text(r["名称"].as_str().unwrap().into()));
            rec.cells.insert("q".into(), Value::Number(r["数量"].as_f64().unwrap()));
            rec.cells.insert("p".into(), Value::Number(r["单价"].as_f64().unwrap()));
            let day = &r["日期"].as_str().unwrap()[..10];
            rec.cells.insert("d".into(), Value::Date(value::parse_date(day).unwrap()));
            if let Some(s) = r["状态"].as_array().and_then(|a| a.first()) {
                rec.cells.insert("s".into(), Value::Options(vec![format!("o{}", s.as_str().unwrap())]));
            }
            if let Some(g) = r["标签"].as_array() {
                if !g.is_empty() {
                    rec.cells.insert("g".into(), Value::Options(g.iter().map(|x| format!("o{}", x.as_str().unwrap())).collect()));
                }
            }
            t.push_record(rec);
        }
        b.tables = vec![t];
        let t = &b.tables[0];
        let shown = |v: Option<Value>| -> String {
            match v {
                None => String::new(),
                Some(Value::Bool(x)) => x.to_string(),
                Some(Value::Number(n)) => value::format_plain(n),
                Some(v) => value::display(&b, t, &FieldKind::Text, &v),
            }
        };
        let same = |got: &str, want: &str| -> bool {
            if got == want {
                return true;
            }
            match (got.parse::<f64>(), want.parse::<f64>()) {
                (Ok(a), Ok(w)) => (a - w).abs() <= 1e-6 * w.abs().max(1.0),
                _ => {
                    // A whole table's FILTER comes back in Feishu's internal
                    // record order: compare those lists as sets.
                    let mut a: Vec<&str> = got.split(',').collect();
                    let mut w: Vec<&str> = want.split(',').collect();
                    a.sort();
                    w.sort();
                    got.eq_ignore_ascii_case(want) || (a.len() > 1 && a == w)
                }
            }
        };
        let formulas = fixture["formulas"].as_array().unwrap();
        let mut bad = vec![];
        for (ri, rec) in t.records.iter().enumerate() {
            for (fi, expr) in formulas.iter().enumerate() {
                let expr = expr.as_str().unwrap();
                let want = match &fixture["expected"][ri][fi] {
                    serde_json::Value::Null => String::new(),
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let got = shown(evaluate(&b, t, rec, expr));
                if !same(&got, &want) {
                    bad.push(format!("F{:02} {expr} | record {ri}: rutable={got:?} feishu={want:?}", fi + 1));
                }
            }
        }
        assert!(bad.is_empty(), "{} mismatches with Feishu:\n{}", bad.len(), bad.join("\n"));
    }
}
