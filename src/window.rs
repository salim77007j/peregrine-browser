//! The browser window: tab strip, toolbar, omnibox, bookmark bar, find bar,
//! status bar, download popover, tab lifecycle (incl. sleeping), and session state.

use std::sync::Arc;
use std::time::Instant;

use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use webkit6::prelude::*;
use webkit6::{FindOptions, PermissionRequest, WebView};

use crate::app::App;
use crate::omnibox::Omnibox;
use crate::tabs::TabState;

pub struct BrowserWindow {
    pub app: Arc<App>,
    pub window: adw::ApplicationWindow,
    pub root: gtk4::Box,
    pub tab_view: adw::TabView,
    pub tab_bar: adw::TabBar,
    pub tabs: std::cell::RefCell<Vec<TabState>>,
    pub omnibox: Omnibox,
    pub back_btn: gtk4::Button,
    pub fwd_btn: gtk4::Button,
    pub reload_btn: gtk4::Button,
    pub star_btn: gtk4::Button,
    pub find_revealer: gtk4::Revealer,
    pub find_entry: gtk4::SearchEntry,
    pub find_label: gtk4::Label,
    pub status_revealer: gtk4::Revealer,
    pub status_label: gtk4::Label,
    pub bookmark_revealer: gtk4::Revealer,
    pub download_btn: gtk4::Button,
    pub download_badge: gtk4::Label,
    pub toolbar: gtk4::Box,
    pub home_btn: gtk4::Button,
    pub find_prev_btn: gtk4::Button,
    pub find_next_btn: gtk4::Button,
    pub find_close_btn: gtk4::Button,
    pub fullscreen: std::cell::Cell<bool>,
    pub chrome_revealer: gtk4::Revealer,
    find_controller: std::cell::RefCell<Option<webkit6::FindController>>,
    sleeping_pages: std::cell::RefCell<Vec<(usize, String, String)>>, // (pos, url, title)
    closing_for_sleep: std::cell::Cell<bool>,
}

impl BrowserWindow {
    /// Re-derive the `'static` reference to this window.
    ///
    /// # Safety invariant
    /// Every `BrowserWindow` is created exclusively by `window_ops::open_new_window`,
    /// which leaks it (`Box::leak`); instances live for the whole process lifetime.
    /// Widgets and closures may therefore safely capture `&'static BrowserWindow`.
    #[inline]
    pub fn static_ref(&self) -> &'static Self {
        unsafe { &*(self as *const Self) }
    }

    pub fn new(app: &Arc<App>) -> Self {
        let window = adw::ApplicationWindow::builder()
            .application(&app.app)
            .title("Peregrine")
            .default_width(1360)
            .default_height(860)
            .build();
        window.set_icon_name(Some("web-browser-symbolic"));

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

        // ---------- tab strip (tabs in the titlebar, like modern browsers) ----------
        let strip = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let controls_start = gtk4::WindowControls::new(gtk4::PackType::Start);
        let controls_end = gtk4::WindowControls::new(gtk4::PackType::Start);
        let tab_bar = adw::TabBar::builder().css_classes(["peregrine-tabbar"]).build();
        tab_bar.set_autohide(false);
        tab_bar.set_hexpand(true);
        let new_tab_btn = gtk4::Button::builder()
            .icon_name("list-add-symbolic")
            .css_classes(["flat", "peregrine-newtab"])
            .tooltip_text("New tab (Ctrl+T)")
            .build();
        strip.append(&controls_start);
        strip.append(&tab_bar);
        strip.append(&new_tab_btn);
        strip.append(&controls_end);

        // ---------- toolbar ----------
        let toolbar = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        toolbar.set_margin_top(3);
        toolbar.set_margin_bottom(3);
        toolbar.set_margin_start(4);
        toolbar.set_margin_end(4);
        toolbar.add_css_class("peregrine-toolbar");

        let back_btn = flat_btn("go-previous-symbolic", "Back (Alt+Left)");
        let fwd_btn = flat_btn("go-next-symbolic", "Forward (Alt+Right)");
        let reload_btn = flat_btn("view-refresh-symbolic", "Reload (Ctrl+R)");
        let home_btn = flat_btn("go-home-symbolic", "Home (Alt+Home)");

        let omnibox = Omnibox::new(app.clone());

        let star_btn = flat_btn("star-outline-thin-symbolic", "Bookmark this page (Ctrl+D)");
        let download_btn = gtk4::Button::builder()
            .icon_name("document-save-symbolic")
            .css_classes(["flat"])
            .tooltip_text("Downloads (Ctrl+J)")
            .build();
        let download_badge = gtk4::Label::builder().css_classes(["peregrine-badge"]).label("").build();
        download_badge.set_visible(false);
        let badge_box_w = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        badge_box_w.add_css_class("peregrine-badge-box");
        badge_box_w.append(&download_badge);
        download_btn.set_child(Some(&badge_box_w));

        let menu_btn = gtk4::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .css_classes(["flat"])
            .tooltip_text("Menu")
            .build();
        menu_btn.set_popover(Some(&crate::menu::build_menu(app)));

        toolbar.append(&back_btn);
        toolbar.append(&fwd_btn);
        toolbar.append(&reload_btn);
        toolbar.append(&home_btn);
        toolbar.append(omnibox.widget());
        toolbar.append(&star_btn);
        toolbar.append(&download_btn);
        toolbar.append(&menu_btn);
        omnibox.widget().set_hexpand(true);

        // ---------- bookmark bar ----------
        let bookmark_revealer = gtk4::Revealer::new();
        let bbar = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
        bbar.add_css_class("peregrine-bookmarkbar");
        bbar.set_margin_start(6);
        bbar.set_margin_end(6);
        bookmark_revealer.set_child(Some(&bbar));
        bookmark_revealer.set_reveal_child(false);

        // ---------- tab view ----------
        let tab_view = adw::TabView::builder().vexpand(true).build();
        tab_bar.set_view(Some(&tab_view));

        // ---------- find bar ----------
        let find_revealer = gtk4::Revealer::new();
        let find_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        find_bar.add_css_class("peregrine-findbar");
        let find_entry = gtk4::SearchEntry::new();
        find_entry.set_placeholder_text(Some("Find in page"));
        find_entry.set_width_request(240);
        let find_label = gtk4::Label::new(Some(""));
        find_label.add_css_class("dim-label");
        let fprev = flat_btn("go-up-symbolic", "Previous match");
        let fnext = flat_btn("go-down-symbolic", "Next match");
        let fclose = flat_btn("window-close-symbolic", "Close find bar (Esc)");
        find_bar.append(&find_entry);
        find_bar.append(&find_label);
        find_bar.append(&fprev);
        find_bar.append(&fnext);
        find_bar.append(&fclose);
        let find_pad = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        find_pad.set_margin_start(6);
        find_pad.set_margin_top(6);
        find_pad.append(&find_bar);
        find_revealer.set_child(Some(&find_pad));
        find_revealer.set_valign(gtk4::Align::Start);

        // ---------- status bar ----------
        let status_revealer = gtk4::Revealer::new();
        let status_label = gtk4::Label::new(Some(""));
        status_label.set_halign(gtk4::Align::Start);
        status_label.add_css_class("peregrine-status");
        status_label.set_ellipsize(pango::EllipsizeMode::End);
        status_revealer.set_child(Some(&status_label));
        status_revealer.set_valign(gtk4::Align::End);
        status_revealer.set_reveal_child(false);

        // ---------- overlay for find/status over content ----------
        let overlay = gtk4::Overlay::new();
        overlay.set_child(Some(&tab_view));
        overlay.add_overlay(&find_revealer);
        overlay.add_overlay(&status_revealer);

        // ---------- assemble chrome (hideable in fullscreen) ----------
        let chrome = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        chrome.append(&strip);
        chrome.append(&toolbar);
        chrome.append(&bookmark_revealer);
        let chrome_revealer = gtk4::Revealer::new();
        chrome_revealer.set_child(Some(&chrome));
        chrome_revealer.set_reveal_child(true);

        root.append(&chrome_revealer);
        root.append(&overlay);
        window.set_content(Some(&root));

        Self {
            app: app.clone(),
            window,
            root,
            tab_view,
            tab_bar,
            tabs: std::cell::RefCell::new(vec![]),
            omnibox,
            back_btn,
            fwd_btn,
            reload_btn,
            star_btn,
            find_revealer,
            find_entry,
            find_label,
            status_revealer,
            status_label,
            bookmark_revealer,
            download_btn,
            download_badge,
            toolbar,
            home_btn,
            find_prev_btn: fprev,
            find_next_btn: fnext,
            find_close_btn: fclose,
            fullscreen: std::cell::Cell::new(false),
            chrome_revealer,
            find_controller: std::cell::RefCell::new(None),
            sleeping_pages: std::cell::RefCell::new(vec![]),
            closing_for_sleep: std::cell::Cell::new(false),
        }
    }

    /// Wire all signals. Must be called once after construction.
    pub fn wire(&self) {
        let app = self.app.clone();

        // ---------- tab close ----------
        self.tab_view.connect_close_page({ let win = self.static_ref(); move |view, page| {
            if win.closing_for_sleep.get() {
                view.close_page_finish(page, false);
                return glib::Propagation::Stop;
            }
            // placeholder (sleeping) tab closed by the user
            if page.child().downcast_ref::<gtk4::Label>().is_some() {
                let title = page.title().to_string();
                let mut sp = win.sleeping_pages.borrow_mut();
                if let Some(i) = sp.iter().position(|(_, _, t)| *t == title && !title.is_empty()) {
                    sp.remove(i);
                }
                if win.tab_count() == 1 {
                    win.new_tab_home();
                    return glib::Propagation::Stop;
                }
                return glib::Propagation::Proceed;
            }
            let url = page
                .child()
                .downcast_ref::<WebView>()
                .map(|w| w.uri().map(|u| u.to_string()).unwrap_or_default())
                .unwrap_or_default();
            win.app.push_closed_tab(&url);
            if win.tab_count() == 1 {
                win.new_tab_home();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        } });

        self.tab_view.connect_selected_page_notify({ let win = self.static_ref(); move |view| {
            if let Some(page) = view.selected_page() {
                // lazy-load pending (restored) tabs
                let pending = {
                    let tabs = win.tabs.borrow();
                    tabs.iter().find(|t| t.page == page).and_then(|t| t.pending.clone())
                };
                if let Some(url) = pending {
                    win.clear_pending(&page);
                    if let Some(wv) = win.webview_of(&page) {
                        wv.load_uri(&url);
                    }
                }
                win.mark_active(&page);
                win.sync_toolbar();
                win.omnibox.sync_from_view();
                win.update_star_for_current();
            }
        } });

        // ---------- buttons ----------
        self.back_btn.connect_clicked({ let win = self.static_ref(); move |_| {
            if let Some(wv) = win.current_webview() { wv.go_back(); }
        } });
        self.fwd_btn.connect_clicked({ let win = self.static_ref(); move |_| {
            if let Some(wv) = win.current_webview() { wv.go_forward(); }
        } });
        self.reload_btn.connect_clicked({ let win = self.static_ref(); move |_| {
            if let Some(wv) = win.current_webview() {
                if wv.is_loading() { wv.stop_loading(); } else { wv.reload(); }
            }
        } });
        // home button
        self.home_btn.connect_clicked({ let win = self.static_ref(); move |_| {
            let home = win.app.data.prefs.get().home_page.clone();
            let home = if home.is_empty() { "peregrine://newtab".to_string() } else { home };
            win.navigate(&home);
        } });
        self.omnibox.wire(self.static_ref());
        self.star_btn.connect_clicked({ let win = self.static_ref(); move |_| {
            let uri = win.current_uri();
            if uri.starts_with("http") {
                let title = win.current_title();
                let added = win.app.data.bookmarks.toggle(&uri, &title);
                win.update_star(added);
            }
        } });
        self.download_btn.connect_clicked({ let win = self.static_ref(); let app = app.clone(); move |btn| {
            crate::downloads::show_downloads_popover(&win, &app, btn);
        } });

        // find bar
        self.find_entry.connect_changed({ let win = self.static_ref(); move |e| {
            let t = e.text().to_string();
            if t.is_empty() {
                win.find_close();
            } else {
                win.find_start(&t);
            }
        } });
        self.find_entry.connect_activate({ let win = self.static_ref(); move |_| {
            win.find_next();
        } });
        // find bar buttons
        self.find_prev_btn.connect_clicked({ let win = self.static_ref(); move |_| {
            win.find_prev();
        } });
        self.find_next_btn.connect_clicked({ let win = self.static_ref(); move |_| {
            win.find_next();
        } });
        self.find_close_btn.connect_clicked({ let win = self.static_ref(); move |_| {
            win.find_close();
        } });

        // ---------- periodic: sleep tabs + badge refresh ----------
        glib::timeout_add_local(
            std::time::Duration::from_secs(20),
            { let win = self.static_ref(); let app = app.clone(); move || {
                win.sleep_old_tabs(&app);
                win.wake_if_selected();
                win.refresh_download_badge(&app);
                glib::ControlFlow::Continue
            } },
        );

        // ---------- window close → session save ----------
        self.window.connect_close_request({ let win = self.static_ref(); move |_| {
            win.app.unregister_window(win);
            win.app.save_session();
            glib::Propagation::Proceed
        } });

        self.refresh_appearance(&self.app.data.prefs.get());
    }

    // =====================================================================
    // Tab management
    // =====================================================================

    pub fn new_tab_home(&self) {
        self.new_tab_url("peregrine://newtab");
    }

    pub fn new_tab_url(&self, url: &str) {
        let wv = crate::webview::create_view(self, &self.app);
        let page = self.tab_view.append(&wv);
        page.set_title("New tab");
        self.tabs.borrow_mut().push(TabState {
            page: page.clone(),
            webview: wv.clone(),
            pending: None,
            last_active: Instant::now(),
            sleeping: false,
        });
        self.tab_view.set_selected_page(&page);
        wv.load_uri(url);
    }

    /// Adopt a JS/window.open target as a new (background) tab.
    pub fn tab_for_created_webview(&self, wv: &WebView) {
        let page = self.tab_view.append(wv);
        page.set_title("New tab");
        page.set_needs_attention(true);
        self.tabs.borrow_mut().push(TabState {
            page: page.clone(),
            webview: wv.clone(),
            pending: None,
            last_active: Instant::now(),
            sleeping: false,
        });
    }

    pub fn close_current_tab(&self) {
        if let Some(page) = self.tab_view.selected_page() {
            self.tab_view.close_page(&page);
        }
    }

    pub fn reopen_closed_tab(&self) {
        if let Some(url) = self.app.pop_closed_tab() {
            self.new_tab_url(&url);
        }
    }

    pub fn mute_current_tab(&self) {
        if let Some(wv) = self.current_webview() {
            wv.set_is_muted(!wv.is_muted());
            self.refresh_audio_indicators();
        }
    }

    pub fn pin_current_tab(&self) {
        if let Some(page) = self.tab_view.selected_page() {
            let pinned = page.is_pinned();
            self.tab_view.set_page_pinned(&page, !pinned);
        }
    }

    pub fn duplicate_current_tab(&self) {
        let uri = self.current_uri();
        if !uri.is_empty() {
            self.new_tab_url(&uri);
        }
    }

    fn tab_count(&self) -> i32 {
        self.tab_view.n_pages()
    }

    /// Put old hidden tabs to sleep: replaces their page with a lightweight
    /// placeholder so the WebKitWebProcess exits and RAM is freed.
    fn sleep_old_tabs(&self, app: &Arc<App>) {
        let minutes = app.data.prefs.get().tab_sleep_minutes;
        if minutes == 0 {
            return;
        }
        let cutoff = std::time::Duration::from_secs(minutes * 60);
        let selected = self.tab_view.selected_page();
        let mut to_sleep = vec![];
        {
            let tabs = self.tabs.borrow();
            for t in tabs.iter() {
                if t.sleeping || Some(&t.page) == selected.as_ref() || t.page.is_pinned() {
                    continue;
                }
                if t.webview.is_playing_audio() {
                    continue;
                }
                if t.last_active.elapsed() > cutoff {
                    to_sleep.push((
                        t.page.clone(),
                        t.webview.uri().map(|u| u.to_string()).unwrap_or_default(),
                        t.webview.title().map(|x| x.to_string()).unwrap_or_default(),
                        t.page.is_pinned(),
                    ));
                }
            }
        }
        for (page, uri, title, _pinned) in to_sleep {
            let pos = self.tab_view.page_position(&page);
            self.sleeping_pages.borrow_mut().push((pos as usize, uri, title.clone()));
            self.closing_for_sleep.set(true);
            self.tabs.borrow_mut().retain(|t| t.page != page);
            self.tab_view.close_page(&page);
            self.closing_for_sleep.set(false);
            // placeholder keeps the tab visible in the strip
            let ph = gtk4::Label::new(Some("💤"));
            ph.add_css_class("peregrine-sleeping");
            ph.set_tooltip_text(Some("Sleeping tab — select to wake"));
            let new_page = self.tab_view.append(&ph);
            new_page.set_title(if title.is_empty() { "Sleeping tab" } else { title.as_str() });
            self.tab_view.reorder_page(&new_page, pos);
        }
    }

    fn wake_if_selected(&self) {
        let selected = match self.tab_view.selected_page() {
            Some(p) => p,
            None => return,
        };
        if selected.child().downcast_ref::<gtk4::Label>().is_none() {
            return;
        }
        let title = selected.title().to_string();
        let idx = {
            let sp = self.sleeping_pages.borrow();
            sp.iter().position(|(_, _, t)| *t == title && !title.is_empty())
        };
        if let Some(i) = idx {
            let (_, uri, title) = self.sleeping_pages.borrow_mut().remove(i);
            // remove the placeholder page
            self.closing_for_sleep.set(true);
            self.tab_view.close_page(&selected);
            self.closing_for_sleep.set(false);
            glib::idle_add_local({ let win = self.static_ref(); move || {
                win.new_tab_url(&uri);
                if let Some(p) = win.tab_view.selected_page() {
                    if !title.is_empty() {
                        p.set_title(&title);
                    }
                }
                glib::ControlFlow::Break
            } });
        }
    }

    // =====================================================================
    // Navigation / state
    // =====================================================================

    pub fn navigate(&self, url: &str) {
        if let Some(wv) = self.current_webview() {
            wv.load_uri(url);
        } else {
            self.new_tab_url(url);
        }
    }

    pub fn current_webview(&self) -> Option<WebView> {
        let tabs = self.tabs.borrow();
        tabs.iter()
            .find(|t| Some(&t.page) == self.tab_view.selected_page().as_ref())
            .map(|t| t.webview.clone())
            .or_else(|| tabs.last().map(|t| t.webview.clone()))
    }

    pub fn webview_of(&self, page: &adw::TabPage) -> Option<WebView> {
        let tabs = self.tabs.borrow();
        tabs.iter().find(|t| &t.page == page).map(|t| t.webview.clone())
    }

    pub fn current_uri(&self) -> String {
        self.current_webview().and_then(|w| w.uri()).map(|u| u.to_string()).unwrap_or_default()
    }

    pub fn current_origin(&self) -> String {
        url::Url::parse(&self.current_uri())
            .map(|u| u.origin().ascii_serialization())
            .unwrap_or_default()
    }

    pub fn current_title(&self) -> String {
        self.current_webview()
            .and_then(|w| w.title().map(|t| t.to_string()))
            .unwrap_or_default()
    }

    pub fn sync_toolbar(&self) {
        let wv = self.current_webview();
        self.back_btn.set_sensitive(wv.as_ref().map(|w| w.can_go_back()).unwrap_or(false));
        self.fwd_btn.set_sensitive(wv.as_ref().map(|w| w.can_go_forward()).unwrap_or(false));
        let loading = wv.as_ref().map(|w| w.is_loading()).unwrap_or(false);
        self.reload_btn.set_icon_name(if loading { "process-stop-symbolic" } else { "view-refresh-symbolic" });
        if let Some(page) = self.tab_view.selected_page() {
            page.set_loading(loading);
        }
    }

    pub fn update_star_for_current(&self) {
        let uri = self.current_uri();
        if uri.is_empty() {
            return;
        }
        let starred = self.app.data.bookmarks.contains(&uri);
        self.update_star(starred);
    }

    pub fn update_star(&self, on: bool) {
        self.star_btn.set_icon_name(if on { "star-filled-thin-symbolic" } else { "star-outline-thin-symbolic" });
    }

    // =====================================================================
    // Events from webview wiring
    // =====================================================================

    pub fn on_uri_changed(&self, uri: &str) {
        self.omnibox.on_uri(uri);
        self.sync_toolbar();
        if uri.starts_with("http") {
            if let Some(host) = crate::util::host_of_uri(uri) {
                let zoom = self.app.site_zoom(&host);
                if let Some(wv) = self.current_webview() {
                    if (wv.zoom_level() - zoom).abs() > f64::EPSILON {
                        wv.set_zoom_level(zoom);
                    }
                }
            }
        }
    }

    pub fn on_title_changed(&self, title: &str) {
        if let Some(page) = self.tab_view.selected_page() {
            let t = if title.is_empty() { "Loading…" } else { title };
            page.set_title(&crate::util::ellipsize(t, 80));
        }
        self.window.set_title(Some(&format!(
            "{} — Peregrine",
            if title.is_empty() { "Peregrine" } else { title }
        )));
    }

    pub fn on_loading_changed(&self, loading: bool) {
        if let Some(page) = self.tab_view.selected_page() {
            page.set_loading(loading);
        }
        self.sync_toolbar();
    }

    pub fn on_favicon(&self, tex: &gtk4::gdk::Texture) {
        if let Some(page) = self.tab_view.selected_page() {
            page.set_icon(Some(tex));
        }
    }

    pub fn on_load_committed(&self, uri: &str) {
        self.omnibox.on_uri(uri);
        if uri == "peregrine://newtab" {
            self.omnibox.set_text("");
        }
    }

    pub fn on_load_finished(&self) {
        self.sync_toolbar();
        self.update_star_for_current();
    }

    pub fn on_load_failed(&self, uri: &str, err: &gtk4::glib::Error) {
        if err.code() == 302 {
            return; // cancelled
        }
        let html = format!(
            r#"<!DOCTYPE html><html><head><meta charset="utf-8"><style>
body {{ margin:0; font-family: system-ui, sans-serif; background:#14161d; color:#e8eaf0;
display:flex; align-items:center; justify-content:center; height:100vh; }}
.card {{ max-width:520px; padding:40px; text-align:center; }}
h1 {{ font-size:20px; margin:10px 0; }}
p {{ color:#9aa3b2; font-size:13.5px; line-height:1.6; }}
.url {{ font-family:monospace; color:#6b7484; font-size:12px; word-break:break-all; }}
button {{ background:#22d3ee; color:#0b0e14; border:none; padding:9px 20px; border-radius:8px;
font-weight:600; cursor:pointer; margin-top:14px; }}
</style></head><body><div class="card"><div style="font-size:44px">🦅</div>
<h1>Can't reach this page</h1><p>{msg}</p><div class="url">{uri}</div>
<button onclick="location.reload()">Try again</button></div></body></html>"#,
            msg = crate::pages::esc(&err.message().to_string()),
            uri = crate::pages::esc(uri),
        );
        if let Some(wv) = self.current_webview() {
            wv.load_html(&html, Some(uri));
        }
    }

    pub fn status_message(&self, msg: Option<String>) {
        self.status_revealer.set_reveal_child(msg.is_some());
        if let Some(m) = msg {
            self.status_label.set_text(&m);
        }
    }

    pub fn refresh_audio_indicators(&self) {
        // Mute/audio state is surfaced in the page title (the AdwTabBar indicator
        // API needs a GdkPaintable the bindings don't currently expose).
        let tabs = self.tabs.borrow();
        for t in tabs.iter() {
            let base = t
                .page
                .title()
                .to_string()
                .trim_start_matches("\u{1f507} ")
                .to_string();
            let title = if t.webview.is_muted() {
                format!("\u{1f507} {}", if base.is_empty() { "Tab" } else { &base })
            } else {
                base
            };
            if !title.is_empty() {
                t.page.set_title(&title);
            }
            let tip: &str = if t.webview.is_muted() {
                "Tab is muted — use Menu → Mute/Unmute Tab"
            } else if t.webview.is_playing_audio() {
                "Playing audio"
            } else {
                ""
            };
            t.page.set_indicator_tooltip(tip);
        }
    }

    pub fn refresh_appearance(&self, prefs: &crate::data::prefs::Prefs) {
        self.bookmark_revealer.set_reveal_child(prefs.show_bookmark_bar);
        if prefs.show_bookmark_bar {
            self.rebuild_bookmark_bar();
        }
    }

    fn rebuild_bookmark_bar(&self) {
        let Some(box_) = self.bookmark_revealer.child() else { return };
        let Some(bbar) = box_.downcast_ref::<gtk4::Box>() else { return };
        while let Some(child) = bbar.first_child() {
            bbar.remove(&child);
        }
        for bm in self.app.data.bookmarks.all().into_iter().take(30) {
            let host = crate::util::host_of_uri(&bm.url).unwrap_or_else(|| bm.url.clone());
            let title = if bm.title.is_empty() { host.clone() } else { bm.title.clone() };
            let btn = gtk4::Button::builder()
                .label(&crate::util::ellipsize(&title, 26))
                .tooltip_text(&format!("{}\n{}", bm.title, bm.url))
                .css_classes(["flat", "peregrine-bm-item"])
                .build();
            let url = bm.url.clone();
            btn.connect_clicked({ let win = self.static_ref(); move |_| {
                win.navigate(&url);
            } });
            bbar.append(&btn);
        }
    }

    // =====================================================================
    // Find in page
    // =====================================================================

    pub fn open_find(&self) {
        self.find_revealer.set_reveal_child(true);
        self.find_entry.grab_focus();
    }

    fn find_start(&self, text: &str) {
        if let Some(wv) = self.current_webview() {
            if let Some(fc) = wv.find_controller() {
                *self.find_controller.borrow_mut() = Some(fc.clone());
                fc.connect_counted_matches({ let win = self.static_ref(); move |_fc, n| {
                    win.find_label.set_text(&format!("{} matches", n));
                } });
                let opts = FindOptions::CASE_INSENSITIVE | FindOptions::WRAP_AROUND;
                fc.search(text, opts.bits(), 500);
            }
        }
    }

    pub fn find_next(&self) {
        if let Some(fc) = self.find_controller.borrow().clone() {
            fc.search_next();
        }
    }

    pub fn find_prev(&self) {
        if let Some(fc) = self.find_controller.borrow().clone() {
            fc.search_previous();
        }
    }

    pub fn find_close(&self) {
        if let Some(fc) = self.find_controller.borrow().clone() {
            fc.search_finish();
        }
        self.find_revealer.set_reveal_child(false);
        self.find_label.set_text("");
    }

    // =====================================================================
    // Zoom / fullscreen / save
    // =====================================================================

    pub fn zoom_in(&self) {
        if let Some(wv) = self.current_webview() {
            wv.set_zoom_level((wv.zoom_level() + 0.1).min(5.0));
            let host = crate::util::host_of_uri(&self.current_uri()).unwrap_or_default();
            self.app.set_site_zoom(&host, wv.zoom_level());
        }
    }

    pub fn zoom_out(&self) {
        if let Some(wv) = self.current_webview() {
            wv.set_zoom_level((wv.zoom_level() - 0.1).max(0.25));
            let host = crate::util::host_of_uri(&self.current_uri()).unwrap_or_default();
            self.app.set_site_zoom(&host, wv.zoom_level());
        }
    }

    pub fn zoom_reset(&self) {
        if let Some(wv) = self.current_webview() {
            wv.set_zoom_level(1.0);
            let host = crate::util::host_of_uri(&self.current_uri()).unwrap_or_default();
            self.app.set_site_zoom(&host, 1.0);
        }
    }

    pub fn toggle_fullscreen(&self) {
        if self.fullscreen.get() {
            self.window.unfullscreen();
            self.fullscreen.set(false);
            self.chrome_revealer.set_reveal_child(true);
        } else {
            self.window.fullscreen();
            self.fullscreen.set(true);
        }
    }

    pub fn save_page(&self) {
        let wv = match self.current_webview() {
            Some(w) => w,
            None => return,
        };
        let title = self.current_title();
        let safe: String = title
            .chars()
            .take(40)
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        let file_name = if safe.is_empty() { "page.mhtml".into() } else { format!("{}.mhtml", safe.to_lowercase()) };
        let mut path = crate::util::downloads_dir();
        let _ = std::fs::create_dir_all(&path);
        path.push(file_name);
        let mut final_path = path.clone();
        let mut n = 1;
        while final_path.exists() {
            let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            final_path = path.with_file_name(format!("{}-{}.mhtml", stem, n));
            n += 1;
        }
        let file = gtk4::gio::File::for_path(&final_path);
        wv.save_to_file(
            &file,
            webkit6::SaveMode::Mhtml,
            None::<&gtk4::gio::Cancellable>,
            { let win = self.static_ref(); move |res| {
                if res.is_err() {
                    win.status_message(Some("Could not save page".into()));
                } else {
                    win.status_message(Some(format!("Saved {}", final_path.display())));
                }
            } },
        );
    }

    // =====================================================================
    // Prompts
    // =====================================================================

    pub fn prompt_save_password(&self, app: &Arc<App>, origin: &str, user: &str, pass: &str) {
        if origin.is_empty() || origin.starts_with("peregrine://") {
            return;
        }
        let dlg = adw::AlertDialog::builder()
            .heading("Save password?")
            .body(format!("Save the login for {} in Peregrine's encrypted vault?", origin))
            .build();
        dlg.add_response("no", "Not now");
        dlg.add_response("never", "Never for this site");
        dlg.add_response("save", "Save");
        dlg.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        let origin = origin.to_string();
        let user = user.to_string();
        let pass = pass.to_string();
        dlg.choose(Some(&self.window), None::<&gtk4::gio::Cancellable>, gtk4::glib::clone!(#[strong] app, move |resp| {
            match resp.as_str() {
                "save" => {
                    if let Err(e) = app.data.vault.save(&origin, &user, &pass) {
                        eprintln!("peregrine: vault save failed: {e}");
                    }
                }
                "never" => {
                    app.data.prefs.set_perm(&origin, "passwords", "deny");
                }
                _ => {}
            }
        }));
    }

    pub fn prompt_permission(&self, req: &PermissionRequest, kind: &str, origin: &str) {
        let (title, icon) = match kind {
            "camera" => ("Camera access", "📷"),
            "microphone" => ("Microphone access", "🎙️"),
            "location" => ("Location access", "📍"),
            "notifications" => ("Notifications", "🔔"),
            _ => ("Permission request", "❓"),
        };
        let dlg = adw::AlertDialog::builder()
            .heading(format!("{icon} {title}"))
            .body(format!("{} wants to use this. Allow?", origin))
            .build();
        dlg.add_response("deny", "Block");
        dlg.add_response("deny-always", "Block always");
        dlg.add_response("allow-always", "Allow always");
        dlg.add_response("allow", "Allow");
        dlg.set_response_appearance("allow", adw::ResponseAppearance::Suggested);
        let req = req.clone();
        let app = self.app.clone();
        let kind = kind.to_string();
        let origin = origin.to_string();
        dlg.choose(Some(&self.window), None::<&gtk4::gio::Cancellable>, move |resp| {
            match resp.as_str() {
                "allow" => req.allow(),
                "allow-always" => {
                    app.data.prefs.set_perm(&origin, &kind, "allow");
                    req.allow();
                }
                "deny-always" => {
                    app.data.prefs.set_perm(&origin, &kind, "deny");
                    req.deny();
                }
                _ => req.deny(),
            }
        });
    }

    // =====================================================================
    // Misc
    // =====================================================================

    fn clear_pending(&self, page: &adw::TabPage) {
        let mut tabs = self.tabs.borrow_mut();
        if let Some(t) = tabs.iter_mut().find(|t| &t.page == page) {
            t.pending = None;
        }
    }

    fn mark_active(&self, page: &adw::TabPage) {
        let mut tabs = self.tabs.borrow_mut();
        if let Some(t) = tabs.iter_mut().find(|t| &t.page == page) {
            t.last_active = Instant::now();
        }
    }

    pub fn refresh_download_badge(&self, app: &Arc<App>) {
        let active = app.download_hub.active_count();
        if active > 0 {
            self.download_badge.set_text(&active.to_string());
            self.download_badge.set_visible(true);
        } else {
            self.download_badge.set_visible(false);
        }
    }

    pub fn session_state(&self) -> serde_json::Value {
        let mut tabs_json = vec![];
        {
            let tabs = self.tabs.borrow();
            for t in tabs.iter() {
                let uri = t.pending.clone().or_else(|| t.webview.uri().map(|u| u.to_string())).unwrap_or_default();
                if uri.is_empty() || uri == "about:blank" {
                    continue;
                }
                tabs_json.push(serde_json::json!({
                    "url": uri,
                    "title": t.page.title().to_string(),
                    "pinned": t.page.is_pinned(),
                }));
            }
        }
        for (_, uri, title) in self.sleeping_pages.borrow().iter() {
            tabs_json.push(serde_json::json!({ "url": uri, "title": title, "pinned": false }));
        }
        let active = self
            .tabs
            .borrow()
            .iter()
            .position(|t| Some(&t.page) == self.tab_view.selected_page().as_ref())
            .unwrap_or(0);
        serde_json::json!({ "tabs": tabs_json, "active": active })
    }

    pub fn restore_from_state(&self, state: &serde_json::Value) {
        let tabs = state.get("tabs").and_then(|t| t.as_array()).cloned().unwrap_or_default();
        if tabs.is_empty() {
            self.new_tab_home();
            return;
        }
        for t in tabs {
            let url = t.get("url").and_then(|u| u.as_str()).unwrap_or("peregrine://newtab");
            let pinned = t.get("pinned").and_then(|p| p.as_bool()).unwrap_or(false);
            let title = t.get("title").and_then(|x| x.as_str()).unwrap_or("");
            let wv = crate::webview::create_view(self, &self.app);
            let page = if pinned {
                self.tab_view.append_pinned(&wv)
            } else {
                self.tab_view.append(&wv)
            };
            page.set_title(if title.is_empty() { url } else { title });
            self.tabs.borrow_mut().push(TabState {
                page: page.clone(),
                webview: wv.clone(),
                pending: Some(url.to_string()),
                last_active: Instant::now(),
                sleeping: false,
            });
        }
        let active = state.get("active").and_then(|a| a.as_u64()).unwrap_or(0) as usize;
        let pages: Vec<adw::TabPage> = {
            let tabs = self.tabs.borrow();
            tabs.iter().map(|t| t.page.clone()).collect()
        };
        if let Some(p) = pages.get(active) {
            self.tab_view.set_selected_page(p);
        } else if let Some(p) = pages.first() {
            self.tab_view.set_selected_page(p);
        }
    }

    pub fn is_active(&self) -> bool {
        self.window.is_active()
    }

    pub fn widget(&self) -> &adw::ApplicationWindow {
        &self.window
    }

    pub fn present(&self) {
        self.window.present();
    }
}

fn flat_btn(icon: &str, tooltip: &str) -> gtk4::Button {
    gtk4::Button::builder()
        .icon_name(icon)
        .css_classes(["flat", "peregrine-nav"])
        .tooltip_text(tooltip)
        .build()
}
