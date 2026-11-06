//! The gallery view (画册视图): records as cards with their cover.

use nsh_core::prelude::*;

use super::{icons, kanban::card, state::st, theme::*};
use crate::model::{Field, query};

pub fn gallery() -> impl IntoElement {
    let s = st();
    let width = signal(1000.0f32);
    scroll()
        .flex_1()
        .bg(rgb(BG_GREY))
        .on_layout(move |b: &Bounds<Points>| width.set(b.size.width.0))
        .child(dynamic(move || {
            let tid = s.cur.get();
            let search = s.search.get().to_lowercase();
            let ids: Vec<String> = s
                .with_view(|b, t, v| {
                    let fields: Vec<&Field> = t.fields.iter().collect();
                    query::records(b, t, v)
                        .into_iter()
                        .filter(|&i| search.is_empty() || query::found(b, t, &t.records[i], &fields, &search))
                        .map(|i| t.records[i].id.clone())
                        .collect()
                })
                .unwrap_or_default();
            let w = width.get();
            let per_row = ((w - 32.0 + 16.0) / (240.0 + 16.0)).floor().max(1.0);
            let card_w = ((w - 32.0 - (per_row - 1.0) * 16.0) / per_row).floor();
            let mut items: Vec<AnyElement> = ids.into_iter().map(|id| card(tid.clone(), id, card_w, 160.0)).collect();
            items.push(
                view()
                    .w(px(card_w))
                    .h(px(200.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .border_dashed()
                    .items_center()
                    .justify_center()
                    .gap(px(6.0))
                    .hover(|s| s.bg(rgb(BG)))
                    .on_press(move || super::header::add_record_button(crate::model::ViewKind::Gallery))
                    .child(icons::icon("plus", 24.0, TEXT3))
                    .child(text("添加记录").text_size(px(13.0)).text_color(rgb(TEXT3)))
                    .into_element(),
            );
            view().flex_row().flex_wrap().gap(px(16.0)).p(px(16.0)).items_start().child(items).into_element()
        }))
}
