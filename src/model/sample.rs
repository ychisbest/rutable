//! The document a first start opens: a small project management base.

use std::collections::HashMap;

use super::*;

fn field(name: &str, kind: FieldKind) -> Field {
    Field { id: new_id("fld"), name: name.into(), kind, description: String::new() }
}

fn opts(names: &[(&str, u8)]) -> Vec<SelectOption> {
    names.iter().map(|(n, c)| SelectOption { id: new_id("opt"), name: (*n).into(), color: *c }).collect()
}

pub fn members() -> Vec<Member> {
    [("我", 0u8), ("张伟", 3), ("李娜", 7), ("王强", 6), ("刘洋", 9), ("陈静", 2)]
        .iter()
        .enumerate()
        .map(|(i, (n, c))| Member { id: format!("usr{i}"), name: (*n).into(), color: *c })
        .collect()
}

/// An empty base with one table, as a new base starts.
pub fn empty() -> Base {
    let mut b = Base {
        name: "未命名多维表格".into(),
        tables: vec![],
        dashboards: vec![],
        order: vec![],
        members: members(),
        me: "usr0".into(),
        workflows: vec![],
        advanced_perm: false,
        roles: vec![],
        member_roles: HashMap::new(),
    };
    b.new_table("数据表");
    b
}

pub fn sample() -> Base {
    let members = members();
    let me = members[0].id.clone();
    let day = value::DAY_MS;
    let today = value::today_ms();

    // 项目
    let p_name = field("项目名称", FieldKind::Text);
    let p_owner = field("项目经理", FieldKind::User { multiple: false });
    let p_budget = field("预算", FieldKind::Currency { symbol: "¥".into(), precision: 2 });
    let p_stage = field(
        "阶段",
        FieldKind::SingleSelect { options: opts(&[("规划中", 10), ("进行中", 0), ("已上线", 3)]) },
    );
    let p_tasks_id = new_id("fld");
    let mut projects = Table {
        id: new_id("tbl"),
        name: "项目".into(),
        fields: vec![],
        records: vec![],
        views: vec![View::new("全部项目", ViewKind::Grid)],
        seq: 0,
        option_sources: HashMap::new(),
    };

    // 任务
    let status_opts = opts(&[("未开始", 10), ("进行中", 0), ("已完成", 3), ("已延期", 7)]);
    let prio_opts = opts(&[("P0 紧急", 7), ("P1 重要", 6), ("P2 一般", 1)]);
    let tag_opts = opts(&[("前端", 0), ("后端", 2), ("设计", 9), ("测试", 5), ("文档", 4)]);
    let t_name = field("任务名称", FieldKind::Text);
    let t_status = field("状态", FieldKind::SingleSelect { options: status_opts.clone() });
    let t_prio = field("优先级", FieldKind::SingleSelect { options: prio_opts.clone() });
    let t_owner = field("负责人", FieldKind::User { multiple: true });
    let t_start = field("开始日期", FieldKind::Date { format: DateFormat::YmdSlash, time: false });
    let t_end = field("截止日期", FieldKind::Date { format: DateFormat::YmdSlash, time: false });
    let t_progress = field("进度", FieldKind::Progress { color: 0 });
    let t_hours = field("预估工时", FieldKind::Number { precision: 1, thousands: false, percent: false });
    let t_tags = field("标签", FieldKind::MultiSelect { options: tag_opts.clone() });
    let t_done = field("已验收", FieldKind::Checkbox);
    let t_rating = field("满意度", FieldKind::Rating { max: 5, icon: 0 });
    let t_files = field("附件", FieldKind::Attachment);
    let mut t_project = field("所属项目", FieldKind::Link { table: projects.id.clone(), multiple: false, pair: Some(p_tasks_id.clone()) });
    t_project.description = "关联「项目」表".into();
    let t_left = field("剩余天数", FieldKind::Formula { expr: "IF(ISBLANK([截止日期]), \"\", DAYS([截止日期], TODAY()))".into() });
    let t_note = field("备注", FieldKind::Text);
    let t_btn = field(
        "操作",
        FieldKind::Button {
            label: "标记完成".into(),
            color: 3,
            action: Action::SetField { field: t_status.id.clone(), value: Value::Options(vec![status_opts[2].id.clone()]) },
        },
    );
    let t_url = field("文档链接", FieldKind::Url);
    let t_no = field("编号", FieldKind::AutoNumber);
    let t_created = field("创建时间", FieldKind::CreatedTime { format: DateFormat::YmdSlash, time: true });

    let mut tasks = Table {
        id: new_id("tbl"),
        name: "任务".into(),
        fields: vec![
            t_name.clone(),
            t_status.clone(),
            t_prio.clone(),
            t_owner.clone(),
            t_start.clone(),
            t_end.clone(),
            t_progress.clone(),
            t_hours.clone(),
            t_tags.clone(),
            t_project.clone(),
            t_done.clone(),
            t_rating.clone(),
            t_btn.clone(),
            t_left.clone(),
            t_files.clone(),
            t_url.clone(),
            t_note.clone(),
            t_no.clone(),
            t_created.clone(),
        ],
        records: vec![],
        views: vec![],
        seq: 0,
        option_sources: HashMap::new(),
    };

    // Projects' records.
    let project_rows = [
        ("官网改版", 1usize, 280000.0, 1usize),
        ("移动端 App 2.0", 2, 650000.0, 1),
        ("数据中台", 3, 1200000.0, 0),
        ("客服系统升级", 4, 180000.0, 2),
    ];
    let mut project_ids = vec![];
    for (name, owner, budget, stage) in project_rows {
        let mut r = projects.new_record(&me);
        r.cells.insert(p_name.id.clone(), Value::Text(name.into()));
        r.cells.insert(p_owner.id.clone(), Value::Users(vec![members[owner].id.clone()]));
        r.cells.insert(p_budget.id.clone(), Value::Number(budget));
        if let FieldKind::SingleSelect { options } = &p_stage.kind {
            r.cells.insert(p_stage.id.clone(), Value::Options(vec![options[stage].id.clone()]));
        }
        project_ids.push(r.id.clone());
        projects.push_record(r);
    }

    // (name, status, prio, owners, start offset, length, progress, hours, tags, project, done, rating)
    let rows: &[(&str, usize, usize, &[usize], i64, i64, f64, f64, &[usize], usize, bool, u8)] = &[
        ("首页视觉设计", 2, 0, &[4], -20, 6, 1.0, 24.0, &[2], 0, true, 5),
        ("导航与页脚组件开发", 2, 1, &[1], -14, 5, 1.0, 16.0, &[0], 0, true, 4),
        ("产品页接口联调", 1, 0, &[1, 3], -6, 8, 0.6, 32.0, &[0, 1], 0, false, 0),
        ("SEO 优化方案", 0, 2, &[5], 3, 4, 0.0, 8.0, &[4], 0, false, 0),
        ("登录注册流程重构", 1, 0, &[2], -8, 10, 0.45, 40.0, &[0, 1], 1, false, 0),
        ("消息推送服务", 3, 1, &[3], -15, 7, 0.3, 28.0, &[1], 1, false, 2),
        ("App 图标与启动页", 2, 2, &[4], -25, 3, 1.0, 6.0, &[2], 1, true, 5),
        ("性能监控埋点", 0, 1, &[1], 5, 6, 0.0, 20.0, &[1, 3], 1, false, 0),
        ("数据仓库建模", 1, 0, &[3, 0], -10, 20, 0.35, 80.0, &[1], 2, false, 0),
        ("指标口径文档", 1, 2, &[5], -3, 7, 0.5, 12.0, &[4], 2, false, 0),
        ("权限体系设计", 0, 1, &[0], 2, 9, 0.0, 30.0, &[1, 2], 2, false, 0),
        ("工单自动分配", 2, 1, &[2], -18, 9, 1.0, 36.0, &[1], 3, true, 4),
        ("满意度回访问卷", 1, 2, &[5, 4], -2, 5, 0.2, 10.0, &[2, 4], 3, false, 0),
        ("回归测试", 0, 0, &[2], 6, 4, 0.0, 16.0, &[3], 3, false, 0),
    ];
    let notes = [
        "按照设计规范 v3 执行",
        "",
        "等待后端提供 mock 数据",
        "参考竞品分析报告",
        "需要安全团队评审",
        "第三方通道延迟较高，已提工单",
        "",
        "",
        "先完成 ODS 与 DWD 两层",
        "",
        "",
        "已上线灰度 30%",
        "",
        "",
    ];
    for (i, row) in rows.iter().enumerate() {
        let (name, st, pr, owners, so, len, prog, hours, tags, proj, done, rating) = *row;
        let mut r = tasks.new_record(&members[owners[0]].id);
        r.created_at = super::now_ms() - (rows.len() - i) as i64 * 3_600_000;
        r.updated_at = r.created_at;
        let c = &mut r.cells;
        c.insert(t_name.id.clone(), Value::Text(name.into()));
        c.insert(t_status.id.clone(), Value::Options(vec![status_opts[st].id.clone()]));
        c.insert(t_prio.id.clone(), Value::Options(vec![prio_opts[pr].id.clone()]));
        c.insert(t_owner.id.clone(), Value::Users(owners.iter().map(|o| members[*o].id.clone()).collect()));
        c.insert(t_start.id.clone(), Value::Date(today + so * day));
        c.insert(t_end.id.clone(), Value::Date(today + (so + len) * day));
        c.insert(t_progress.id.clone(), Value::Number(prog));
        c.insert(t_hours.id.clone(), Value::Number(hours));
        c.insert(t_tags.id.clone(), Value::Options(tags.iter().map(|t| tag_opts[*t].id.clone()).collect()));
        c.insert(t_project.id.clone(), Value::Links(vec![project_ids[proj].clone()]));
        c.insert(t_done.id.clone(), Value::Bool(done));
        if rating > 0 {
            c.insert(t_rating.id.clone(), Value::Number(rating as f64));
        }
        if !notes[i].is_empty() {
            c.insert(t_note.id.clone(), Value::Text(notes[i].into()));
        }
        if i % 4 == 0 {
            c.insert(
                t_url.id.clone(),
                Value::Url { text: "需求文档".into(), link: format!("https://example.com/docs/{}", i + 1) },
            );
        }
        tasks.push_record(r);
    }

    // The projects' side of the duplex link, and the lookups.
    let p_tasks = Field {
        id: p_tasks_id.clone(),
        name: "任务".into(),
        kind: FieldKind::Link { table: tasks.id.clone(), multiple: true, pair: Some(t_project.id.clone()) },
        description: String::new(),
    };
    let p_count = field("任务数", FieldKind::Lookup { link: p_tasks_id.clone(), target: t_name.id.clone(), aggregate: Aggregate::Count });
    let p_hours = field("总工时", FieldKind::Lookup { link: p_tasks_id.clone(), target: t_hours.id.clone(), aggregate: Aggregate::Sum });
    let p_avg = field(
        "平均进度",
        FieldKind::Formula { expr: "IF([任务数] = 0, 0, ROUND(AVERAGE([任务].[进度]) * 100, 0) & \"%\")".into() },
    );
    for r in projects.records_mut() {
        let ids: Vec<Id> = tasks
            .records
            .iter()
            .filter(|t| matches!(t.cells.get(&t_project.id), Some(Value::Links(l)) if l.contains(&r.id)))
            .map(|t| t.id.clone())
            .collect();
        r.cells.insert(p_tasks_id.clone(), Value::Links(ids));
    }
    projects.fields = vec![p_name, p_owner, p_stage, p_budget, p_tasks, p_count, p_hours, p_avg];

    // Views of 任务.
    let mut grid = View::new("全部任务", ViewKind::Grid);
    grid.widths = HashMap::from([(t_name.id.clone(), 200.0), (t_note.id.clone(), 220.0)]);
    grid.stats.insert(t_hours.id.clone(), Stat::Sum);
    grid.stats.insert(t_progress.id.clone(), Stat::Average);
    let mut by_owner = View::new("按状态分组", ViewKind::Grid);
    by_owner.groups = vec![SortRule { field: t_status.id.clone(), desc: false }];
    by_owner.widths = grid.widths.clone();
    let mut mine = View::new("我负责的", ViewKind::Grid);
    mine.filter = Filter {
        any: false,
        conditions: vec![Condition {
            id: new_id("cnd"),
            field: t_owner.id.clone(),
            op: Op::Contains,
            value: FilterValue::Ids(vec![me.clone()]),
        }],
    };
    let mut kanban = View::new("任务看板", ViewKind::Kanban);
    kanban.group_field = Some(t_status.id.clone());
    let mut gantt = View::new("项目排期", ViewKind::Gantt);
    gantt.start_field = Some(t_start.id.clone());
    gantt.end_field = Some(t_end.id.clone());
    let mut calendar = View::new("日历", ViewKind::Calendar);
    calendar.start_field = Some(t_start.id.clone());
    calendar.end_field = Some(t_end.id.clone());
    let mut gallery = View::new("画册", ViewKind::Gallery);
    gallery.cover_field = Some(t_files.id.clone());
    let mut form = View::new("新建任务表单", ViewKind::Form);
    form.form = Some(FormConfig {
        title: "提交新任务".into(),
        description: "填写任务信息后提交，将自动添加到「任务」表中".into(),
        fields: vec![
            (t_name.id.clone(), true),
            (t_prio.id.clone(), false),
            (t_owner.id.clone(), false),
            (t_end.id.clone(), false),
            (t_tags.id.clone(), false),
            (t_note.id.clone(), false),
        ],
        submit_text: "提交".into(),
    });
    tasks.views = vec![grid, by_owner, mine, kanban, gantt, calendar, gallery, form];

    let dashboard = Dashboard {
        id: new_id("dsh"),
        name: "项目仪表盘".into(),
        charts: vec![
            Chart {
                id: new_id("cht"),
                title: "任务总数".into(),
                kind: ChartKind::Metric,
                table: tasks.id.clone(),
                group_by: None,
                measure: Measure::Count,
                value_field: None,
                filter: Filter::default(),
                span: 1,
                rows: 1,
            },
            Chart {
                id: new_id("cht"),
                title: "总预估工时".into(),
                kind: ChartKind::Metric,
                table: tasks.id.clone(),
                group_by: None,
                measure: Measure::Sum,
                value_field: Some(t_hours.id.clone()),
                filter: Filter::default(),
                span: 1,
                rows: 1,
            },
            Chart {
                id: new_id("cht"),
                title: "已完成任务".into(),
                kind: ChartKind::Metric,
                table: tasks.id.clone(),
                group_by: None,
                measure: Measure::Count,
                value_field: None,
                filter: Filter {
                    any: false,
                    conditions: vec![Condition {
                        id: new_id("cnd"),
                        field: t_status.id.clone(),
                        op: Op::Is,
                        value: FilterValue::Ids(vec![status_opts[2].id.clone()]),
                    }],
                },
                span: 2,
                rows: 1,
            },
            Chart {
                id: new_id("cht"),
                title: "任务状态分布".into(),
                kind: ChartKind::Donut,
                table: tasks.id.clone(),
                group_by: Some(t_status.id.clone()),
                measure: Measure::Count,
                value_field: None,
                filter: Filter::default(),
                span: 2,
                rows: 3,
            },
            Chart {
                id: new_id("cht"),
                title: "各负责人工时".into(),
                kind: ChartKind::Bar,
                table: tasks.id.clone(),
                group_by: Some(t_owner.id.clone()),
                measure: Measure::Sum,
                value_field: Some(t_hours.id.clone()),
                filter: Filter::default(),
                span: 2,
                rows: 3,
            },
            Chart {
                id: new_id("cht"),
                title: "各优先级任务数".into(),
                kind: ChartKind::HBar,
                table: tasks.id.clone(),
                group_by: Some(t_prio.id.clone()),
                measure: Measure::Count,
                value_field: None,
                filter: Filter::default(),
                span: 2,
                rows: 3,
            },
            Chart {
                id: new_id("cht"),
                title: "各项目预算".into(),
                kind: ChartKind::Line,
                table: projects.id.clone(),
                group_by: Some(projects.fields[0].id.clone()),
                measure: Measure::Sum,
                value_field: Some(projects.fields[3].id.clone()),
                filter: Filter::default(),
                span: 2,
                rows: 3,
            },
        ],
    };

    let order = vec![tasks.id.clone(), projects.id.clone(), dashboard.id.clone()];
    Base {
        name: "产品研发项目管理".into(),
        tables: vec![tasks, projects],
        dashboards: vec![dashboard],
        order,
        members,
        me,
        workflows: vec![],
        advanced_perm: false,
        roles: vec![],
        member_roles: HashMap::new(),
    }
}
