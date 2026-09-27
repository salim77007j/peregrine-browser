//! Application core: state, WebKit context/session setup, preference application,
//! window lifecycle, session persistence.

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

use gtk4::glib;
use webkit6::prelude::*;
use webkit6::{CookieManager, NetworkProxyMode, NetworkProxySettings, NetworkSession, Settings, WebContext};

use crate::data::Data;
use crate::privacy::PrivacyManager;
use crate::util;
use crate::window::BrowserWindow;

pub struct App {
    pub app: libadwaita::Application,
    pub data: Arc<Data>,
    pub privacy: Arc<PrivacyManager>,
    pub context: WebContext,
    pub session: NetworkSession,
    pub settings: Settings,
    pub windows: std::cell::RefCell<Vec<&'static BrowserWindow>>,
    pub closed_tabs: std::cell::RefCell<VecDeque<String>>,
    pub sb_whitelist: std::sync::Mutex<HashSet<String>>,
    /// path for the sentinel "running" file used to detect crashes
    sentinel: std::path::PathBuf,
    pub version: &'static str,
    pub download_hub: crate::downloads::DownloadHub,
}

impl App {
    /// Build the application state. Called once from main.
    pub fn bootstrap() -> Arc<Self> {
        let profile = util::profile_dir();
        let data = Arc::new(Data::open(&profile).unwrap_or_else(|e| {
            eprintln!("peregrine: fatal: cannot open profile at {}: {}", profile.display(), e);
            std::process::exit(1);
        }));

        // ---- privacy subsystem (engine + proxy) ----
        let mut prefs = data.prefs.get();
        if std::env::var("PEREGRINE_SELFTEST").is_ok() {
            data.prefs.set("custom_filters", "###ad-banner\n##.ad-slot\n");
            prefs = data.prefs.get();
        }
        let privacy = Arc::new(PrivacyManager::new(&prefs));
        privacy.start();

        // ---- WebKit context (scheme handlers must exist before any NetworkSession) ----
        let context = WebContext::new();
        crate::pages::register(&context);

        // ---- WebKit: website data + network session ----
        let base_data = profile.join("webkit-data");
        let cache = profile.join("cache");
        let _ = std::fs::create_dir_all(&base_data);
        let _ = std::fs::create_dir_all(&cache);
        let session = NetworkSession::new(Some(base_data.to_str().unwrap_or_default()), Some(cache.to_str().unwrap_or_default()));

        session.set_itp_enabled(prefs.enable_itp);
        if let Some(cm) = session.cookie_manager() {
            apply_cookie_policy(&cm, &prefs.cookie_policy);
        }

        // routing through the filtering proxy (loopback only; loopback test servers bypass)
        let no_proxy = std::env::var("PEREGRINE_NO_PROXY").is_ok();
        let mut configured = false;
        for _ in 0..50 {
            if no_proxy {
                break;
            }
            if let Some(addr) = privacy.proxy_addr() {
                let settings = NetworkProxySettings::new(Some(&addr), &["127.0.0.1", "localhost", "0.0.0.0"]);
                session.set_proxy_settings(NetworkProxyMode::Custom, Some(&settings));
                configured = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        if !configured {
            session.set_proxy_settings(NetworkProxyMode::Default, None);
            eprintln!("peregrine: filtering proxy unavailable — traffic flows unfiltered");
        }


        // ---- shared settings ----
        let settings = build_webkit_settings(&prefs);

        let adw_app = libadwaita::Application::builder()
            .application_id("com.peregrinebrowser.Peregrine")
            .flags(gtk4::gio::ApplicationFlags::empty())
            .build();

        let sentinel = profile.join("running.lock");

        let app = Arc::new(Self {
            app: adw_app,
            data,
            privacy,
            context,
            session,
            settings,
            windows: std::cell::RefCell::new(vec![]),
            closed_tabs: std::cell::RefCell::new(VecDeque::new()),
            sb_whitelist: std::sync::Mutex::new(HashSet::new()),
            sentinel,
            version: env!("CARGO_PKG_VERSION"),
            download_hub: crate::downloads::DownloadHub::new(),
        });

        // sentinel for crash detection (session restore prompt)
        let _ = std::fs::write(&app.sentinel, std::process::id().to_string());

        // publish for the peregrine:// scheme handler
        crate::pages::publish_app(app.clone());

        app
    }

    /// Open a URL: reuse the active window (same or new tab).
    pub fn open_url(&self, url: &str, new_tab: bool) {
        let win = self.active_window();
        if let Some(w) = win {
            if new_tab {
                w.new_tab_url(url);
            } else {
                w.navigate(url);
            }
        }
    }

    pub fn active_window(&self) -> Option<&'static BrowserWindow> {
        let list = self.windows.borrow();
        for w in list.iter().rev() {
            if w.is_active() {
                return Some(w);
            }
        }
        list.last().copied()
    }

    pub fn any_window(&self) -> Option<&'static BrowserWindow> {
        let list = self.windows.borrow();
        list.last().copied()
    }

    /// Re-apply preferences everywhere (called after any change from settings UI).
    pub fn apply_prefs(&self) {
        let prefs = self.data.prefs.get();

        // privacy engine (background rebuild)
        let engine = self.privacy.engine.clone();
        let cfg = crate::privacy::adblock::BlockConfig {
            ads: prefs.block_ads,
            trackers: prefs.block_trackers,
            annoyances: prefs.block_annoyances,
            malware: prefs.block_malware,
            heuristics: prefs.heuristic_blocking,
            cosmetic: prefs.cosmetic_filtering,
            custom: prefs.custom_filters.clone(),
        };
        std::thread::Builder::new()
            .name("engine-rebuild".into())
            .spawn(move || engine.rebuild_with(cfg))
            .ok();

        // cookies + ITP
        self.session.set_itp_enabled(prefs.enable_itp);
        if let Some(cm) = self.session.cookie_manager() {
            apply_cookie_policy(&cm, &prefs.cookie_policy);
        }

        // engine-level web settings (shared across views)
        apply_web_settings(&self.settings, &prefs);

        // theme
        apply_theme(&prefs.theme);

        // windows refresh (bookmark bar, compact tabs)
        let windows: Vec<&'static BrowserWindow> = self.windows.borrow().clone();
        for w in windows {
            w.refresh_appearance(&prefs);
        }
    }

    /// Clear browsing data categories ("history,cookies,cache" subset).
    pub fn clear_browsing_data(&self, what: &str) {
        for part in what.split(',') {
            match part.trim() {
                "history" => self.data.history.clear(),
                "cookies" => {
                    // wipe WebKit site data (cookies, storage) asynchronously
                    let types = webkit6::WebsiteDataTypes::COOKIES
                        | webkit6::WebsiteDataTypes::LOCAL_STORAGE
                        | webkit6::WebsiteDataTypes::INDEXEDDB_DATABASES
                        | webkit6::WebsiteDataTypes::SERVICE_WORKER_REGISTRATIONS;
                    clear_website_data(&self.session, types);
                }
                "cache" => {
                    let types = webkit6::WebsiteDataTypes::DISK_CACHE
                        | webkit6::WebsiteDataTypes::MEMORY_CACHE;
                    clear_website_data(&self.session, types);
                }
                _ => {}
            }
        }
    }

    // ---- pinned new-tab tiles ----
    pub fn pinned_tiles(&self) -> Vec<(String, String)> {
        let prefs = self.data.prefs.get();
        parse_tiles(&read_pref_raw(self, "pinned_tiles_json"))
            .or_else(|| default_tiles())
            .unwrap_or_default()
    }

    pub fn set_pinned_tiles(&self, tiles: Vec<(String, String)>) {
        let json = serde_json::to_string(
            &tiles.into_iter().map(|(u, t)| serde_json::json!([u, t])).collect::<Vec<_>>(),
        )
        .unwrap_or_default();
        self.data.prefs.set("pinned_tiles_json", &json);
    }

    // ---- custom filter list URLs ----
    pub fn custom_lists(&self) -> Vec<String> {
        parse_list_urls(&read_pref_raw(self, "custom_lists_json")).unwrap_or_default()
    }

    pub fn set_custom_lists(&self, urls: Vec<String>) {
        let json = serde_json::to_string(&urls).unwrap_or_default();
        self.data.prefs.set("custom_lists_json", &json);
    }

    // ---- safe browsing proceed whitelist ----
    pub fn sb_whitelist_add(&self, host: &str) {
        self.sb_whitelist.lock().unwrap().insert(host.to_string());
    }

    pub fn sb_whitelisted(&self) -> HashSet<String> {
        self.sb_whitelist.lock().unwrap().clone()
    }

    // ---- per-site zoom ----
    pub fn site_zoom(&self, host: &str) -> f64 {
        parse_zoom_map(&read_pref_raw(self, "site_zoom_json"))
            .and_then(|m| m.get(host).copied())
            .unwrap_or(1.0)
    }

    pub fn set_site_zoom(&self, host: &str, zoom: f64) {
        if host.is_empty() {
            return;
        }
        let mut m = parse_zoom_map(&read_pref_raw(self, "site_zoom_json")).unwrap_or_default();
        m.insert(host.to_string(), zoom);
        if let Ok(json) = serde_json::to_string(&m) {
            self.data.prefs.set("site_zoom_json", &json);
        }
    }

    /// Track a closed tab for Ctrl+Shift+T reopen.
    pub fn push_closed_tab(&self, url: &str) {
        if url.is_empty() || url == "about:blank" {
            return;
        }
        let mut q = self.closed_tabs.borrow_mut();
        q.push_front(url.to_string());
        q.truncate(25);
    }

    pub fn pop_closed_tab(&self) -> Option<String> {
        self.closed_tabs.borrow_mut().pop_front()
    }

    pub fn register_window(&self, w: &'static BrowserWindow) {
        self.windows.borrow_mut().push(w);
    }

    pub fn unregister_window(&self, w: &BrowserWindow) {
        self.windows.borrow_mut().retain(|x| !std::ptr::eq(*x, w));
    }

    /// Save the session (all windows, all tabs) synchronously.
    pub fn save_session(&self) {
        let windows: Vec<&'static BrowserWindow> = self.windows.borrow().clone();
        let mut win_states = vec![];
        for w in &windows {
            win_states.push(w.session_state());
        }
        let state = serde_json::json!({ "windows": win_states });
        let _ = std::fs::write(util::profile_dir().join("session.json"), state.to_string());
    }

    pub fn load_session(&self) -> Option<serde_json::Value> {
        let txt = std::fs::read_to_string(util::profile_dir().join("session.json")).ok()?;
        serde_json::from_str(&txt).ok()
    }

    pub fn clean_shutdown(&self) {
        self.save_session();
        let _ = std::fs::remove_file(&self.sentinel);
        let prefs = self.data.prefs.get();
        if prefs.clear_browsing_on_exit {
            self.clear_browsing_data("history,cookies,cache");
        }
    }

    /// Was the previous run terminated without a clean shutdown?
    pub fn crashed_last_time(&self) -> bool {
        util::profile_dir().join("running.lock").exists()
    }
}

fn parse_zoom_map(json: &str) -> Option<std::collections::HashMap<String, f64>> {
    serde_json::from_str(json).ok()
}

fn parse_tiles(json: &str) -> Option<Vec<(String, String)>> {
    let v: Vec<serde_json::Value> = serde_json::from_str(json).ok()?;
    Some(v.into_iter().filter_map(|x| {
        let a = x.as_array()?;
        Some((a.first()?.as_str()?.to_string(), a.get(1)?.as_str().unwrap_or("").to_string()))
    }).collect())
}

fn default_tiles() -> Option<Vec<(String, String)>> {
    None
}

fn parse_list_urls(json: &str) -> Option<Vec<String>> {
    serde_json::from_str(json).ok()
}

fn read_pref_raw(app: &App, key: &str) -> String {
    // Read a raw key from the prefs table (keys unknown to the typed Prefs struct
    // still persist there — e.g. JSON blobs for tiles/zooms).
    use rusqlite::OptionalExtension;
    let _ = app;
    let conn = match rusqlite::Connection::open(util::db_path()) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    conn.query_row("SELECT value FROM prefs WHERE key=?1", rusqlite::params![key], |r| r.get(0))
        .optional()
        .ok()
        .flatten()
        .unwrap_or_default()
}

fn apply_cookie_policy(cm: &CookieManager, policy: &str) {
    let p = match policy {
        "always" => webkit6::CookieAcceptPolicy::Always,
        "never" => webkit6::CookieAcceptPolicy::Never,
        _ => webkit6::CookieAcceptPolicy::NoThirdParty,
    };
    cm.set_accept_policy(p);
}

pub fn build_webkit_settings(prefs: &crate::data::prefs::Prefs) -> Settings {
    let s = Settings::new();
    apply_web_settings(&s, prefs);
    s
}

fn apply_web_settings(s: &Settings, prefs: &crate::data::prefs::Prefs) {
    s.set_enable_javascript(true);
    s.set_enable_webgl(prefs.enable_webgl);
    s.set_enable_smooth_scrolling(prefs.smooth_scrolling);
    s.set_enable_dns_prefetching(prefs.prefetch_dns);
    s.set_enable_developer_extras(prefs.developer_extras);
    s.set_enable_hyperlink_auditing(false); // never send <a ping> beacons
    s.set_enable_fullscreen(true);
    s.set_enable_mediasource(true);
    s.set_enable_webaudio(true);
    s.set_enable_encrypted_media(true); // EME off by default? keep on for daily usability
    s.set_enable_offline_web_application_cache(true);
    s.set_enable_page_cache(true);
    s.set_enable_back_forward_navigation_gestures(true); // touchpad swipe nav
    s.set_enable_caret_browsing(false);
    s.set_enable_site_specific_quirks(true);
    s.set_enable_webrtc(true);
    s.set_enable_media_stream(true); // gated by our permission manager
    s.set_allow_file_access_from_file_urls(false);
    s.set_allow_universal_access_from_file_urls(false);
    s.set_allow_modal_dialogs(false);
    s.set_allow_top_navigation_to_data_urls(false);
    s.set_draw_compositing_indicators(false);
    if prefs.user_agent.trim().is_empty() {
        s.set_user_agent_with_application_details(Some("Peregrine"), Some(env!("CARGO_PKG_VERSION")));
    } else {
        s.set_user_agent(Some(prefs.user_agent.trim()));
    }
}

fn apply_theme(theme: &str) {
    let sm = libadwaita::StyleManager::default();
    use libadwaita::ColorScheme;
    match theme {
        "light" => sm.set_color_scheme(ColorScheme::ForceLight),
        "system" => sm.set_color_scheme(ColorScheme::PreferDark),
        _ => sm.set_color_scheme(ColorScheme::PreferDark),
    }
}

fn clear_website_data(session: &NetworkSession, types: webkit6::WebsiteDataTypes) {
    if let Some(mgr) = session.website_data_manager() {
        crate::ffi::website_data_manager_clear(&mgr, types, 0);
    }
}
