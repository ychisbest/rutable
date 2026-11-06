//! Where the wheel goes: the canvas view on screen (the grid, the gantt
//! chart) registers its handler; the window thread sends the turns over
//! its box.

use std::{cell::RefCell, rc::Rc};

thread_local! {
    static HANDLER: RefCell<Option<Rc<dyn Fn(f32, f32)>>> = const { RefCell::new(None) };
}

/// Makes `f` the wheel's handler until the calling component goes.
pub fn take(f: impl Fn(f32, f32) + 'static) {
    HANDLER.with(|h| *h.borrow_mut() = Some(Rc::new(f)));
    nsh_core::prelude::cleanup(|| {
        HANDLER.with(|h| *h.borrow_mut() = None);
        crate::win::set_grid_wheel(false);
    });
}

pub fn dispatch(dx: f32, dy: f32) {
    let h = HANDLER.with(|h| h.borrow().clone());
    if let Some(h) = h {
        h(dx, dy);
    }
}
