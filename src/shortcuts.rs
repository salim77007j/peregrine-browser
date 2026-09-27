//! Keyboard shortcuts (GTK4 ShortcutController) — every action from the menu is
//! reachable via the keyboard.

use gtk4::glib::{self, clone};
use gtk4::prelude::*;

use crate::window::BrowserWindow;

pub fn register_shortcuts(win: &'static BrowserWindow) {
    let controller = gtk4::ShortcutController::new();

    // (accelerator, action name)
    const ACCELS: &[(&str, &str)] = &[
        ("<Control>t", "win.new-tab"),
        ("<Control>w", "win.close-tab"),
        ("<Control><Shift>t", "win.reopen-tab"),
        ("<Control>m", "win.mute-tab"),
        ("<Control>k", "win.duplicate-tab"),
        ("<Control>l", "win.focus-omnibox"),
        ("<Control>d", "win.bookmark-page"),
        ("<Control>h", "win.open-history"),
        ("<Control>b", "win.open-bookmarks"),
        ("<Control>j", "win.open-downloads"),
        ("<Control>comma", "win.open-settings"),
        ("<Control>f", "win.find"),
        ("<Control>p", "win.print-page"),
        ("<Control>s", "win.save-page"),
        ("<Control>r", "win.reload"),
        ("F5", "win.reload"),
        ("<Control><Shift>r", "win.hard-reload"),
        ("Escape", "win.stop"),
        ("<Alt>Left", "win.back"),
        ("<Alt>Right", "win.forward"),
        ("<Control>plus", "win.zoom-in"),
        ("<Control>equal", "win.zoom-in"),
        ("<Control>minus", "win.zoom-out"),
        ("<Control>0", "win.zoom-reset"),
        ("F11", "win.fullscreen"),
        ("<Control><Shift>i", "win.inspector"),
        ("<Control>question", "win.shortcuts"),
        ("<Control>n", "win.new-window"),
    ];

    for (accel, action) in ACCELS {
        let action_str = format!("action('{action}')");
        if let (Some(trigger), Some(act)) = (
            gtk4::ShortcutTrigger::parse_string(accel),
            gtk4::ShortcutAction::parse_string(&action_str),
        ) {
            let shortcut = gtk4::Shortcut::builder().trigger(&trigger).action(&act).build();
            controller.add_shortcut(shortcut);
        } else {
            eprintln!("peregrine: bad accel {accel} for {action}");
        }
    }

    // ---- custom closures ----

    // Ctrl+Tab / Ctrl+Shift+Tab: cycle tabs
    let next = gtk4::Shortcut::builder()
        .trigger(&gtk4::ShortcutTrigger::parse_string("<Control>Tab").unwrap())
        .action(&gtk4::CallbackAction::new(move |_w, _v| {
            win.tab_view.select_next_page();
            glib::Propagation::Stop
        }))
        .build();
    controller.add_shortcut(next);

    let prev = gtk4::Shortcut::builder()
        .trigger(&gtk4::ShortcutTrigger::parse_string("<Control><Shift>ISO_Left_Tab").unwrap())
        .action(&gtk4::CallbackAction::new(move |_w, _v| {
            win.tab_view.select_previous_page();
            glib::Propagation::Stop
        }))
        .build();
    controller.add_shortcut(prev);
    let prev2 = gtk4::Shortcut::builder()
        .trigger(&gtk4::ShortcutTrigger::parse_string("<Control><Shift>Tab").unwrap())
        .action(&gtk4::CallbackAction::new(move |_w, _v| {
            win.tab_view.select_previous_page();
            glib::Propagation::Stop
        }))
        .build();
    controller.add_shortcut(prev2);

    // Alt+Home → home page
    let home = gtk4::Shortcut::builder()
        .trigger(&gtk4::ShortcutTrigger::parse_string("<Alt>Home").unwrap())
        .action(&gtk4::CallbackAction::new(move |_w, _v| {
            let h = win.app.data.prefs.get().home_page.clone();
            let h = if h.is_empty() { "peregrine://newtab".to_string() } else { h };
            win.navigate(&h);
            glib::Propagation::Stop
        }))
        .build();
    controller.add_shortcut(home);

    // Ctrl+1..8: jump to tab N; Ctrl+9: last tab
    for n in 1..=9u32 {
        let accel = format!("<Control>{n}");
        if let Some(trigger) = gtk4::ShortcutTrigger::parse_string(&accel) {
            let sc = gtk4::Shortcut::builder()
                .trigger(&trigger)
                .action(&gtk4::CallbackAction::new(move |_w, _v| {
                    let idx = if n == 9 {
                        (win.tab_view.n_pages().max(1) - 1) as i32
                    } else {
                        (n - 1) as i32
                    };
                    let pages: Vec<libadwaita::TabPage> = {
                        let tabs = win.tabs.borrow();
                        tabs.iter().map(|t| t.page.clone()).collect()
                    };
                    if let Some(p) = pages.get(idx as usize) {
                        win.tab_view.set_selected_page(p);
                    }
                    glib::Propagation::Stop
                }))
                .build();
            controller.add_shortcut(sc);
        }
    }

    win.window.add_controller(controller);
}
