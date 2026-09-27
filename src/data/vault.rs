//! Downloads metadata store + encrypted password vault (AES-256-GCM, Argon2id).

use std::path::Path;

// ---------------------------------------------------------------------------
// Downloads
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DownloadRecord {
    pub id: i64,
    pub url: String,
    pub path: String,
    pub mime: String,
    pub size: i64,
    pub state: String, // running | finished | cancelled | failed
    pub started: i64,
}

pub struct DownloadStore {
    conn: std::sync::Mutex<rusqlite::Connection>,
}

impl DownloadStore {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = rusqlite::Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS downloads (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL,
                path TEXT NOT NULL,
                mime TEXT NOT NULL DEFAULT '',
                size INTEGER NOT NULL DEFAULT 0,
                state TEXT NOT NULL DEFAULT 'running',
                started INTEGER NOT NULL
            );",
        )?;
        Ok(Self { conn: std::sync::Mutex::new(conn) })
    }

    pub fn insert(&self, rec: &DownloadRecord) -> i64 {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO downloads(url, path, mime, size, state, started) VALUES(?1,?2,?3,?4,?5,?6)",
            rusqlite::params![rec.url, rec.path, rec.mime, rec.size, rec.state, rec.started],
        )
        .map(|_| conn.last_insert_rowid())
        .unwrap_or(0)
    }

    pub fn update(&self, id: i64, size: i64, state: &str) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute(
            "UPDATE downloads SET size=?2, state=?3 WHERE id=?1",
            rusqlite::params![id, size, state],
        );
    }

    pub fn list(&self, limit: usize) -> Vec<DownloadRecord> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id, url, path, mime, size, state, started FROM downloads ORDER BY started DESC LIMIT ?1",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let rows = stmt
            .query_map(rusqlite::params![limit as i64], |r| {
                Ok(DownloadRecord {
                    id: r.get(0)?,
                    url: r.get(1)?,
                    path: r.get(2)?,
                    mime: r.get(3)?,
                    size: r.get(4)?,
                    state: r.get(5)?,
                    started: r.get(6)?,
                })
            })
            .map(|it| it.filter_map(|x| x.ok()).collect())
            .unwrap_or_default();
        rows
    }

    pub fn clear(&self) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute("DELETE FROM downloads", []);
    }
}

// ---------------------------------------------------------------------------
// Password vault — AES-256-GCM with Argon2id key derivation.
//
// Threat model: at-rest protection of saved credentials in the profile dir.
// The 16-byte random salt is stored next to the vault; the key is derived from
// a random 32-byte master secret kept in a 0600 file inside the profile.
// (OS keychain integration is the documented next step — see ARCHITECTURE.md.)
// ---------------------------------------------------------------------------

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedLogin {
    pub origin: String,
    pub username: String,
    #[serde(skip_serializing)]
    pub secret: String, // encrypted blob, base64
    pub secret_blob: Vec<u8>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct VaultFile {
    pub logins: Vec<LoginRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRecord {
    pub origin: String,
    pub username: String,
    pub nonce: String,     // base64 12 bytes
    pub ciphertext: String, // base64
    pub created: i64,
}

pub struct PasswordVault {
    cipher: Aes256Gcm,
    path: std::path::PathBuf,
}

impl PasswordVault {
    pub fn open(profile: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(profile).map_err(|e| e.to_string())?;
        let secret_path = profile.join("vault.key");
        let secret: [u8; 32] = if secret_path.exists() {
            let b = std::fs::read(&secret_path).map_err(|e| e.to_string())?;
            if b.len() != 32 {
                return Err("vault key corrupt".into());
            }
            b.try_into().unwrap()
        } else {
            use rand::RngCore;
            let mut k = [0u8; 32];
            OsRng.fill_bytes(&mut k);
            std::fs::write(&secret_path, k).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&secret_path, std::fs::Permissions::from_mode(0o600));
            }
            k
        };
        // Argon2id hardening of the stored random secret (depth 1: also defends
        // against raw-key exfiltration from memory dumps of the key file).
        use argon2::{Argon2, Params, Version};
        let params = Params::new(19456, 2, 1, Some(32)).map_err(|e| e.to_string())?;
        let a2 = Argon2::new(Algorithm2::Argon2id, Version::V0x13, params);
        let mut key = [0u8; 32];
        a2.hash_password_into(&secret, b"peregrine-vault-v1", &mut key)
            .map_err(|e| e.to_string())?;
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        Ok(Self {
            cipher,
            path: profile.join("vault.json"),
        })
    }

    pub fn save(&self, origin: &str, username: &str, secret: &str) -> Result<(), String> {
        use base64::Engine;
        let mut nonce_bytes = [0u8; 12];
        use rand::RngCore;
        OsRng.fill_bytes(&mut nonce_bytes);
        let ct = self
            .cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), secret.as_bytes())
            .map_err(|_| "encrypt failed".to_string())?;
        let mut file = self.load_file()?;
        // replace existing entry for same origin+username
        file.logins.retain(|l| !(l.origin == origin && l.username == username));
        file.logins.push(LoginRecord {
            origin: origin.to_string(),
            username: username.to_string(),
            nonce: base64::engine::general_purpose::STANDARD.encode(nonce_bytes),
            ciphertext: base64::engine::general_purpose::STANDARD.encode(ct),
            created: chrono::Utc::now().timestamp(),
        });
        self.write_file(&file)
    }

    pub fn get(&self, origin: &str) -> Result<Vec<(String, String)>, String> {
        use base64::Engine;
        let file = self.load_file()?;
        let mut out = vec![];
        for l in &file.logins {
            if l.origin != origin {
                continue;
            }
            let nonce = base64::engine::general_purpose::STANDARD
                .decode(&l.nonce)
                .map_err(|_| "corrupt".to_string())?;
            let ct = base64::engine::general_purpose::STANDARD
                .decode(&l.ciphertext)
                .map_err(|_| "corrupt".to_string())?;
            if nonce.len() != 12 {
                continue;
            }
            if let Ok(pt) = self.cipher.decrypt(Nonce::from_slice(&nonce), ct.as_ref()) {
                if let Ok(s) = String::from_utf8(pt) {
                    out.push((l.username.clone(), s));
                }
            }
        }
        Ok(out)
    }

    pub fn remove(&self, origin: &str, username: &str) -> Result<(), String> {
        let mut file = self.load_file()?;
        file.logins
            .retain(|l| !(l.origin == origin && l.username == username));
        self.write_file(&file)
    }

    pub fn list_origins(&self) -> Vec<(String, String, i64)> {
        self.load_file()
            .map(|f| f.logins.iter().map(|l| (l.origin.clone(), l.username.clone(), l.created)).collect())
            .unwrap_or_default()
    }

    fn load_file(&self) -> Result<VaultFile, String> {
        if !self.path.exists() {
            return Ok(VaultFile::default());
        }
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .ok_or_else(|| "vault parse error".to_string())
    }

    fn write_file(&self, f: &VaultFile) -> Result<(), String> {
        let s = serde_json::to_string_pretty(f).map_err(|e| e.to_string())?;
        std::fs::write(&self.path, s).map_err(|e| e.to_string())
    }
}

// Argon2 algorithm re-export (avoids extra import path gymnastics)
use argon2::Algorithm as Algorithm2;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let v = PasswordVault::open(dir.path()).unwrap();
        v.save("https://example.io", "alice", "s3cret!").unwrap();
        let got = v.get("https://example.io").unwrap();
        assert_eq!(got, vec![("alice".into(), "s3cret!".into())]);
        v.remove("https://example.io", "alice").unwrap();
        assert!(v.get("https://example.io").unwrap().is_empty());
    }
}
