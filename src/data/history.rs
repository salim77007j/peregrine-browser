//! History store (SQLite) — visited pages, visit counts, frecency-style scoring.

use std::path::Path;

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub visit_count: i64,
    pub last_visit: i64, // unix seconds
}

pub struct History {
    conn: std::sync::Mutex<rusqlite::Connection>,
}

impl History {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = rusqlite::Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL DEFAULT '',
                visit_count INTEGER NOT NULL DEFAULT 1,
                last_visit INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_history_last ON history(last_visit DESC);
            CREATE INDEX IF NOT EXISTS idx_history_count ON history(visit_count DESC);
            CREATE VIRTUAL TABLE IF NOT EXISTS history_fts USING fts5(url, title, content='history', content_rowid='id');",
        )?;
        Ok(Self { conn: std::sync::Mutex::new(conn) })
    }

    pub fn visit(&self, url: &str, title: &str) {
        if url.is_empty() || url.starts_with("peregrine://") || url == "about:blank" {
            return;
        }
        let now = chrono::Utc::now().timestamp();
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute(
            "INSERT INTO history(url, title, visit_count, last_visit) VALUES(?1, ?2, 1, ?3)
             ON CONFLICT(url) DO UPDATE SET visit_count=visit_count+1, last_visit=?3,
                 title=CASE WHEN ?2 != '' THEN ?2 ELSE title END",
            rusqlite::params![url, title, now],
        );
        // keep FTS in sync
        let _ = conn.execute(
            "INSERT INTO history_fts(rowid, url, title)
             SELECT id, url, title FROM history WHERE url=?1
             ON CONFLICT(rowid) DO UPDATE SET url=excluded.url, title=excluded.title",
            rusqlite::params![url],
        );
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<HistoryEntry> {
        let conn = self.conn.lock().unwrap();
        let like = format!("%{}%", query.replace('%', ""));
        let mut stmt = match conn.prepare(
            "SELECT id, url, title, visit_count, last_visit FROM history
             WHERE url LIKE ?1 OR title LIKE ?1
             ORDER BY visit_count * 2 + last_visit/86400 DESC LIMIT ?2",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let rows = stmt
            .query_map(rusqlite::params![like, limit as i64], |r| {
                Ok(HistoryEntry {
                    id: r.get(0)?,
                    url: r.get(1)?,
                    title: r.get(2)?,
                    visit_count: r.get(3)?,
                    last_visit: r.get(4)?,
                })
            })
            .map(|it| it.filter_map(|x| x.ok()).collect())
            .unwrap_or_default();
        rows
    }

    pub fn recent(&self, limit: usize, offset: usize) -> Vec<HistoryEntry> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id, url, title, visit_count, last_visit FROM history
             ORDER BY last_visit DESC LIMIT ?1 OFFSET ?2",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let rows = stmt
            .query_map(rusqlite::params![limit as i64, offset as i64], |r| {
                Ok(HistoryEntry {
                    id: r.get(0)?,
                    url: r.get(1)?,
                    title: r.get(2)?,
                    visit_count: r.get(3)?,
                    last_visit: r.get(4)?,
                })
            })
            .map(|it| it.filter_map(|x| x.ok()).collect())
            .unwrap_or_default();
        rows
    }

    /// Top sites for the new-tab speed dial.
    pub fn top_sites(&self, limit: usize) -> Vec<HistoryEntry> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id, url, title, visit_count, last_visit FROM history
             WHERE url LIKE 'http%'
             GROUP BY substr(url, instr(url, '://')+3, 40)
             ORDER BY visit_count DESC LIMIT ?1",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let rows = stmt
            .query_map(rusqlite::params![limit as i64], |r| {
                Ok(HistoryEntry {
                    id: r.get(0)?,
                    url: r.get(1)?,
                    title: r.get(2)?,
                    visit_count: r.get(3)?,
                    last_visit: r.get(4)?,
                })
            })
            .map(|it| it.filter_map(|x| x.ok()).collect())
            .unwrap_or_default();
        rows
    }

    pub fn remove(&self, id: i64) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute("DELETE FROM history WHERE id=?1", rusqlite::params![id]);
    }

    pub fn remove_urls_matching(&self, host: &str) {
        let conn = self.conn.lock().unwrap();
        let like = format!("%{}%", host);
        let _ = conn.execute("DELETE FROM history WHERE url LIKE ?1", rusqlite::params![like]);
    }

    pub fn clear(&self) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute_batch("DELETE FROM history; DELETE FROM history_fts;");
    }

    pub fn count(&self) -> i64 {
        let conn = self.conn.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM history", [], |r| r.get(0)).unwrap_or(0)
    }
}
