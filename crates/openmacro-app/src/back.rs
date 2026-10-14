//! The Android back button.
//!
//! The Android activity only sends Back to the page when the WebView has
//! history to go back through; otherwise it closes the app. The router keeps
//! its history in memory, so the WebView has none. So we push a history
//! entry for each open layer (and one when off the Dashboard). Back pops
//! one, and the resulting `popstate` closes the top-most layer: a detail
//! view, then the sheet behind it, then the tab.
//!
//! Components that should close on Back call [`use_back_handler`] while
//! they're shown.

use std::sync::atomic::{AtomicUsize, Ordering};

use dioxus::prelude::*;

/// Open layers that Back can close, oldest first.
static HANDLERS: GlobalSignal<Vec<(usize, Callback)>> = Signal::global(Vec::new);

/// Closes this component's layer on Back while it's mounted. Layers opened
/// later (shown on top) close first.
pub fn use_back_handler(handler: impl FnMut(()) + 'static) {
    let callback = use_callback(handler);
    let id = use_hook(|| {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        NEXT.fetch_add(1, Ordering::Relaxed)
    });
    use_effect(move || HANDLERS.write().push((id, callback)));
    use_drop(move || HANDLERS.write().retain(|(i, _)| *i != id));
}

/// Wires the system Back button to the open layers; `off_root` is true
/// when the current screen isn't the Dashboard. Back there returns to it.
#[cfg(target_os = "android")]
pub fn use_system_back(off_root: bool) {
    let nav = navigator();
    // History entries we've pushed: one per open layer, plus one off the
    // Dashboard. Entries are only pushed as layers open, which is always in
    // response to a tap: Chromium makes Back skip entries a page adds
    // without a user gesture, so we never re-push after a Back press.
    let mut pushed = use_signal(|| 0usize);
    use_hook(|| {
        spawn(async move {
            let mut eval = document::eval(
                "window.addEventListener('popstate', () => {
                     if (window.__openmacroIgnorePop) {
                         window.__openmacroIgnorePop = false;
                         return;
                     }
                     dioxus.send(true);
                 });
                 await new Promise(() => {});",
            );
            while eval.recv::<bool>().await.is_ok() {
                // The Back press used up one entry.
                let left = pushed.peek().saturating_sub(1);
                pushed.set(left);
                let top = HANDLERS.peek().last().map(|(_, callback)| *callback);
                match top {
                    Some(close) => close.call(()),
                    None => {
                        nav.push(crate::Route::Dashboard {});
                    }
                }
            }
        });
    });

    let off_root = use_memo(use_reactive((&off_root,), |(off_root,)| off_root));
    use_effect(move || {
        let depth = HANDLERS.read().len() + usize::from(off_root());
        let have = *pushed.peek();
        if depth > have {
            document::eval(&format!(
                "for (let i = 0; i < {}; i++) history.pushState({{ openmacro: true }}, '');",
                depth - have
            ));
            pushed.set(depth);
        } else if depth < have {
            // A layer was closed on screen: drop its entries without
            // treating that as a Back press.
            document::eval(&format!(
                "window.__openmacroIgnorePop = true; history.go(-{});",
                have - depth
            ));
            pushed.set(depth);
        }
    });
}
