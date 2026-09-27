//! Small shared utilities: paths, hostnames, string helpers.

use std::path::PathBuf;
use std::sync::OnceLock;

/// Profile root: ~/.local/share/peregrine
pub fn profile_dir() -> PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        if let Ok(dir) = std::env::var("PEREGRINE_PROFILE") {
            return PathBuf::from(dir);
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".local/share/peregrine")
    })
    .clone()
}

pub fn db_path() -> PathBuf {
    profile_dir().join("peregrine.db")
}

pub fn cache_dir() -> PathBuf {
    profile_dir().join("cache")
}

pub fn filter_store_dir() -> PathBuf {
    cache_dir().join("contentfilters")
}

pub fn lists_dir() -> PathBuf {
    profile_dir().join("lists")
}

pub fn downloads_dir() -> PathBuf {
    if let Ok(d) = std::env::var("PEREGRINE_DOWNLOADS") {
        return PathBuf::from(d);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join("Downloads")
}

/// Extract the registrable-ish hostname from a URL string (best-effort, no external deps).
pub fn host_of_uri(uri: &str) -> Option<String> {
    let url = url::Url::parse(uri).ok()?;
    Some(url.host_str()?.to_string())
}

/// Base domain (last two labels, or three for common multi-part TLDs) — best effort.
pub fn base_domain(host: &str) -> String {
    let h = host.trim_start_matches('.');
    let labels: Vec<&str> = h.split('.').collect();
    if labels.len() <= 2 {
        return h.to_string();
    }
    // A tiny public-suffix heuristic: treat common second-level suffixes as part of the TLD.
    const SUFFIX2: [&str; 12] = [
        "co.uk", "org.uk", "ac.uk", "gov.uk", "co.jp", "com.au", "net.au", "org.au", "co.nz",
        "com.br", "com.cn", "co.in",
    ];
    let last2 = labels[labels.len() - 2..].join(".");
    if SUFFIX2.contains(&last2.as_str()) && labels.len() >= 3 {
        return labels[labels.len() - 3..].join(".");
    }
    last2
}

/// Percent-decode a short string (used for URI paths of internal pages).
pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() + 1 && i + 2 <= bytes.len() - 1 + 1 {
            let hex = &s[i + 1..i + 3];
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Truncate a string for display.
pub fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", t)
    }
}

/// A deterministic pleasant color for a hostname (used for letter avatars).
pub fn host_color(host: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(host.as_bytes());
    let d = h.finalize();
    let hue = (d[0] as u32 * 360) / 255;
    format!("hsl({}, 65%, 45%)", hue)
}

/// Pretty-print a byte count.
pub fn human_bytes(n: u64) -> String {
    const U: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{} {}", n, U[0])
    } else {
        format!("{:.1} {}", v, U[i])
    }
}

/// Relative "time ago" label.
pub fn time_ago(ts: i64) -> String {
    let now = chrono::Utc::now().timestamp();
    let d = now - ts;
    if d < 60 {
        "just now".into()
    } else if d < 3600 {
        format!("{} min ago", d / 60)
    } else if d < 86400 {
        format!("{} h ago", d / 3600)
    } else if d < 86400 * 30 {
        format!("{} d ago", d / 86400)
    } else {
        let dt = chrono::DateTime::from_timestamp(ts, 0);
        dt.map(|t| t.format("%Y-%m-%d").to_string()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_domain_works() {
        assert_eq!(base_domain("en.wikipedia.org"), "wikipedia.org");
        assert_eq!(base_domain("www.bbc.co.uk"), "bbc.co.uk");
        assert_eq!(base_domain("example.com"), "example.com");
    }

    #[test]
    fn host_of_uri_works() {
        assert_eq!(host_of_uri("https://a.b.io/x?y=1").as_deref(), Some("a.b.io"));
        assert_eq!(host_of_uri("about:blank"), None);
    }
}
