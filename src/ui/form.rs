//! The form view (表单视图): questions made of the table's fields; a
//! submission adds a record.

use std::{collections::HashMap, rc::Rc};

use nsh_core::prelude::*;

use super::{
    icons,
    record::{Slot, value_input},
    state::st,
    theme::*,
    widgets,
};
use crate::model::{FieldKind, FormConfig, Id, Value};

pub fn form_view() -> impl IntoElement {
    let s = st();
    let draft = stored(HashMap::<Id, Value>::new());
    let draft_rev = signal(0u32);
    let submitted = signal(false);
    let editing_title = signal(false);
    let title = signal(String::new());
    let desc = signal(String::new());
    let config = move || s.with_view(|_, _, v| v.form.clone()).flatten().unwrap_or(FormConfig { title: String::new(), description: String::new(), fields: vec![], submit_text: "提交".into() });
    let set_config = move |f: Box<dyn FnOnce(&mut FormConfig)>| {
        s.edit_view(|v| {
            if v.form.is_none() {
                v.form = Some(FormConfig { title: String::new(), description: String::new(), fields: vec![], submit_text: "提交".into() });
            }
            f(v.form.as_mut().unwrap())
        })
    };
    let submit = move || {
        let s = st();
        let cfg = config();
        let values = draft.with(|d| d.clone());
        // Required questions must be answered.
        let missing: Vec<String> = cfg
            .fields
            .iter()
            .filter(|(f, req)| *req && values.get(f).is_none_or(|v| v.is_empty()))
            .filter_map(|(f, _)| s.with_table(|_, t| t.field(f).map(|f| f.name.clone())).flatten())
            .collect();
        if !missing.is_empty() {
            s.toast(&format!("请填写必填项：{}", missing.join("、")));
            return;
        }
        let table = s.cur.get_untracked();
        s.edit(|b| {
            let me = b.me.clone();
            let Some(t) = b.table_mut(&table) else { return };
            let mut r = t.new_record(&me);
            for (f, v) in values {
                r.cells.insert(f, v);
            }
            let id = r.id.clone();
            t.records.push(r);
            crate::ui::automations::on_add(b, &table, &id);
        });
        draft.with(|d| d.clear());
        draft_rev.update(|r| *r += 1);
        submitted.set(true);
    };

    view()
        .flex_1()
        .flex_row()
        .bg(rgb(BG_GREY))
        .child(
            // The questions: which fields, and which are required.
            view()
                .w(px(280.0))
                .bg(rgb(BG))
                .border_r(px(1.0))
                .border_color(rgb(BORDER))
                .child(view().px(px(16.0)).py(px(12.0)).child(text("表单字段").text_size(px(14.0)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT))))
                .child(scroll().flex_1().child(dynamic(move || {
                    let cfg = config();
                    let fields = s.with_table(|_, t| t.fields.iter().filter(|f| !f.kind.is_computed()).map(|f| (f.id.clone(), f.name.clone(), f.kind.clone())).collect::<Vec<_>>()).unwrap_or_default();
                    fields
                        .into_iter()
                        .map(|(id, name, kind)| {
                            let on = cfg.fields.iter().any(|(f, _)| *f == id);
                            let req = cfg.fields.iter().any(|(f, r)| *f == id && *r);
                            let id2 = id.clone();
                            view()
                                .h(px(38.0))
                                .px(px(16.0))
                                .flex_row()
                                .items_center()
                                .gap(px(8.0))
                                .hover(|s| s.bg(rgb(HOVER)))
                                .child(icons::icon(icons::field_icon(&kind), 16.0, TEXT2))
                                .child(text(name).flex_1().text_size(px(14.0)).text_color(rgb(if on { TEXT } else { TEXT3 })).number_of_lines(1u32))
                                .child(when(move || on, move || {
                                    let id = id2.clone();
                                    view()
                                        .px(px(6.0))
                                        .py(px(2.0))
                                        .rounded(px(4.0))
                                        .bg(if req { rgb(0xFDE2E2) } else { rgb(BG_GREY) })
                                        .on_press(move || {
                                            let id = id.clone();
                                            set_config(Box::new(move |c| {
                                                if let Some(x) = c.fields.iter_mut().find(|(f, _)| *f == id) {
                                                    x.1 = !x.1;
                                                }
                                            }))
                                        })
                                        .child(text("必填").text_size(px(11.0)).text_color(rgb(if req { DANGER } else { TEXT3 })))
                                }))
                                .child(widgets::switch(move || on, move || {
                                    let id = id.clone();
                                    set_config(Box::new(move |c| {
                                        if let Some(p) = c.fields.iter().position(|(f, _)| *f == id) {
                                            c.fields.remove(p);
                                        } else {
                                            c.fields.push((id, false));
                                        }
                                    }))
                                }))
                                .into_element()
                        })
                        .collect::<Vec<_>>()
                }))),
        )
        .child(
            scroll().flex_1().child(
                view().items_center().py(px(32.0)).child(
                    view()
                        .w(px(640.0))
                        .bg(rgb(BG))
                        .rounded(px(12.0))
                        .shadow_sm()
                        .overflow_hidden()
                        .child(view().h(px(8.0)).bg(rgb(PRIMARY)))
                        .child(dynamic(move || {
                            if submitted.get() {
                                return view()
                                    .p(px(48.0))
                                    .items_center()
                                    .gap(px(16.0))
                                    .child(view().size(px(56.0)).rounded(px(28.0)).bg(rgb(0xD9F5D6)).items_center().justify_center().child(icons::icon("check", 28.0, SUCCESS)))
                                    .child(text("提交成功").text_size(px(20.0)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT)))
                                    .child(text("你的回答已记录到数据表中").text_size(px(14.0)).text_color(rgb(TEXT2)))
                                    .child(widgets::button("再填一份", move || submitted.set(false)))
                                    .into_element();
                            }
                            let cfg = config();
                            let tname = s.with_table(|_, t| t.name.clone()).unwrap_or_default();
                            let shown_title = if cfg.title.is_empty() { tname } else { cfg.title.clone() };
                            let questions: Vec<(Id, String, FieldKind, String, bool)> = cfg
                                .fields
                                .iter()
                                .filter_map(|(f, req)| s.with_table(|_, t| t.field(f).map(|ff| (ff.id.clone(), ff.name.clone(), ff.kind.clone(), ff.description.clone(), *req))).flatten())
                                .collect();
                            let table = s.cur.get_untracked();
                            view()
                                .p(px(32.0))
                                .gap(px(20.0))
                                .child(dynamic(move || {
                                    if editing_title.get() {
                                        view()
                                            .gap(px(8.0))
                                            .child(widgets::text_field(title, "表单标题").h(px(40.0)).text_size(px(20.0)))
                                            .child(widgets::text_field(desc, "表单描述"))
                                            .child(view().flex_row().gap(px(8.0)).child(widgets::primary_button("保存", move || {
                                                let (t, d) = (title.get_untracked(), desc.get_untracked());
                                                set_config(Box::new(move |c| {
                                                    c.title = t;
                                                    c.description = d;
                                                }));
                                                editing_title.set(false);
                                            })).child(widgets::button("取消", move || editing_title.set(false))))
                                            .into_element()
                                    } else {
                                        let cfg = config();
                                        let shown = shown_title.clone();
                                        view()
                                            .gap(px(8.0))
                                            .on_press(move || {
                                                title.set(shown.clone());
                                                desc.set(cfg.description.clone());
                                                editing_title.set(true);
                                            })
                                            .child(text(shown_title.clone()).text_size(px(22.0)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT)))
                                            .child(text(if config().description.is_empty() { "点击添加表单描述".to_string() } else { config().description }).text_size(px(14.0)).text_color(rgb(TEXT2)))
                                            .into_element()
                                    }
                                }))
                                .child(
                                    questions
                                        .into_iter()
                                        .enumerate()
                                        .map(|(i, (fid, name, kind, fdesc, req))| {
                                            let f2 = fid.clone();
                                            let f3 = fid.clone();
                                            let slot = Slot {
                                                get: Rc::new(move || {
                                                    let _ = draft_rev.get();
                                                    draft.with(|d| d.get(&f2).cloned())
                                                }),
                                                set: Rc::new(move |v| {
                                                    draft.with(|d| match v {
                                                        Some(v) => {
                                                            d.insert(f3.clone(), v);
                                                        }
                                                        None => {
                                                            d.remove(&f3);
                                                        }
                                                    });
                                                    draft_rev.update(|r| *r += 1);
                                                }),
                                                live: true,
                                            };
                                            view()
                                                .gap(px(8.0))
                                                .child(
                                                    view()
                                                        .flex_row()
                                                        .gap(px(4.0))
                                                        .child(text(format!("{}. {}", i + 1, name)).text_size(px(15.0)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT)))
                                                        .child(if req { text("*").text_size(px(15.0)).text_color(rgb(DANGER)).into_element() } else { view().into_element() }),
                                                )
                                                .child(if fdesc.is_empty() { view().into_element() } else { text(fdesc).text_size(px(13.0)).text_color(rgb(TEXT3)).into_element() })
                                                .child(value_input(table.clone(), fid, kind, slot))
                                                .into_element()
                                        })
                                        .collect::<Vec<_>>(),
                                )
                                .child(view().flex_row().justify_center().pt(px(8.0)).child(
                                    view()
                                        .h(px(40.0))
                                        .w(px(200.0))
                                        .rounded(px(8.0))
                                        .bg(rgb(PRIMARY))
                                        .hover(|s| s.bg(rgb(PRIMARY_HOVER)))
                                        .items_center()
                                        .justify_center()
                                        .on_press(submit)
                                        .child(text(if cfg.submit_text.is_empty() { "提交".to_string() } else { cfg.submit_text.clone() }).text_size(px(15.0)).text_color(rgb(0xFFFFFF))),
                                ))
                                .into_element()
                        })),
                ),
            ),
        )
}
