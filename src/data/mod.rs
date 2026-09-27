//! Data layer: SQLite-backed stores opened from the profile directory.

pub mod bookmarks;
pub mod history;
pub mod prefs;
pub mod vault;

use std::sync::{Arc, Mutex};

pub struct Data {
    pub history: Arc<history::History>,
    pub bookmarks: Arc<bookmarks::Bookmarks>,
    pub downloads: Arc<vault::DownloadStore>,
    pub vault: Arc<vault::PasswordVault>,
    pub prefs: Arc<prefs::PrefsStore>,
}

impl Data {
    /// Open (or create) all stores. The profile directory must already exist.
    pub fn open(profile: &std::path::Path) -> Result<Self, String> {
        std::fs::create_dir_all(profile).map_err(|e| format!("profile dir: {e}"))?;
        std::fs::create_dir_all(profile.join("cache")).map_err(|e| format!("cache dir: {e}"))?;
        let db = profile.join("peregrine.db");
        let history = history::History::open(&db).map_err(|e| format!("history: {e}"))?;
        let bookmarks = bookmarks::Bookmarks::open(&db).map_err(|e| format!("bookmarks: {e}"))?;
        let downloads = vault::DownloadStore::open(&db).map_err(|e| format!("downloads: {e}"))?;
        let vault = vault::PasswordVault::open(profile)?;
        let (prefs, _rx) = prefs::PrefsStore::open().map_err(|e| format!("prefs: {e}"))?;
        Ok(Self {
            history: Arc::new(history),
            bookmarks: Arc::new(bookmarks),
            downloads: Arc::new(downloads),
            vault: Arc::new(vault),
            prefs: Arc::new(prefs),
        })
    }
}

/// A tiny helper for background DB writes that must not block the UI thread.
pub fn spawn_db_task<F>(f: F)
where
    F: FnOnce() + Send + 'static,
{
    std::thread::Builder::new()
        .name("db-task".into())
        .spawn(f)
        .ok();
}

#[allow(dead_code)]
pub fn mutex_lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
