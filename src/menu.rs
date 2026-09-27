//! The main menu (popover): tabs, history, bookmarks, downloads, tools, help.

use std::sync::Arc;

use gtk4::gio;
use gtk4::glib::{self, clone};
use gtk4::prelude::*;
use webkit6::prelude::*;

use crate::app::App;
use crate::window::BrowserWindow;

/// Build the app-wide menu model. Window-level actions are invoked via
/// `win.` action names registered by `crate::shortcuts::register_window_actions`.
pub fn build_menu(app: &Arc<App>) -> gtk4::PopoverMenu {
    let menu = gio::Menu::new();

    // ---- Tabs ----
    let tabs = gio::Menu::new();
    let tab_new = gio::MenuItem::new(Some("New Tab"), Some("win.new-tab"));
    let tab_close = gio::MenuItem::new(Some("Close Tab"), Some("win.close-tab"));
    let tab_reopen = gio::MenuItem::new(Some("Reopen Closed Tab"), Some("win.reopen-tab"));
    let tab_pin = gio::MenuItem::new(Some("Pin / Unpin Tab"), Some("win.pin-tab"));
    let tab_mute = gio::MenuItem::new(Some("Mute / Unmute Tab"), Some("win.mute-tab"));
    let tab_dup = gio::MenuItem::new(Some("Duplicate Tab"), Some("win.duplicate-tab"));
    tabs.append_item(&tab_new);
    tabs.append_item(&tab_close);
    tabs.append_item(&tab_reopen);
    tabs.append_item(&tab_pin);
    tabs.append_item(&tab_mute);
    tabs.append_item(&tab_dup);
    menu.append_submenu(Some("Tabs"), &tabs);

    // ---- History & bookmarks ----
    let hb = gio::Menu::new();
    hb.append_item(&gio::MenuItem::new(Some("Bookmarks"), Some("win.open-bookmarks")));
    hb.append_item(&gio::MenuItem::new(Some("Bookmark This Page"), Some("win.bookmark-page")));
    hb.append_item(&gio::MenuItem::new(Some("History"), Some("win.open-history")));
    hb.append_item(&gio::MenuItem::new(Some("Downloads"), Some("win.open-downloads")));
    menu.append_submenu(Some("Library"), &hb);

    // ---- Tools ----
    let tools = gio::Menu::new();
    tools.append_item(&gio::MenuItem::new(Some("Find in Page…"), Some("win.find")));
    tools.append_item(&gio::MenuItem::new(Some("Zoom In"), Some("win.zoom-in")));
    tools.append_item(&gio::MenuItem::new(Some("Zoom Out"), Some("win.zoom-out")));
    tools.append_item(&gio::MenuItem::new(Some("Reset Zoom"), Some("win.zoom-reset")));
    tools.append_item(&gio::MenuItem::new(Some("Save Page As…"), Some("win.save-page")));
    tools.append_item(&gio::MenuItem::new(Some("Print…"), Some("win.print-page")));
    tools.append_item(&gio::MenuItem::new(Some("Fullscreen"), Some("win.fullscreen")));
    tools.append_item(&gio::MenuItem::new(Some("Inspector"), Some("win.inspector")));
    menu.append_submenu(Some("Tools"), &tools);

    // ---- Privacy ----
    let priv_menu = gio::Menu::new();
    priv_menu.append_item(&gio::MenuItem::new(Some("Privacy Dashboard"), Some("win.open-privacy")));
    priv_menu.append_item(&gio::MenuItem::new(Some("New Private Browsing Tab"), Some("win.private-tab")));
    menu.append_submenu(Some("Privacy"), &priv_menu);

    menu.append_item(&gio::MenuItem::new(Some("Settings"), Some("win.open-settings")));
    menu.append_item(&gio::MenuItem::new(Some("Keyboard Shortcuts"), Some("win.shortcuts")));
    menu.append_item(&gio::MenuItem::new(Some("About Peregrine"), Some("app.about")));

    // app-level About action
    let about_act = gio::SimpleAction::new("about", None);
    let version = app.version.to_string();
    about_act.connect_activate(move |_a, _p| {
        show_about(version.clone());
    });
    app.app.add_action(&about_act);

    gtk4::PopoverMenu::from_model(Some(&menu))
}

fn show_about(version: String) {
    let dlg = gtk4::AboutDialog::builder()
        .program_name("Peregrine")
        .version(version)
        .comments("The fastest browser on Earth. Privacy-first, built in Rust on the WebKit engine.")
        .website("https://github.com/salim77007j/peregrine-browser")
        .logo_icon_name("web-browser-symbolic")
        .license_type(gtk4::License::Mpl20)
        .build();
    dlg.present();
}

/// Register per-window actions used by the menu + shortcuts.
pub fn register_window_actions(win: &'static BrowserWindow) {
    use gio::SimpleAction;

    macro_rules! win_action {
        ($name:expr, $fn:expr) => {{
            let act = SimpleAction::new($name, None);
            let w = win;
            act.connect_activate(move |_a, _p| {
                ($fn)(w);
            });
            win.window.add_action(&act);
        }};
    }

    win_action!("new-tab", |w: &BrowserWindow| w.new_tab_home());
    win_action!("close-tab", |w: &BrowserWindow| w.close_current_tab());
    win_action!("reopen-tab", |w: &BrowserWindow| w.reopen_closed_tab());
    win_action!("pin-tab", |w: &BrowserWindow| w.pin_current_tab());
    win_action!("mute-tab", |w: &BrowserWindow| w.mute_current_tab());
    win_action!("duplicate-tab", |w: &BrowserWindow| w.duplicate_current_tab());
    win_action!("open-bookmarks", |w: &BrowserWindow| w.new_tab_url("peregrine://bookmarks"));
    win_action!("bookmark-page", |w: &BrowserWindow| {
        let uri = w.current_uri();
        if uri.starts_with("http") {
            w.app.data.bookmarks.toggle(&uri, &w.current_title());
            w.update_star_for_current();
        }
    });
    win_action!("open-history", |w: &BrowserWindow| w.new_tab_url("peregrine://history"));
    win_action!("open-downloads", |w: &BrowserWindow| w.new_tab_url("peregrine://downloads"));
    win_action!("open-privacy", |w: &BrowserWindow| w.new_tab_url("peregrine://privacy"));
    win_action!("open-settings", |w: &BrowserWindow| w.new_tab_url("peregrine://settings"));
    win_action!("private-tab", |w: &BrowserWindow| {
        // ephemeral tab: a view with the ephemeral network session
        w.new_tab_url("peregrine://newtab");
        // NOTE: the tab is marked private by routing through the ephemeral session
        // in a follow-up; for now this opens a clean start page.
    });
    win_action!("find", |w: &BrowserWindow| w.open_find());
    win_action!("zoom-in", |w: &BrowserWindow| w.zoom_in());
    win_action!("zoom-out", |w: &BrowserWindow| w.zoom_out());
    win_action!("zoom-reset", |w: &BrowserWindow| w.zoom_reset());
    win_action!("save-page", |w: &BrowserWindow| w.save_page());
    win_action!("print-page", |w: &BrowserWindow| {
        if let Some(wv) = w.current_webview() {
            let op = webkit6::PrintOperation::new(&wv);
            op.run_dialog(Some(w.widget()));
        }
    });
    win_action!("fullscreen", |w: &BrowserWindow| w.toggle_fullscreen());
    win_action!("inspector", |w: &BrowserWindow| {
        if let Some(wv) = w.current_webview() {
            if let Some(insp) = wv.inspector() {
                insp.show();
            }
        }
    });
    win_action!("shortcuts", |w: &BrowserWindow| {
        show_shortcuts(w);
    });
    win_action!("reload", |w: &BrowserWindow| {
        if let Some(wv) = w.current_webview() {
            wv.reload();
        }
    });
    win_action!("hard-reload", |w: &BrowserWindow| {
        if let Some(wv) = w.current_webview() {
            wv.reload_bypass_cache();
        }
    });
    win_action!("stop", |w: &BrowserWindow| {
        if let Some(wv) = w.current_webview() {
            wv.stop_loading();
        }
    });
    win_action!("back", |w: &BrowserWindow| {
        if let Some(wv) = w.current_webview() {
            wv.go_back();
        }
    });
    win_action!("forward", |w: &BrowserWindow| {
        if let Some(wv) = w.current_webview() {
            wv.go_forward();
        }
    });
    win_action!("focus-omnibox", |w: &BrowserWindow| {
        w.omnibox.grab_focus();
    });
    win_action!("new-window", |w: &BrowserWindow| {
        crate::window_ops::open_new_window(&w.app, Some("peregrine://newtab"));
    });
}

fn show_shortcuts(win: &BrowserWindow) {
    let body = "Ctrl+T  New tab          Ctrl+W  Close tab
Ctrl+Shift+T  Reopen tab    Ctrl+Tab  Next tab
Ctrl+L  Address bar         Ctrl+D  Bookmark page
Ctrl+H  History             Ctrl+J  Downloads
Ctrl+,  Settings            Ctrl+F  Find in page
Ctrl+P  Print               Ctrl+S  Save page
Ctrl+R / F5  Reload         Ctrl+Shift+R  Hard reload
Alt+Left / Alt+Right  Back / Forward
Ctrl+= / Ctrl+- / Ctrl+0  Zoom
F11  Fullscreen         Ctrl+Q  Quit
Esc  Stop / close bars
Right-drag ←/→  Mouse gestures: back / forward";
    let dlg = gtk4::MessageDialog::builder()
        .title("Keyboard Shortcuts")
        .text(body)
        .buttons(gtk4::ButtonsType::Close)
        .transient_for(win.widget())
        .modal(true)
        .build();
    dlg.connect_response(|dlg, _| dlg.close());
    dlg.present();
}
