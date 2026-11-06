//! Creating a field or changing one: its title, type, settings and
//! description.

use nsh_core::prelude::*;

use super::{
    icons,
    state::{Popup, st},
    theme::*,
    widgets::{self, popover_at},
};
use crate::model::{
    Aggregate, DateFormat, Field, FieldKind, Id, KindTag, SelectOption, formula, new_id,
    value::{self, COLOR_NAMES, TAG_COLORS, strong_color, tag_color},
};

fn close() {
    st().popup.set(None);
}

pub fn field_editor(field: Option<Id>, at: Option<usize>, x: f32, y: f32) -> impl IntoElement {
    let s = st();
    let existing = field.as_ref().and_then(|f| s.with_table(|_, t| t.field(f).cloned()).flatten());
    let table_id = s.cur.get_untracked();
    let default_name = s.with_table(|_, t| t.unique_field_name("字段")).unwrap_or_else(|| "字段".into());
    let name = signal(existing.as_ref().map(|f| f.name.clone()).unwrap_or(default_name));
    let kind = signal(existing.as_ref().map(|f| f.kind.clone()).unwrap_or(FieldKind::Text));
    let desc = signal(existing.as_ref().map(|f| f.description.clone()).unwrap_or_default());
    let type_open = signal(false);
    let source = signal(existing.as_ref().and_then(|f| s.with_table(|_, t| t.option_sources.get(&f.id).cloned()).flatten()));
    let is_primary = existing.as_ref().is_some_and(|f| s.with_table(|_, t| t.primary().map(|p| p.id == f.id)).flatten().unwrap_or(false));
    let editing_id = existing.as_ref().map(|f| f.id.clone());
    let editing_id2 = editing_id.clone();
    let type_at = element_ref();

    let save = move || {
        let n = name.get_untracked().trim().to_string();
        if n.is_empty() {
            st().toast("请输入字段标题");
            return;
        }
        let s = st();
        let dup = s.with_table(|_, t| t.fields.iter().any(|f| f.name == n && Some(&f.id) != editing_id.as_ref())).unwrap_or(false);
        if dup {
            s.toast("字段标题已存在");
            return;
        }
        let k = kind.get_untracked();
        if let FieldKind::Formula { expr } = &k {
            let check = s.peek(|b| b.table(&table_id).map(|t| formula::check(b, t, expr)));
            if let Some(Err(e)) = check {
                s.toast(&format!("公式有误：{e}"));
                return;
            }
        }
        if let FieldKind::Lookup { link, target, .. } = &k {
            if link.is_empty() || target.is_empty() {
                s.toast("请选择关联字段和要引用的字段");
                return;
            }
        }
        let d = desc.get_untracked();
        let fid = match editing_id.clone() {
            Some(fid) => {
                update_field(&table_id, &fid, n, k.clone(), d);
                Some(fid)
            }
            None => {
                create_field(&table_id, n.clone(), k.clone(), d, at);
                s.with_table(|_, t| t.field_by_name(&n).map(|f| f.id.clone())).flatten()
            }
        };
        // 引用选项: the options' source.
        if let Some(fid) = fid {
            let src = if k.options().is_some() { source.get_untracked() } else { None };
            let had = s.with_table(|_, t| t.option_sources.get(&fid).cloned()).flatten();
            if src != had {
                let tid = table_id.clone();
                s.edit(|b| {
                    if let Some(t) = b.table_mut(&tid) {
                        match src {
                            Some(x) => {
                                t.option_sources.insert(fid.clone(), x);
                            }
                            None => {
                                t.option_sources.remove(&fid);
                            }
                        }
                    }
                });
            }
        }
        close();
    };

    let title = if existing.is_some() { "修改字段" } else { "新建字段" };
    popover_at(
        x,
        y,
        380.0,
        close,
        view()
            .px(px(16.0))
            .py(px(12.0))
            .gap(px(12.0))
            .child(text(title).text_size(px(14.0)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT)))
            .child(
                view().gap(px(6.0)).child(label("字段标题")).child(
                    widgets::text_field(name, "请输入字段标题").auto_focus(true).on_submit({
                        let save = save.clone();
                        move |_: &str| save()
                    }),
                ),
            )
            .child(
                view()
                    .gap(px(6.0))
                    .child(label("字段类型"))
                    .child(
                        view()
                            .element_ref(&type_at)
                            .h(px(32.0))
                            .px(px(10.0))
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(move || if type_open.get() { rgb(PRIMARY) } else { rgb(BORDER) })
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .on_press(move || {
                                if is_primary {
                                    // The primary field takes only some types.
                                }
                                type_open.update(|o| *o = !*o)
                            })
                            .child(icons::icon_dyn(move || icons::field_icon(&kind.get()), 16.0, || TEXT2))
                            .child(text(move || kind.get().tag().label().to_string()).flex_1().text_size(px(14.0)).text_color(rgb(TEXT)))
                            .child(icons::icon("chevron-down", 14.0, TEXT3)),
                    )
                    .child(when(move || type_open.get(), move || type_menu(type_at, type_open, kind, is_primary, editing_id2.clone()))),
            )
            .child(dynamic(move || settings(kind, source)))
            .child(view().gap(px(6.0)).child(label("描述")).child(widgets::text_field(desc, "添加字段描述（选填）")))
            .child(
                view()
                    .flex_row()
                    .justify_end()
                    .gap(px(8.0))
                    .child(widgets::button("取消", close))
                    .child(widgets::primary_button("确定", save)),
            ),
    )
}

fn label(s: &'static str) -> impl IntoElement {
    text(s).text_size(px(13.0)).text_color(rgb(TEXT2))
}

fn type_menu(anchor: ElementRef, open: Signal<bool>, kind: Signal<FieldKind>, primary: bool, editing: Option<Id>) -> impl IntoElement {
    let s = st();
    overlay()
        .anchor(&anchor)
        .placement(Placement::Bottom)
        .align(Align::Start)
        .offset(px(4.0))
        .margin(px(8.0))
        .on_press_outside(move |_: &Point<Points>| open.set(false))
        .on_cancel(move || open.set(false))
        .w(px(348.0))
        .bg(rgb(BG))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgb(BORDER))
        .shadow_lg()
        .child(scroll().max_h(px(420.0)).py(px(4.0)).child(
            KindTag::MENU
                .iter()
                .enumerate()
                .map(|(gi, group)| {
                    let title = ["基础字段", "高级字段", "系统字段"][gi];
                    view()
                        .child(widgets::menu_title(title))
                        .child(view().flex_row().flex_wrap().px(px(4.0)).child(
                            group
                                .iter()
                                .filter(|t| !primary || !matches!(t, KindTag::Link | KindTag::DuplexLink | KindTag::Lookup | KindTag::Attachment | KindTag::Checkbox | KindTag::User))
                                .map(|t| {
                                    let t = *t;
                                    let editing = editing.clone();
                                    view()
                                        .w(px(164.0))
                                        .h(px(34.0))
                                        .px(px(10.0))
                                        .rounded(px(6.0))
                                        .flex_row()
                                        .items_center()
                                        .gap(px(8.0))
                                        .bg(move || if kind.get().tag() == t { rgb(ACTIVE) } else { rgb(BG) })
                                        .hover(|s| s.bg(rgb(HOVER)))
                                        .on_press(move || {
                                            let cur = st().cur.get_untracked();
                                            let mut nk = s.peek(|b| t.default_kind(&b.tables, &cur));
                                            // Options carry over between single and multiple select.
                                            if let (Some(old), Some(new)) = (kind.get_untracked().options().cloned(), nk.options_mut()) {
                                                *new = old;
                                            }
                                            if matches!(nk, FieldKind::Formula { .. }) && editing.is_some() {
                                                if let FieldKind::Formula { expr } = kind.get_untracked() {
                                                    nk = FieldKind::Formula { expr };
                                                }
                                            }
                                            kind.set(nk);
                                            open.set(false);
                                        })
                                        .child(icons::icon(icons::kind_icon(t), 16.0, TEXT2))
                                        .child(text(t.label()).text_size(px(14.0)).text_color(rgb(TEXT)))
                                        .into_element()
                                })
                                .collect::<Vec<_>>(),
                        ))
                        .into_element()
                })
                .collect::<Vec<_>>(),
        ))
}

/// The settings of the chosen type.
fn settings(kind: Signal<FieldKind>, source: Signal<Option<(Id, Id)>>) -> AnyElement {
    let s = st();
    let k = kind.get();
    let set = move |f: Box<dyn FnOnce(&mut FieldKind)>| kind.update(f);
    match k {
        FieldKind::Number { precision, thousands, percent } => view()
            .gap(px(8.0))
            .child(label("数字格式"))
            .child(widgets::dropdown(
                move || ["整数", "保留 1 位小数", "保留 2 位小数", "保留 3 位小数", "保留 4 位小数"][precision.min(4) as usize].to_string(),
                || ["整数", "保留 1 位小数", "保留 2 位小数", "保留 3 位小数", "保留 4 位小数"].iter().map(|s| s.to_string()).collect(),
                move |i| set(Box::new(move |k| if let FieldKind::Number { precision, .. } = k { *precision = i as u8 })),
                348.0,
            ))
            .child(switch_row("千分位", thousands, move || set(Box::new(|k| if let FieldKind::Number { thousands, .. } = k { *thousands = !*thousands }))))
            .child(switch_row("百分比", percent, move || set(Box::new(|k| if let FieldKind::Number { percent, .. } = k { *percent = !*percent }))))
            .child(preview(value::format_number(1234.5678, &FieldKind::Number { precision, thousands, percent })))
            .into_element(),
        FieldKind::Currency { symbol, precision } => {
            let symbols = ["¥", "$", "€", "£", "HK$", "₩", "₹", "JP¥"];
            view()
                .gap(px(8.0))
                .child(label("货币类型"))
                .child(widgets::dropdown(
                    move || symbol.clone(),
                    move || symbols.iter().map(|s| s.to_string()).collect(),
                    move |i| set(Box::new(move |k| if let FieldKind::Currency { symbol, .. } = k { *symbol = symbols[i].into() })),
                    348.0,
                ))
                .child(label("小数位数"))
                .child(widgets::dropdown(
                    move || format!("{precision} 位"),
                    || (0..=4).map(|i| format!("{i} 位")).collect(),
                    move |i| set(Box::new(move |k| if let FieldKind::Currency { precision, .. } = k { *precision = i as u8 })),
                    348.0,
                ))
                .into_element()
        }
        FieldKind::Progress { color } => view()
            .gap(px(8.0))
            .child(label("颜色"))
            .child(view().flex_row().gap(px(6.0)).child(
                (0..10u8)
                    .map(|c| {
                        view()
                            .size(px(24.0))
                            .rounded(px(12.0))
                            .bg(rgb(strong_color(c)))
                            .border(px(if c == color { 3.0 } else { 0.0 }))
                            .border_color(rgb(0xDEE0E3))
                            .on_press(move || set(Box::new(move |k| if let FieldKind::Progress { color } = k { *color = c })))
                            .into_element()
                    })
                    .collect::<Vec<_>>(),
            ))
            .into_element(),
        FieldKind::Rating { max, .. } => view()
            .gap(px(8.0))
            .child(label("最大值"))
            .child(widgets::dropdown(
                move || format!("{max}"),
                || (1..=10).map(|i| i.to_string()).collect(),
                move |i| set(Box::new(move |k| if let FieldKind::Rating { max, .. } = k { *max = i as u8 + 1 })),
                348.0,
            ))
            .into_element(),
        FieldKind::SingleSelect { options } | FieldKind::MultiSelect { options } => {
            // Fields of other tables an option list can come from.
            let cur = s.cur.get_untracked();
            let choices: Vec<((Id, Id), String)> = s.peek(|b| {
                b.tables
                    .iter()
                    .filter(|t| t.id != cur)
                    .flat_map(|t| t.fields.iter().map(move |f| ((t.id.clone(), f.id.clone()), format!("{} · {}", t.name, f.name))))
                    .collect()
            });
            let c2 = choices.clone();
            let c3 = choices.clone();
            view()
                .gap(px(8.0))
                .child(label("选项来源"))
                .child(widgets::dropdown(
                    move || match source.get() {
                        None => "自定义选项".to_string(),
                        Some(x) => c2.iter().find(|c| c.0 == x).map(|c| format!("引用 {}", c.1)).unwrap_or("自定义选项".into()),
                    },
                    move || std::iter::once("自定义选项".to_string()).chain(c3.iter().map(|c| format!("引用 {}", c.1))).collect(),
                    move |i| source.set(if i == 0 { None } else { choices.get(i - 1).map(|c| c.0.clone()) }),
                    348.0,
                ))
                .child(dynamic(move || {
                    if source.get().is_some() {
                        preview("选项与引用字段的值保持同步，引用表中新增的值会自动成为选项".into()).into_element()
                    } else {
                        options_editor(kind, options.clone())
                    }
                }))
                .into_element()
        }
        FieldKind::Date { format, time } | FieldKind::CreatedTime { format, time } | FieldKind::ModifiedTime { format, time } => view()
            .gap(px(8.0))
            .child(label("日期格式"))
            .child(widgets::dropdown(
                move || format.label().to_string(),
                || DateFormat::ALL.iter().map(|f| f.label().to_string()).collect(),
                move |i| {
                    set(Box::new(move |k| match k {
                        FieldKind::Date { format, .. } | FieldKind::CreatedTime { format, .. } | FieldKind::ModifiedTime { format, .. } => *format = DateFormat::ALL[i],
                        _ => {}
                    }))
                },
                348.0,
            ))
            .child(switch_row("包含时间", time, move || {
                set(Box::new(|k| match k {
                    FieldKind::Date { time, .. } | FieldKind::CreatedTime { time, .. } | FieldKind::ModifiedTime { time, .. } => *time = !*time,
                    _ => {}
                }))
            }))
            .into_element(),
        FieldKind::User { multiple } => switch_row("允许添加多个成员", multiple, move || set(Box::new(|k| if let FieldKind::User { multiple } = k { *multiple = !*multiple }))).into_element(),
        FieldKind::Link { table, multiple, pair } => {
            let tables: Vec<(Id, String)> = s.peek(|b| b.tables.iter().map(|t| (t.id.clone(), t.name.clone())).collect());
            let t2 = tables.clone();
            let cur_name = tables.iter().find(|t| t.0 == table).map(|t| t.1.clone()).unwrap_or("选择数据表".into());
            view()
                .gap(px(8.0))
                .child(label("关联数据表"))
                .child(widgets::dropdown(
                    move || cur_name.clone(),
                    move || tables.iter().map(|t| t.1.clone()).collect(),
                    move |i| {
                        let id = t2[i].0.clone();
                        set(Box::new(move |k| if let FieldKind::Link { table, .. } = k { *table = id }))
                    },
                    348.0,
                ))
                .child(switch_row("允许添加多条记录", multiple, move || set(Box::new(|k| if let FieldKind::Link { multiple, .. } = k { *multiple = !*multiple }))))
                .child(preview(if pair.is_some() { "双向关联：对方数据表中会同时生成一个关联字段".into() } else { "单向关联：只在当前数据表中显示关联记录".into() }))
                .into_element()
        }
        FieldKind::Lookup { link, target, aggregate } => {
            let cur = s.cur.get_untracked();
            let links: Vec<(Id, String, Id)> = s.peek(|b| {
                b.table(&cur)
                    .map(|t| {
                        t.fields
                            .iter()
                            .filter_map(|f| match &f.kind {
                                FieldKind::Link { table, .. } => Some((f.id.clone(), f.name.clone(), table.clone())),
                                _ => None,
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            });
            let other = links.iter().find(|l| l.0 == link).map(|l| l.2.clone());
            let targets: Vec<(Id, String)> = other
                .and_then(|o| s.peek(|b| b.table(&o).map(|t| t.fields.iter().map(|f| (f.id.clone(), f.name.clone())).collect())))
                .unwrap_or_default();
            let l2 = links.clone();
            let t2 = targets.clone();
            let link_name = links.iter().find(|l| l.0 == link).map(|l| l.1.clone()).unwrap_or("选择关联字段".into());
            let target_name = targets.iter().find(|l| l.0 == target).map(|l| l.1.clone()).unwrap_or("选择引用字段".into());
            view()
                .gap(px(8.0))
                .child(label("查找引用：从关联字段中引用数据"))
                .child(if links.is_empty() {
                    preview("当前数据表没有关联字段，请先创建单向或双向关联字段".into()).into_element()
                } else {
                    view()
                        .gap(px(8.0))
                        .child(widgets::dropdown(
                            move || link_name.clone(),
                            move || links.iter().map(|l| l.1.clone()).collect(),
                            move |i| {
                                let id = l2[i].0.clone();
                                set(Box::new(move |k| {
                                    if let FieldKind::Lookup { link, target, .. } = k {
                                        *link = id;
                                        target.clear();
                                    }
                                }))
                            },
                            348.0,
                        ))
                        .child(widgets::dropdown(
                            move || target_name.clone(),
                            move || targets.iter().map(|l| l.1.clone()).collect(),
                            move |i| {
                                let id = t2[i].0.clone();
                                set(Box::new(move |k| if let FieldKind::Lookup { target, .. } = k { *target = id }))
                            },
                            348.0,
                        ))
                        .child(label("汇总方式"))
                        .child(widgets::dropdown(
                            move || aggregate.label().to_string(),
                            || Aggregate::ALL.iter().map(|a| a.label().to_string()).collect(),
                            move |i| set(Box::new(move |k| if let FieldKind::Lookup { aggregate, .. } = k { *aggregate = Aggregate::ALL[i] })),
                            348.0,
                        ))
                        .into_element()
                })
                .into_element()
        }
        FieldKind::Formula { expr } => formula_editor(kind, expr),
        FieldKind::Button { label: blabel, color, action } => button_settings(kind, blabel, color, action),
        FieldKind::AutoNumber => preview("新增记录时自动生成递增编号".into()).into_element(),
        FieldKind::CreatedBy | FieldKind::ModifiedBy => preview("自动记录创建人 / 最后修改人".into()).into_element(),
        _ => view().into_element(),
    }
}

fn button_settings(kind: Signal<FieldKind>, blabel: String, color: u8, action: crate::model::Action) -> AnyElement {
    use crate::model::Action;
    let s = st();
    let label_sig = signal(blabel);
    let set = move |f: Box<dyn FnOnce(&mut String, &mut u8, &mut Action)>| {
        kind.update(|k| {
            if let FieldKind::Button { label, color, action } = k {
                f(label, color, action)
            }
        })
    };
    let kind_idx = match &action {
        Action::SetField { .. } => 0,
        Action::OpenUrl { .. } => 1,
        Action::Notify { .. } => 2,
    };
    let text0 = match &action {
        Action::OpenUrl { url } => url.clone(),
        Action::Notify { text } => text.clone(),
        Action::SetField { value, .. } => match value {
            crate::model::Value::Text(t) => t.clone(),
            v => {
                let cur = s.cur.get_untracked();
                s.peek(|b| b.table(&cur).map(|t| value::display(b, t, &FieldKind::Text, v))).unwrap_or_default()
            }
        },
    };
    let arg = signal(text0);
    let target = match &action {
        Action::SetField { field, .. } => Some(field.clone()),
        _ => None,
    };
    let cur = s.cur.get_untracked();
    let fields: Vec<(Id, String, FieldKind)> = s.peek(|b| {
        b.table(&cur)
            .map(|t| t.fields.iter().filter(|f| !f.kind.is_computed()).map(|f| (f.id.clone(), f.name.clone(), f.kind.clone())).collect())
            .unwrap_or_default()
    });
    let f2 = fields.clone();
    let f3 = fields.clone();
    // The action's value, parsed for the target field.
    let apply = move |idx: usize, target: Option<Id>, txt: String| {
        let fields = f3.clone();
        let cur = st().cur.get_untracked();
        let act = match idx {
            1 => Action::OpenUrl { url: txt },
            2 => Action::Notify { text: txt },
            _ => {
                let fid = target.or_else(|| fields.first().map(|f| f.0.clone())).unwrap_or_default();
                let mut k = fields.iter().find(|f| f.0 == fid).map(|f| f.2.clone()).unwrap_or(FieldKind::Text);
                let v = st().peek(|b| value::parse(b, &mut k, &txt)).unwrap_or(crate::model::Value::Text(txt.clone()));
                let _ = cur;
                Action::SetField { field: fid, value: v }
            }
        };
        set(Box::new(move |_, _, a| *a = act));
    };
    let apply2 = apply.clone();
    let apply3 = apply.clone();
    let target2 = target.clone();
    let target3 = target.clone();
    view()
        .gap(px(8.0))
        .child(label("按钮文字"))
        .child(widgets::text_field(label_sig, "按钮文字").on_change(move |t: &str| {
            label_sig.set(t.to_string());
            let t = t.to_string();
            set(Box::new(move |l, _, _| *l = t));
        }))
        .child(label("颜色"))
        .child(view().flex_row().gap(px(6.0)).child(
            (0..10u8)
                .map(|c| {
                    view()
                        .size(px(22.0))
                        .rounded(px(11.0))
                        .bg(rgb(strong_color(c)))
                        .border(px(if c == color { 3.0 } else { 0.0 }))
                        .border_color(rgb(0xDEE0E3))
                        .on_press(move || set(Box::new(move |_, col, _| *col = c)))
                        .into_element()
                })
                .collect::<Vec<_>>(),
        ))
        .child(label("点击后"))
        .child(widgets::dropdown(
            move || ["修改当前记录", "打开链接", "发送通知"][kind_idx].to_string(),
            || vec!["修改当前记录".into(), "打开链接".into(), "发送通知".into()],
            move |i| apply(i, target2.clone(), arg.get_untracked()),
            348.0,
        ))
        .child(when(move || kind_idx == 0, move || {
            let f2 = f2.clone();
            let f4 = f2.clone();
            let apply = apply2.clone();
            let tname = target3.as_ref().and_then(|t| f2.iter().find(|f| &f.0 == t).map(|f| f.1.clone())).unwrap_or("选择字段".into());
            widgets::dropdown(
                move || tname.clone(),
                move || f4.iter().map(|f| f.1.clone()).collect(),
                move |i| apply(0, f2.get(i).map(|f| f.0.clone()), arg.get_untracked()),
                348.0,
            )
        }))
        .child(widgets::text_field(arg, if kind_idx == 1 { "https://example.com/{名称}" } else { "值 / 通知内容" }).on_change(move |t: &str| {
            arg.set(t.to_string());
            apply3(kind_idx, target.clone(), t.to_string());
        }))
        .into_element()
}

fn preview(t: String) -> impl IntoElement {
    view().px(px(10.0)).py(px(8.0)).rounded(px(6.0)).bg(rgb(BG_GREY)).child(text(t).text_size(px(12.0)).text_color(rgb(TEXT2)))
}

fn switch_row(label_text: &'static str, on: bool, toggle: impl Fn() + 'static) -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .child(text(label_text).flex_1().text_size(px(14.0)).text_color(rgb(TEXT)))
        .child(widgets::switch(move || on, toggle))
}

fn options_editor(kind: Signal<FieldKind>, options: Vec<SelectOption>) -> AnyElement {
    let set_opts = move |f: Box<dyn FnOnce(&mut Vec<SelectOption>)>| {
        kind.update(|k| {
            if let Some(o) = k.options_mut() {
                f(o)
            }
        })
    };
    view()
        .gap(px(6.0))
        .child(label("选项"))
        .child(
            scroll().max_h(px(220.0)).child(
                options
                    .iter()
                    .enumerate()
                    .map(|(i, o)| {
                        let (bg, _) = tag_color(o.color);
                        let name = signal(o.name.clone());
                        let color = o.color;
                        view()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .py(px(3.0))
                            .child(
                                view()
                                    .size(px(20.0))
                                    .rounded(px(10.0))
                                    .bg(rgb(bg))
                                    .border_1()
                                    .border_color(rgb(BORDER))
                                    .on_press(move || set_opts(Box::new(move |o| o[i].color = (color + 1) % TAG_COLORS.len() as u8)))
                                    .tooltip(text(COLOR_NAMES[color as usize % COLOR_NAMES.len()]).text_size(px(12.0))),
                            )
                            .child(widgets::text_field(name, "选项名称").flex_1().on_change(move |t: &str| {
                                name.set(t.to_string());
                                let t = t.to_string();
                                set_opts(Box::new(move |o| o[i].name = t));
                            }))
                            .child(
                                view()
                                    .size(px(26.0))
                                    .rounded(px(6.0))
                                    .items_center()
                                    .justify_center()
                                    .hover(|s| s.bg(rgb(HOVER)))
                                    .on_press(move || set_opts(Box::new(move |o| {
                                        o.remove(i);
                                    })))
                                    .child(icons::icon("x", 14.0, TEXT3)),
                            )
                            .into_element()
                    })
                    .collect::<Vec<_>>(),
            ),
        )
        .child(view().flex_row().child(widgets::link_button("+ 添加选项", move || {
            set_opts(Box::new(|o| {
                let n = o.len();
                o.push(SelectOption { id: new_id("opt"), name: format!("选项{}", n + 1), color: (n % TAG_COLORS.len()) as u8 });
            }))
        })))
        .into_element()
}

fn formula_editor(kind: Signal<FieldKind>, expr: String) -> AnyElement {
    let s = st();
    let text_sig = signal(expr);
    let cur = s.cur.get_untracked();
    let fields: Vec<String> = s.peek(|b| b.table(&cur).map(|t| t.fields.iter().filter(|f| !matches!(f.kind, FieldKind::Formula { .. }) || true).map(|f| f.name.clone()).collect()).unwrap_or_default());
    let check = move || {
        let e = text_sig.get();
        if e.trim().is_empty() {
            return ("请输入公式，例如 [单价] * [数量]".to_string(), TEXT3);
        }
        let cur = st().cur.get_untracked();
        match st().peek(|b| b.table(&cur).map(|t| formula::check(b, t, &e))) {
            Some(Ok(())) => {
                let sample = st().peek(|b| b.table(&cur).and_then(|t| t.records.first().map(|r| (t.clone(), r.clone()))).map(|(t, r)| {
                    formula::evaluate(b, &t, &r, &e).map(|v| value::display(b, &t, &FieldKind::Formula { expr: e.clone() }, &v)).unwrap_or_default()
                }));
                (format!("✓ 公式正确{}", sample.filter(|s| !s.is_empty()).map(|s| format!("，示例结果：{s}")).unwrap_or_default()), SUCCESS)
            }
            Some(Err(err)) => (err, DANGER),
            None => (String::new(), TEXT3),
        }
    };
    let insert = move |t: String| {
        text_sig.update(|e| e.push_str(&t));
        let e = text_sig.get_untracked();
        kind.set(FieldKind::Formula { expr: e });
    };
    view()
        .gap(px(6.0))
        .child(label("公式"))
        .child(
            nsh_core::element::input()
                .min_h(px(64.0))
                .px(px(10.0))
                .py(px(8.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(rgb(BORDER))
                .text_size(px(14.0))
                .font_family("Consolas")
                .multiline(true)
                .placeholder("[单价] * [数量]")
                .placeholder_color(rgb(TEXT3))
                .value(move || text_sig.get())
                .on_change(move |t: &str| {
                    text_sig.set(t.to_string());
                    kind.set(FieldKind::Formula { expr: t.to_string() });
                }),
        )
        .child(text(move || check().0).text_size(px(12.0)).text_color(move || rgb(check().1)))
        .child(label("字段"))
        .child(view().flex_row().flex_wrap().gap(px(4.0)).child(
            fields
                .into_iter()
                .map(|f| {
                    let f2 = f.clone();
                    view()
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded(px(4.0))
                        .bg(rgb(BG_GREY))
                        .hover(|s| s.bg(rgb(ACTIVE)))
                        .on_press(move || insert(format!("[{f2}]")))
                        .child(text(f).text_size(px(12.0)).text_color(rgb(TEXT)))
                        .into_element()
                })
                .collect::<Vec<_>>(),
        ))
        .child(label("函数"))
        .child(
            scroll().max_h(px(120.0)).child(view().flex_row().flex_wrap().gap(px(4.0)).child(
                formula::FUNCTIONS
                    .iter()
                    .map(|(name, sig, _)| {
                        let n = name.to_string();
                        view()
                            .px(px(8.0))
                            .py(px(3.0))
                            .rounded(px(4.0))
                            .bg(rgb(0xF0F4FF))
                            .hover(|s| s.bg(rgb(ACTIVE)))
                            .on_press(move || insert(format!("{n}(")))
                            .tooltip(text(*sig).text_size(px(12.0)))
                            .child(text(*name).text_size(px(12.0)).text_color(rgb(PRIMARY)))
                            .into_element()
                    })
                    .collect::<Vec<_>>(),
            )),
        )
        .into_element()
}

fn create_field(table: &str, name: String, kind: FieldKind, desc: String, at: Option<usize>) {
    let s = st();
    let vid = s.view.get_untracked();
    let fid = new_id("fld");
    let table = table.to_string();
    let fid2 = fid.clone();
    s.edit(|b| {
        let mut kind = kind;
        // A duplex link makes its other side in the linked table.
        if let FieldKind::Link { table: other, pair: Some(p), multiple } = &mut kind {
            let pair_id = new_id("fld");
            let this_name = b.table(&table).map(|t| t.name.clone()).unwrap_or_default();
            if let Some(ot) = b.table_mut(other) {
                let pname = ot.unique_field_name(&this_name);
                ot.fields.push(Field {
                    id: pair_id.clone(),
                    name: pname,
                    kind: FieldKind::Link { table: table.clone(), multiple: true, pair: Some(fid2.clone()) },
                    description: String::new(),
                });
            }
            *p = pair_id;
            let _ = multiple;
        }
        let Some(t) = b.table_mut(&table) else { return };
        let f = Field { id: fid2.clone(), name, kind, description: desc };
        t.fields.push(f);
        if let Some(at) = at {
            if let Some(v) = t.view(&vid).cloned() {
                let mut order: Vec<Id> = t.view_fields(&v).into_iter().map(|(f, _)| f.id.clone()).collect();
                order.retain(|x| *x != fid2);
                order.insert(at.min(order.len()).max(1), fid2.clone());
                if let Some(v) = t.view_mut(&vid) {
                    v.field_order = order;
                }
            }
        }
        // Forms ask for new fields too.
        for v in &mut t.views {
            if let Some(form) = &mut v.form {
                form.fields.push((fid2.clone(), false));
            }
        }
    });
    s.toast("字段已创建");
    let _ = fid;
}

fn update_field(table: &str, fid: &str, name: String, kind: FieldKind, desc: String) {
    let s = st();
    let table = table.to_string();
    s.edit(|b| {
        let snapshot = b.clone();
        let Some(old) = snapshot.table(&table).and_then(|t| t.field(fid)).cloned() else { return };
        let mut kind = kind;
        let changed_type = old.kind.tag() != kind.tag() || matches!((&old.kind, &kind), (FieldKind::Link { table: a, .. }, FieldKind::Link { table: c, .. }) if a != c);
        // A new duplex link makes its other side.
        if let FieldKind::Link { table: other, pair: Some(p), .. } = &mut kind {
            if p.is_empty() {
                let pair_id = new_id("fld");
                let this_name = snapshot.table(&table).map(|t| t.name.clone()).unwrap_or_default();
                if let Some(ot) = b.table_mut(other) {
                    let pname = ot.unique_field_name(&this_name);
                    ot.fields.push(Field {
                        id: pair_id.clone(),
                        name: pname,
                        kind: FieldKind::Link { table: table.clone(), multiple: true, pair: Some(fid.to_string()) },
                        description: String::new(),
                    });
                }
                *p = pair_id;
            }
        }
        let Some(t) = b.table_mut(&table) else { return };
        let tsnap = snapshot.table(&table).unwrap();
        if changed_type && !kind.is_computed() {
            // Values change with the type, through their text.
            let mut nk = kind.clone();
            for r in &mut t.records {
                if let Some(v) = r.cells.get(fid).cloned() {
                    match value::convert(&snapshot, tsnap, &old.kind, &mut nk, &v) {
                        Some(nv) => {
                            r.cells.insert(fid.to_string(), nv);
                        }
                        None => {
                            r.cells.remove(fid);
                        }
                    }
                }
            }
            kind = nk;
        } else if kind.is_computed() {
            for r in &mut t.records {
                r.cells.remove(fid);
            }
        }
        // Options removed from a select leave the cells.
        if let Some(opts) = kind.options() {
            let ids: Vec<Id> = opts.iter().map(|o| o.id.clone()).collect();
            for r in &mut t.records {
                if let Some(crate::model::Value::Options(v)) = r.cells.get_mut(fid) {
                    v.retain(|x| ids.contains(x));
                    if v.is_empty() {
                        r.cells.remove(fid);
                    }
                }
            }
        }
        if let Some(f) = t.field_mut(fid) {
            f.name = name;
            f.kind = kind;
            f.description = desc;
        }
    });
    s.toast("字段已更新");
}

pub fn open_new_field(x: f32, y: f32) {
    st().popup.set(Some(Popup::FieldEditor { field: None, at: None, x, y }));
}
