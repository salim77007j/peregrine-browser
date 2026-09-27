//! Bookmarks store with folder support and Netscape-format import/export.

use std::path::Path;

#[derive(Debug, Clone)]
pub struct Bookmark {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub folder: String, // flat folder name ("/" = root)
    pub added: i64,
}

pub struct Bookmarks {
    conn: std::sync::Mutex<rusqlite::Connection>,
}

impl Bookmarks {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = rusqlite::Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS bookmarks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL DEFAULT '',
                folder TEXT NOT NULL DEFAULT '/',
                added INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_bm_folder ON bookmarks(folder);",
        )?;
        Ok(Self { conn: std::sync::Mutex::new(conn) })
    }

    pub fn add(&self, url: &str, title: &str, folder: &str) -> bool {
        let now = chrono::Utc::now().timestamp();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO bookmarks(url, title, folder, added) VALUES(?1,?2,?3,?4)",
            rusqlite::params![url, title, folder, now],
        )
        .map(|n| n > 0)
        .unwrap_or(false)
    }

    pub fn update(&self, id: i64, title: &str, url: &str, folder: &str) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute(
            "UPDATE bookmarks SET title=?2, url=?3, folder=?4 WHERE id=?1",
            rusqlite::params![id, title, url, folder],
        );
    }

    pub fn remove(&self, id: i64) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute("DELETE FROM bookmarks WHERE id=?1", rusqlite::params![id]);
    }

    pub fn contains(&self, url: &str) -> bool {
        let conn = self.conn.lock().unwrap();
        conn.query_row("SELECT 1 FROM bookmarks WHERE url=?1", rusqlite::params![url], |_| Ok(()))
            .is_ok()
    }

    pub fn toggle(&self, url: &str, title: &str) -> bool {
        if self.contains(url) {
            let conn = self.conn.lock().unwrap();
            let _ = conn.execute("DELETE FROM bookmarks WHERE url=?1", rusqlite::params![url]);
            false
        } else {
            self.add(url, title, "/");
            true
        }
    }

    pub fn all(&self) -> Vec<Bookmark> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare("SELECT id, url, title, folder, added FROM bookmarks ORDER BY folder, title COLLATE NOCASE") {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let rows = stmt
            .query_map([], |r| {
                Ok(Bookmark { id: r.get(0)?, url: r.get(1)?, title: r.get(2)?, folder: r.get(3)?, added: r.get(4)? })
            })
            .map(|it| it.filter_map(|x| x.ok()).collect())
            .unwrap_or_default();
        rows
    }

    pub fn search(&self, q: &str, limit: usize) -> Vec<Bookmark> {
        let conn = self.conn.lock().unwrap();
        let like = format!("%{}%", q.replace('%', ""));
        let mut stmt = match conn.prepare(
            "SELECT id, url, title, folder, added FROM bookmarks
             WHERE url LIKE ?1 OR title LIKE ?1 ORDER BY title LIMIT ?2",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let rows = stmt
            .query_map(rusqlite::params![like, limit as i64], |r| {
                Ok(Bookmark { id: r.get(0)?, url: r.get(1)?, title: r.get(2)?, folder: r.get(3)?, added: r.get(4)? })
            })
            .map(|it| it.filter_map(|x| x.ok()).collect())
            .unwrap_or_default();
        rows
    }

    pub fn folders(&self) -> Vec<String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare("SELECT DISTINCT folder FROM bookmarks ORDER BY folder") {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let rows = stmt
            .query_map([], |r| r.get(0))
            .map(|it| it.filter_map(|x| x.ok()).collect())
            .unwrap_or_default();
        rows
    }

    /// Export all bookmarks in the classic Netscape bookmarks.html format.
    pub fn export_html(&self) -> String {
        let mut out = String::from(
            "<!DOCTYPE NETSCAPE-Bookmark-file-1>\n<!-- This is an automatically generated file. -->\n<META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\n<TITLE>Bookmarks</TITLE>\n<H1>Bookmarks</H1>\n<DL><p>\n",
        );
        for bm in self.all() {
            let title = escape_html(&bm.title);
            let folder = escape_html(&bm.folder);
            if bm.folder != "/" {
                out.push_str(&format!("    <DT><H3>{}</H3>\n    <DL><p>\n", folder));
            }
            out.push_str(&format!(
                "        <DT><A HREF=\"{}\">{}</A>\n",
                escape_html(&bm.url),
                title
            ));
            if bm.folder != "/" {
                out.push_str("    </DL><p>\n");
            }
        }
        out.push_str("</DL><p>\n");
        out
    }

    /// Parse a Netscape bookmarks file (subset: DT/A and H3 lines).
    pub fn import_html(&self, html: &str) -> usize {
        let mut count = 0;
        let mut current_folder = String::from("/");
        for line in html.lines() {
            let l = line.trim();
            if l.starts_with("<DT>") || l.starts_with("<dt>") {
                let l = &l[4..];
                if let Some(rest) = l.strip_prefix("<H3").or_else(|| l.strip_prefix("<h3")) {
                    // folder heading
                    if let Some(start) = rest.find('>').map(|i| i + 1) {
                        if let Some(end) = rest[start..].find("</H3").or_else(|| rest[start..].find("</h3")) {
                            current_folder = rest[start..start + end].to_string();
                            if current_folder.is_empty() {
                                current_folder = "/".into();
                            }
                        }
                    }
                } else if let Some(rest) = l.strip_prefix("<A ").or_else(|| l.strip_prefix("<a ")) {
                    if let (Some(href_start), Some(tag_end)) = (rest.find("HREF=\"").or_else(|| rest.find("href=\"")), rest.find('>')) {
                        let hs = rest[href_start..].find('"').map(|i| href_start + i + 1).unwrap_or(0);
                        let he = rest[hs..].find('"').map(|i| hs + i).unwrap_or(0);
                        let url = &rest[hs..he];
                        let title = unescape_html(&rest[tag_end + 1..].trim_end_matches("</A>").trim_end_matches("</a>"));
                        if url.starts_with("http") && self.add(url, &title, &current_folder) {
                            count += 1;
                        }
                    }
                }
            }
        }
        count
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
fn unescape_html(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_export_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let bm = Bookmarks::open(&dir.path().join("t.db")).unwrap();
        bm.add("https://a.io/", "A", "/");
        bm.add("https://b.io/", "B & C", "/News");
        let html = bm.export_html();
        let bm2 = Bookmarks::open(&dir.path().join("t2.db")).unwrap();
        let n = bm2.import_html(&html);
        assert_eq!(n, 2);
        assert!(bm2.contains("https://a.io/"));
        assert!(bm2.contains("https://b.io/"));
    }
}
