//! Window lifecycle helpers shared across modules.

use std::sync::Arc;

use crate::app::App;
use crate::window::BrowserWindow;

/// Open a new browser window (optionally at a URL).
pub fn open_new_window(app: &Arc<App>, url: Option<&str>) -> Option<&'static BrowserWindow> {
    // Windows are leaked on purpose: GTK owns the widgets, and all closures
    // capture `&'static BrowserWindow` (see BrowserWindow::static_ref invariant).
    let win: &'static BrowserWindow = Box::leak(Box::new(BrowserWindow::new(app)));
    win.wire();
    crate::menu::register_window_actions(win);
    crate::shortcuts::register_shortcuts(win);
    app.register_window(win);
    if let Some(u) = url {
        win.new_tab_url(u);
    }
    win.present();
    Some(win)
}

/// Open (or reuse) the main window during activation.
pub fn activate(app: &Arc<App>) {
    if let Some(w) = app.any_window() {
        w.present();
        return;
    }
    let prefs = app.data.prefs.get();
    let restored = if prefs.startup_restore_session {
        app.load_session()
    } else {
        None
    };
    let win = match open_new_window(app, None) {
        Some(w) => w,
        None => return,
    };
    if let Some(state) = restored {
        if let Some(windows) = state.get("windows").and_then(|w| w.as_array()) {
            if let Some(first) = windows.first() {
                win.restore_from_state(first);
                for extra in windows.iter().skip(1) {
                    open_new_window(app, None).map(|w| w.restore_from_state(extra));
                }
                return;
            }
        }
    }
    // no session: home page
    if prefs.home_page.is_empty() || prefs.home_page == "peregrine://newtab" {
        win.new_tab_home();
    } else {
        win.new_tab_url(&prefs.home_page);
    }
}
