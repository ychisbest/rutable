//! The root: top bar, sidebar, the open table's views or dashboard, and the
//! popups, record card and toasts over them.

use nsh_core::prelude::*;

use super::{
    automation, dashboard, form, gallery, gantt, grid, header, kanban, calendar, popups, record, sidebar,
    state::{St, st},
    theme::*,
};
use crate::{model::ViewKind, win};

#[component]
pub fn RutableApp() -> impl IntoElement {
    let s = St::init();

    // Commands from the window thread (keys, wheel) and the automation port.
    spawn(async move {
        let rx = win::commands();
        while let Ok(cmd) = rx.recv().await {
            match cmd {
                win::Cmd::Key(k) => {
                    if let Some(g) = grid::grid() {
                        grid::keys::key(g, k);
                    }
                }
                win::Cmd::Wheel(dx, dy) => {
                    super::automation::log(format!("wheel {dx} {dy}"));
                    super::wheel::dispatch(dx, dy)
                }
                win::Cmd::Close => {
                    st().save_now();
                    win::close_now();
                }
                win::Cmd::Automation(req, tx) => {
                    let _ = tx.send(automation::handle(&req));
                }
            }
        }
    });
    // Saving and toasts run on the root's own clock.
    set_interval(std::time::Duration::from_millis(250), move || s.tick());
    // Ctrl+Z / Ctrl+Y outside the grid (sidebar, views) still undo.
    on_key_with("mod+z", KeyOptions { when: nsh_core::keymap::When::NotEditing, ..KeyOptions::default() }, move |_| st().undo());
    on_key_with("mod+y", KeyOptions { when: nsh_core::keymap::When::NotEditing, ..KeyOptions::default() }, move |_| st().redo());
    on_key_with("mod+shift+z", KeyOptions { when: nsh_core::keymap::When::NotEditing, ..KeyOptions::default() }, move |_| st().redo());
    on_key("mod+s", move |_| {
        st().save_now();
        st().toast("已保存");
    });
    on_key("mod+/", move |_| st().popup.set(Some(super::state::Popup::Shortcuts)));

    // What the main area shows changes only with these.
    let is_dash = computed(move || {
        let cur = s.cur.get();
        s.read(|b| b.dashboard(&cur).is_some())
    });
    let view_kind = computed(move || s.view_kind());

    // The window's title follows the base's name.
    effect(move || {
        let name = s.read(|b| b.name.clone());
        win::set_title(&format!("Rutable - {name}"));
    });

    view()
        .flex_1()
        .bg(rgb(BG))
        .on_layout(move |b: &Bounds<Points>| s.window.set((b.size.width.0, b.size.height.0)))
        .child(header::top_bar())
        .child(
            view()
                .flex_1()
                .flex_row()
                .child(when(move || s.sidebar.get(), sidebar::sidebar))
                .child(view().flex_1().min_w(px(0.0)).child(dynamic(move || {
                    let cur = s.cur.get();
                    if is_dash.get() {
                        return dashboard::dashboard(cur).into_element();
                    }
                    view()
                        .flex_1()
                        .child(header::view_tabs())
                        .child(dynamic(move || {
                            let _ = s.view.get();
                            match view_kind.get() {
                                Some(ViewKind::Grid) => vec![header::toolbar(ViewKind::Grid).into_element(), grid::grid_view().into_element()],
                                Some(ViewKind::Kanban) => vec![header::toolbar(ViewKind::Kanban).into_element(), kanban::kanban().into_element()],
                                Some(ViewKind::Gallery) => vec![header::toolbar(ViewKind::Gallery).into_element(), gallery::gallery().into_element()],
                                Some(ViewKind::Gantt) => vec![header::toolbar(ViewKind::Gantt).into_element(), gantt::gantt().into_element()],
                                Some(ViewKind::Calendar) => vec![header::toolbar(ViewKind::Calendar).into_element(), calendar::calendar_view().into_element()],
                                Some(ViewKind::Form) => vec![form::form_view().into_element()],
                                None => vec![view().into_element()],
                            }
                        }))
                        .into_element()
                }))),
        )
        .child(dynamic(popups::popup))
        .child(dynamic(record::record_card))
        .child(dynamic(move || {
            s.toast.get().map(|(msg, _)| {
                overlay()
                    .top(px(72.0))
                    .left(px(0.0))
                    .right(px(0.0))
                    .items_center()
                    .pointer_events(PointerEvents::BoxNone)
                    .child(
                        view()
                            .px(px(16.0))
                            .py(px(10.0))
                            .rounded(px(8.0))
                            .bg(rgb(0xFFFFFF))
                            .border_1()
                            .border_color(rgb(BORDER))
                            .shadow_lg()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .child(super::icons::icon("info", 16.0, PRIMARY))
                            .child(text(msg).text_size(px(14.0)).text_color(rgb(TEXT))),
                    )
            })
        }))
}
