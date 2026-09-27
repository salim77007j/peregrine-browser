//! Typed preferences store, persisted in SQLite, with change notifications.

use serde::{Deserialize, Serialize};
use std::sync::mpsc::{channel, Receiver, Sender};

use crate::util::db_path;

/// Every tunable preference in the browser. Defaults follow privacy-first principles.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    // ---- General ----
    pub search_engine: String,        // id into SEARCH_ENGINES
    pub custom_search_engine: String, // url template for "custom"
    pub home_page: String,
    pub startup_restore_session: bool,
    pub downloads_dir: String,
    pub ask_where_to_save: bool,

    // ---- Appearance ----
    pub theme: String, // "dark" | "light" | "system"
    pub show_bookmark_bar: bool,
    pub compact_tabs: bool,

    // ---- Privacy & security ----
    pub block_ads: bool,
    pub block_trackers: bool,
    pub block_annoyances: bool,
    pub block_malware: bool,
    pub heuristic_blocking: bool,
    pub cosmetic_filtering: bool,
    pub cookie_policy: String, // "no-third-party" | "always" | "never"
    pub enable_itp: bool,
    pub fingerprint_shield: String, // "strict" | "balanced" | "off"
    pub webrtc_leak_protection: bool,
    pub clear_browsing_on_exit: bool,
    pub custom_filters: String, // user-supplied ABP rules, one per line

    // ---- Permissions ----
    pub perm_camera: String, // prompt | allow | deny
    pub perm_microphone: String,
    pub perm_location: String,
    pub perm_notifications: String,
    pub perm_clipboard: String,
    pub perm_popups: String,

    // ---- Performance ----
    pub tab_sleep_minutes: u64, // 0 = never sleep
    pub enable_webgl: bool,
    pub smooth_scrolling: bool,
    pub prefetch_dns: bool,

    // ---- Advanced ----
    pub user_agent: String, // "" = default (Peregrine branded)
    pub do_not_track: bool,
    pub developer_extras: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            search_engine: "duckduckgo".into(),
            custom_search_engine: String::new(),
            home_page: "peregrine://newtab".into(),
            startup_restore_session: true,
            downloads_dir: String::new(), // resolved to ~/Downloads lazily
            ask_where_to_save: false,
            theme: "dark".into(),
            show_bookmark_bar: false,
            compact_tabs: false,
            block_ads: true,
            block_trackers: true,
            block_annoyances: true,
            block_malware: true,
            heuristic_blocking: true,
            cosmetic_filtering: true,
            cookie_policy: "no-third-party".into(),
            enable_itp: true,
            fingerprint_shield: "balanced".into(),
            webrtc_leak_protection: true,
            clear_browsing_on_exit: false,
            custom_filters: String::new(),
            perm_camera: "prompt".into(),
            perm_microphone: "prompt".into(),
            perm_location: "prompt".into(),
            perm_notifications: "prompt".into(),
            perm_clipboard: "prompt".into(),
            perm_popups: "prompt".into(),
            tab_sleep_minutes: 15,
            enable_webgl: true,
            smooth_scrolling: true,
            prefetch_dns: false,
            user_agent: String::new(),
            do_not_track: true,
            developer_extras: true,
        }
    }
}

pub struct SEARCH_ENGINES;

impl SEARCH_ENGINES {
    pub fn all() -> Vec<(&'static str, &'static str, &'static str)> {
        // (id, name, url template)
        vec![
            ("duckduckgo", "DuckDuckGo", "https://duckduckgo.com/?q={}"),
            ("startpage", "Startpage", "https://www.startpage.com/sp/search?query={}"),
            ("brave", "Brave Search", "https://search.brave.com/search?q={}"),
            ("mojeek", "Mojeek", "https://www.mojeek.com/search?q={}"),
            ("google", "Google", "https://www.google.com/search?q={}"),
            ("bing", "Bing", "https://www.bing.com/search?q={}"),
            ("wikipedia", "Wikipedia", "https://en.wikipedia.org/wiki/Special:Search?search={}"),
            ("custom", "Custom…", ""),
        ]
    }
    pub fn url_for(id: &str, custom: &str) -> Option<String> {
        for (i, _, u) in Self::all() {
            if i == id {
                if id == "custom" {
                    let c = custom.trim();
                    if c.contains("{}") {
                        return Some(c.to_string());
                    }
                    return None;
                }
                return Some(u.to_string());
            }
        }
        None
    }
}

/// Event broadcast when any pref changes.
pub enum PrefEvent {
    Changed(Prefs),
}

pub struct PrefsStore {
    conn: std::sync::Mutex<rusqlite::Connection>,
    tx: Sender<PrefEvent>,
    current: std::sync::RwLock<Prefs>,
}

impl PrefsStore {
    pub fn open() -> Result<(Self, Receiver<PrefEvent>), rusqlite::Error> {
        let conn = rusqlite::Connection::open(db_path())?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS prefs (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS perm_exceptions (origin TEXT NOT NULL, perm TEXT NOT NULL, value TEXT NOT NULL, PRIMARY KEY(origin, perm));",
        )?;
        let (tx, rx) = channel();
        let mut prefs = Prefs::default();
        {
            let mut stmt = conn.prepare("SELECT key, value FROM prefs")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            for row in rows {
                let (k, v) = row?;
                prefs.apply_key(&k, &v);
            }
        }
        let current = std::sync::RwLock::new(prefs);
        Ok((
            Self {
                conn: std::sync::Mutex::new(conn),
                tx,
                current,
            },
            rx,
        ))
    }

    pub fn get(&self) -> Prefs {
        self.current.read().unwrap().clone()
    }

    pub fn set(&self, key: &str, value: &str) {
        {
            let mut c = self.current.write().unwrap();
            c.apply_key(key, value);
        }
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute(
            "INSERT INTO prefs(key, value) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET value=?2",
            rusqlite::params![key, value],
        );
        let _ = self.tx.send(PrefEvent::Changed(self.get()));
    }

    /// Per-origin permission exceptions: ("allow" | "deny" | "prompt").
    pub fn perm_for(&self, origin: &str, perm: &str) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT value FROM perm_exceptions WHERE origin=?1 AND perm=?2",
            rusqlite::params![origin, perm],
            |r| r.get(0),
        )
        .ok()
    }

    pub fn set_perm(&self, origin: &str, perm: &str, value: &str) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute(
            "INSERT INTO perm_exceptions(origin, perm, value) VALUES(?1,?2,?3)
             ON CONFLICT(origin, perm) DO UPDATE SET value=?3",
            rusqlite::params![origin, perm, value],
        );
    }

    pub fn all_perms(&self) -> Vec<(String, String, String)> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare("SELECT origin, perm, value FROM perm_exceptions ORDER BY origin") {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map(|it| it.filter_map(|x| x.ok()).collect())
            .unwrap_or_default();
        rows
    }

    pub fn clear_perms(&self) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute("DELETE FROM perm_exceptions", []);
    }
}

impl Prefs {
    /// Apply a single key/value (used both when loading and when the settings page writes).
    pub fn apply_key(&mut self, key: &str, value: &str) {
        match key {
            "search_engine" => self.search_engine = value.into(),
            "custom_search_engine" => self.custom_search_engine = value.into(),
            "home_page" => self.home_page = value.into(),
            "startup_restore_session" => self.startup_restore_session = value == "true",
            "downloads_dir" => self.downloads_dir = value.into(),
            "ask_where_to_save" => self.ask_where_to_save = value == "true",
            "theme" => self.theme = value.into(),
            "show_bookmark_bar" => self.show_bookmark_bar = value == "true",
            "compact_tabs" => self.compact_tabs = value == "true",
            "block_ads" => self.block_ads = value == "true",
            "block_trackers" => self.block_trackers = value == "true",
            "block_annoyances" => self.block_annoyances = value == "true",
            "block_malware" => self.block_malware = value == "true",
            "heuristic_blocking" => self.heuristic_blocking = value == "true",
            "cosmetic_filtering" => self.cosmetic_filtering = value == "true",
            "cookie_policy" => self.cookie_policy = value.into(),
            "enable_itp" => self.enable_itp = value == "true",
            "fingerprint_shield" => self.fingerprint_shield = value.into(),
            "webrtc_leak_protection" => self.webrtc_leak_protection = value == "true",
            "clear_browsing_on_exit" => self.clear_browsing_on_exit = value == "true",
            "custom_filters" => self.custom_filters = value.into(),
            "perm_camera" => self.perm_camera = value.into(),
            "perm_microphone" => self.perm_microphone = value.into(),
            "perm_location" => self.perm_location = value.into(),
            "perm_notifications" => self.perm_notifications = value.into(),
            "perm_clipboard" => self.perm_clipboard = value.into(),
            "perm_popups" => self.perm_popups = value.into(),
            "tab_sleep_minutes" => self.tab_sleep_minutes = value.parse().unwrap_or(15),
            "enable_webgl" => self.enable_webgl = value == "true",
            "smooth_scrolling" => self.smooth_scrolling = value == "true",
            "prefetch_dns" => self.prefetch_dns = value == "true",
            "user_agent" => self.user_agent = value.into(),
            "do_not_track" => self.do_not_track = value == "true",
            "developer_extras" => self.developer_extras = value == "true",
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefs_apply_roundtrip() {
        let mut p = Prefs::default();
        p.apply_key("block_ads", "false");
        assert!(!p.block_ads);
        p.apply_key("tab_sleep_minutes", "30");
        assert_eq!(p.tab_sleep_minutes, 30);
    }

    #[test]
    fn engine_url_lookup() {
        assert!(SEARCH_ENGINES::url_for("duckduckgo", "").unwrap().contains("{}"));
        assert!(SEARCH_ENGINES::url_for("custom", "https://s/?q={}").is_some());
        assert!(SEARCH_ENGINES::url_for("custom", "no placeholder").is_none());
    }
}
