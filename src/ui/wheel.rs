//! Where the wheel goes: the canvas view on screen (the grid, the gantt
//! chart) registers its handler; the window thread sends the turns over
//! its box.

use std::{cell::RefCell, rc::Rc};

thread_local! {
    static HANDLER: RefCell<Option<(u64, Rc<dyn Fn(f32, f32)>)>> = const { RefCell::new(None) };
    static NEXT: std::cell::Cell<u64> = const { std::cell::Cell::new(1) };
}

/// Makes `f` the wheel's handler until the calling component goes.
pub fn take(f: impl Fn(f32, f32) + 'static) {
    let id = NEXT.with(|n| {
        let id = n.get();
        n.set(id + 1);
        id
    });
    HANDLER.with(|h| *h.borrow_mut() = Some((id, Rc::new(f))));
    nsh_core::prelude::cleanup(move || {
        // A view mounted since keeps its handler.
        let mine = HANDLER.with(|h| h.borrow().as_ref().is_some_and(|(i, _)| *i == id));
        if mine {
            HANDLER.with(|h| *h.borrow_mut() = None);
            crate::win::set_grid_wheel(false);
        }
    });
}

pub fn dispatch(dx: f32, dy: f32) {
    let h = HANDLER.with(|h| h.borrow().as_ref().map(|(_, f)| f.clone()));
    if let Some(h) = h {
        h(dx, dy);
    }
}
