//! Ad/tracker blocking engine manager.
//!
//! Brave's `adblock` engine contains `Rc` internals and is therefore NOT `Send`.
//! We confine it to a dedicated engine thread (the actor pattern): every query —
//! from the filtering proxy or the UI thread — is a channel round-trip. This is
//! also how Brave ships it in production.
//!
//! Capabilities:
//! - bundled + updated filter lists (EasyList, EasyPrivacy, Peter Lowe, Fanboy Annoyance, URLhaus)
//! - user custom filters (ABP syntax)
//! - per-request network decisions used by the filtering proxy
//! - cosmetic resources (per-URL hide selectors, generic class/id rules, scriptlets)
//! - live stats aggregation

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex, RwLock};

use crate::util;

/// A named upstream filter list.
#[derive(Clone)]
pub struct ListSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub url: &'static str,
    pub bundled: &'static str, // gzipped file in assets/filters
}

pub const LISTS: &[ListSpec] = &[
    ListSpec {
        id: "easylist",
        name: "EasyList (ads)",
        url: "https://easylist.to/easylist/easylist.txt",
        bundled: "easylist.txt.gz",
    },
    ListSpec {
        id: "easyprivacy",
        name: "EasyPrivacy (trackers)",
        url: "https://easylist.to/easylist/easyprivacy.txt",
        bundled: "easyprivacy.txt.gz",
    },
    ListSpec {
        id: "peterlowe",
        name: "Peter Lowe's ad servers",
        url: "https://pgl.yoyo.org/adservers/serverlist.php?hostformat=adblock&showintro=0&mimetype=plaintext",
        bundled: "peterlowe.txt.gz",
    },
    ListSpec {
        id: "annoyance",
        name: "Fanboy Annoyance (overlays & nuisances)",
        url: "https://easylist.to/easylist/fanboy-annoyance.txt",
        bundled: "fanboy-annoyance.txt.gz",
    },
];

#[derive(Default, Debug, Clone, serde::Serialize)]
pub struct BlockStats {
    pub blocked_requests: i64,
    pub ads_blocked: i64,
    pub trackers_blocked: i64,
    pub annoyances_blocked: i64,
    pub malware_blocked: i64,
    pub cosmetic_hidden: i64,
    pub fingerprint_attempts: i64,
    pub bytes_saved: u64,
}

/// Which categories are enabled right now (mirrors prefs).
#[derive(Clone, Debug)]
pub struct BlockConfig {
    pub ads: bool,
    pub trackers: bool,
    pub annoyances: bool,
    pub malware: bool,
    pub heuristics: bool,
    pub cosmetic: bool,
    pub custom: String,
}

impl Default for BlockConfig {
    fn default() -> Self {
        Self {
            ads: true,
            trackers: true,
            annoyances: true,
            malware: true,
            heuristics: true,
            cosmetic: true,
            custom: String::new(),
        }
    }
}

#[derive(Clone, Default, serde::Serialize)]
pub struct ListStatus {
    pub rules: u64,
    pub updated_at: Option<i64>,
    pub source: String, // "bundled" | "network"
}

#[derive(Clone, Default)]
pub struct BlockStatsSnapshot {
    pub blocked_requests: Arc<AtomicI64>,
    pub ads_blocked: Arc<AtomicI64>,
    pub trackers_blocked: Arc<AtomicI64>,
    pub annoyances_blocked: Arc<AtomicI64>,
    pub malware_blocked: Arc<AtomicI64>,
    pub cosmetic_hidden: Arc<AtomicI64>,
    pub fingerprint_attempts: Arc<AtomicI64>,
    pub bytes_saved: Arc<AtomicU64>,
}

impl BlockStatsSnapshot {
    pub fn read(&self) -> BlockStats {
        BlockStats {
            blocked_requests: self.blocked_requests.load(Ordering::Relaxed),
            ads_blocked: self.ads_blocked.load(Ordering::Relaxed),
            trackers_blocked: self.trackers_blocked.load(Ordering::Relaxed),
            annoyances_blocked: self.annoyances_blocked.load(Ordering::Relaxed),
            malware_blocked: self.malware_blocked.load(Ordering::Relaxed),
            cosmetic_hidden: self.cosmetic_hidden.load(Ordering::Relaxed),
            fingerprint_attempts: self.fingerprint_attempts.load(Ordering::Relaxed),
            bytes_saved: self.bytes_saved.load(Ordering::Relaxed),
        }
    }
}

/// Messages to the engine actor thread.
enum EngineMsg {
    Check {
        url: String,
        host: String,
        resp: Sender<Option<&'static str>>,
    },
    Cosmetic {
        url: String,
        resp: Sender<Option<adblock::cosmetic_filter_cache::UrlSpecificResources>>,
    },
    Generic {
        classes: Vec<String>,
        ids: Vec<String>,
        exceptions: std::collections::HashSet<String>,
        resp: Sender<Vec<String>>,
    },
    Rebuild(BlockConfig),
    UpdateList {
        id: String,
        resp: Sender<(u64, bool)>,
    },
}

/// The thread-safe handle to the engine actor.
pub struct AdBlockEngine {
    tx: Sender<EngineMsg>,
    pub stats: BlockStatsSnapshot,
    per_host: Arc<RwLock<HashMap<String, i64>>>,
    config: RwLock<BlockConfig>,
    pub rules_loaded: Arc<AtomicU64>,
    pub lists_status: Arc<Mutex<HashMap<String, ListStatus>>>,
    ready: Arc<AtomicBool>,
}

impl AdBlockEngine {
    pub fn new(config: BlockConfig) -> Arc<Self> {
        let (tx, rx) = channel::<EngineMsg>();
        let cfg0 = config.clone();
        let stats = BlockStatsSnapshot::default();
        let stats_thread = stats.clone();
        let per_host: Arc<RwLock<HashMap<String, i64>>> = Arc::new(RwLock::new(HashMap::new()));
        let per_host_thread = per_host.clone();
        let lists_status: Arc<Mutex<HashMap<String, ListStatus>>> = Arc::new(Mutex::new(HashMap::new()));
        let lists_thread = lists_status.clone();
        let rules_loaded: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));
        let rules_thread = rules_loaded.clone();
        let ready: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
        let ready_thread = ready.clone();

        // ---- the engine actor thread (owns the !Send adblock::Engine) ----
        std::thread::Builder::new()
            .name("peregrine-adblock".into())
            .spawn(move || {
                let mut cfg = cfg0;
                let mut engine: Option<adblock::Engine> = None;
                rebuild(&mut engine, &cfg, &lists_thread, &rules_thread);
                ready_thread.store(true, Ordering::Release);
                while let Ok(msg) = rx.recv() {
                    match msg {
                        EngineMsg::Rebuild(c) => {
                            cfg = c;
                            rebuild(&mut engine, &cfg, &lists_thread, &rules_thread);
                            ready_thread.store(engine.is_some(), Ordering::Release);
                        }
                        EngineMsg::Check { url, host, resp } => {
                            let verdict = engine.as_ref().map(|e| check(e, &cfg, &url, &host)).flatten();
                            if let Some(cat) = verdict {
                                stats_thread.blocked_requests.fetch_add(1, Ordering::Relaxed);
                                match cat {
                                    "ad" => { stats_thread.ads_blocked.fetch_add(1, Ordering::Relaxed); }
                                    "tracker" => { stats_thread.trackers_blocked.fetch_add(1, Ordering::Relaxed); }
                                    "annoyance" => { stats_thread.annoyances_blocked.fetch_add(1, Ordering::Relaxed); }
                                    "malware" => { stats_thread.malware_blocked.fetch_add(1, Ordering::Relaxed); }
                                    _ => {}
                                }
                                let mut ph = per_host_thread.write().unwrap();
                                *ph.entry(host.clone()).or_insert(0) += 1;
                            }
                            let _ = resp.send(verdict);
                        }
                        EngineMsg::Cosmetic { url, resp } => {
                            let res = engine.as_ref().map(|e| e.url_cosmetic_resources(&url));
                            let _ = resp.send(res);
                        }
                        EngineMsg::Generic { classes, ids, exceptions, resp } => {
                            let res = engine
                                .as_ref()
                                .map(|e| e.hidden_class_id_selectors(classes, ids, &exceptions))
                                .unwrap_or_default();
                            let _ = resp.send(res);
                        }
                        EngineMsg::UpdateList { id, resp } => {
                            let (rules, ok) = update_list(&id).unwrap_or((0, false));
                            if ok {
                                let mut m = lists_thread.lock().unwrap();
                                if let Some(st) = m.get_mut(&id) {
                                    st.rules = rules;
                                    st.updated_at = Some(chrono::Utc::now().timestamp());
                                    st.source = "network".into();
                                }
                            }
                            let _ = resp.send((rules, ok));
                        }
                    }
                }
            })
            .expect("adblock actor thread");

        Arc::new(Self {
            tx,
            stats,
            per_host,
            config: RwLock::new(config),
            rules_loaded,
            lists_status,
            ready,
        })
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    /// Decide whether a request should be blocked (channel round-trip, ~50µs).
    pub fn check_network(&self, url: &str, host: &str) -> Option<&'static str> {
        let (resp_tx, resp_rx) = channel();
        if self.tx.send(EngineMsg::Check {
            url: url.to_string(),
            host: host.to_string(),
            resp: resp_tx,
        })
        .is_err()
        {
            return None;
        }
        resp_rx.recv().ok().flatten()
    }

    /// Record a block decision made elsewhere (e.g. safe-browsing interstitial).
    pub fn record_block(&self, host: &str, category: &str, bytes: u64) {
        self.stats.blocked_requests.fetch_add(1, Ordering::Relaxed);
        match category {
            "ad" => { self.stats.ads_blocked.fetch_add(1, Ordering::Relaxed); }
            "tracker" => { self.stats.trackers_blocked.fetch_add(1, Ordering::Relaxed); }
            "annoyance" => { self.stats.annoyances_blocked.fetch_add(1, Ordering::Relaxed); }
            "malware" => { self.stats.malware_blocked.fetch_add(1, Ordering::Relaxed); }
            _ => {}
        }
        self.stats.bytes_saved.fetch_add(bytes, Ordering::Relaxed);
        let mut per_host = self.per_host.write().unwrap();
        *per_host.entry(host.to_string()).or_insert(0) += 1;
    }

    pub fn record_cosmetic(&self, count: i64) {
        if count > 0 {
            self.stats.cosmetic_hidden.fetch_add(count, Ordering::Relaxed);
        }
    }

    pub fn record_fingerprint(&self) {
        self.stats.fingerprint_attempts.fetch_add(1, Ordering::Relaxed);
    }

    /// Top blocked hosts for the dashboard.
    pub fn top_blocked_hosts(&self, limit: usize) -> Vec<(String, i64)> {
        let per_host = self.per_host.read().unwrap();
        let mut v: Vec<(String, i64)> = per_host.iter().map(|(k, c)| (k.clone(), *c)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v.truncate(limit);
        v
    }

    /// Cosmetic resources for a URL (uBO-style).
    pub fn cosmetic_for(&self, url: &str) -> Option<adblock::cosmetic_filter_cache::UrlSpecificResources> {
        let (resp_tx, resp_rx) = channel();
        if self.tx.send(EngineMsg::Cosmetic { url: url.to_string(), resp: resp_tx }).is_err() {
            return None;
        }
        resp_rx.recv().ok().flatten()
    }

    /// Generic cosmetic selectors for the classes/ids found on a page.
    pub fn generic_cosmetic(
        &self,
        classes: Vec<String>,
        ids: Vec<String>,
        exceptions: &std::collections::HashSet<String>,
    ) -> Option<Vec<String>> {
        let (resp_tx, resp_rx) = channel();
        if self
            .tx
            .send(EngineMsg::Generic {
                classes,
                ids,
                exceptions: exceptions.clone(),
                resp: resp_tx,
            })
            .is_err()
        {
            return None;
        }
        resp_rx.recv().ok()
    }

    /// Rebuild the engine after prefs change.
    pub fn rebuild_with(&self, config: BlockConfig) {
        *self.config.write().unwrap() = config.clone();
        let _ = self.tx.send(EngineMsg::Rebuild(config));
    }

    /// Update one list from the network. Returns (rules, ok).
    pub fn update_list(&self, id: &str) -> (u64, bool) {
        let (resp_tx, resp_rx) = channel();
        if self.tx.send(EngineMsg::UpdateList { id: id.to_string(), resp: resp_tx }).is_err() {
            return (0, false);
        }
        resp_rx.recv().unwrap_or((0, false))
    }

    pub fn stats_snapshot(&self) -> BlockStats {
        self.stats.read()
    }

    pub fn config_snapshot(&self) -> BlockConfig {
        self.config.read().unwrap().clone()
    }
}

// ---------------------------------------------------------------------------
// engine-thread internals
// ---------------------------------------------------------------------------

fn rebuild(
    engine: &mut Option<adblock::Engine>,
    cfg: &BlockConfig,
    lists: &Mutex<HashMap<String, ListStatus>>,
    rules_out: &AtomicU64,
) {
    use adblock::lists::{FilterFormat, FilterSet, ParseOptions};
    let mut set = FilterSet::new(false);
    let mut total = 0u64;
    for spec in LISTS {
        let enabled = match spec.id {
            "easylist" => cfg.ads,
            "easyprivacy" => cfg.trackers,
            "annoyance" => cfg.annoyances,
            _ => true,
        };
        if !enabled {
            continue;
        }
        // prefer a network-updated copy in the profile, else the bundled snapshot
        let updated = std::fs::read_to_string(util::lists_dir().join(format!("{}.txt", spec.id))).ok();
        let from_network = updated.is_some();
        let text = updated.or_else(|| read_bundled(spec.bundled));
        if let Some(text) = text {
            let n = count_rules(&text);
            set.add_filter_list(
                text,
                ParseOptions {
                    format: FilterFormat::Standard,
                    ..Default::default()
                },
            );
            total += n;
            lists.lock().unwrap().insert(
                spec.id.to_string(),
                ListStatus {
                    rules: n,
                    updated_at: None,
                    source: if from_network { "network".into() } else { "bundled".into() },
                },
            );
        }
    }
    if cfg.malware {
        if let Some(text) = read_bundled("urlhaus.txt.gz") {
            let n = count_rules(&text);
            set.add_filter_list(
                text,
                ParseOptions {
                    format: FilterFormat::Hosts,
                    ..Default::default()
                },
            );
            total += n;
            lists.lock().unwrap().insert(
                "urlhaus".to_string(),
                ListStatus { rules: n, updated_at: None, source: "bundled".into() },
            );
        }
    }
    if !cfg.custom.trim().is_empty() {
        let n = count_rules(&cfg.custom);
        set.add_filter_list(
            cfg.custom.clone(),
            ParseOptions {
                format: FilterFormat::Standard,
                ..Default::default()
            },
        );
        total += n;
    }
    *engine = Some(adblock::Engine::new_with_filter_set(set));
    rules_out.store(total, Ordering::Relaxed);
}

fn check(engine: &adblock::Engine, cfg: &BlockConfig, url: &str, host: &str) -> Option<&'static str> {
    if !cfg.ads && !cfg.trackers && !cfg.malware && !cfg.annoyances {
        return None;
    }
    let req = match adblock::request::Request::new(url, "https://example.invalid/", "sub_frame", "get") {
        Ok(r) => r,
        Err(_) => return None,
    };
    let result = engine.check_network_request(&req);
    if result.filter.is_some() && result.exception.is_none() {
        return Some(classify(host));
    }
    if cfg.malware && is_malware_host(host) {
        return Some("malware");
    }
    if cfg.heuristics && heuristic_tracker(host) {
        return Some("tracker");
    }
    None
}

fn classify(host: &str) -> &'static str {
    let h = host.to_ascii_lowercase();
    if is_malware_host(&h) {
        return "malware";
    }
    const AD_HINTS: [&str; 14] = [
        "ad", "ads", "adsrv", "adserver", "advert", "adservice", "doubleclick", "adsystem",
        "adnxs", "adsense", "banner", "popads", "popcash", "taboola",
    ];
    const TRACKER_HINTS: [&str; 18] = [
        "analytics", "telemetry", "track", "tracker", "tracking", "metrics", "stats", "beacon",
        "pixel", "counter", "scorecardresearch", "google-analytics", "hotjar", "mixpanel",
        "segment", "quantserve", "chartbeat", "optimizely",
    ];
    let first = h.split('.').next().unwrap_or("");
    for hint in AD_HINTS {
        if first.contains(hint) {
            return "ad";
        }
    }
    for hint in TRACKER_HINTS {
        if first.contains(hint) {
            return "tracker";
        }
    }
    if h.contains("doubleclick") || h.contains("adsystem") {
        return "ad";
    }
    "other"
}

fn malware_hosts() -> &'static std::collections::HashSet<String> {
    static SET: std::sync::OnceLock<std::collections::HashSet<String>> = std::sync::OnceLock::new();
    SET.get_or_init(|| {
        let mut s = std::collections::HashSet::new();
        if let Some(text) = read_bundled("urlhaus.txt.gz") {
            for line in text.lines() {
                let l = line.trim();
                if l.starts_with('#') || l.is_empty() {
                    continue;
                }
                if let Ok(u) = url::Url::parse(l) {
                    if let Some(h) = u.host_str() {
                        s.insert(h.to_string());
                    }
                }
            }
        }
        s
    })
}

pub fn is_malware_host(host: &str) -> bool {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    malware_hosts().contains(&h)
}

fn heuristic_tracker(host: &str) -> bool {
    let h = host.to_ascii_lowercase();
    let first = h.split('.').next().unwrap_or("");
    matches!(
        first,
        "telemetry" | "analytics" | "metrics" | "stats" | "beacon" | "pix" | "pixel" | "track" | "tracking" | "insights"
    )
}

fn count_rules(text: &str) -> u64 {
    text.lines().filter(|l| {
        let t = l.trim();
        !t.is_empty() && !t.starts_with('!') && !t.starts_with('#') && !t.starts_with("// ")
    }).count() as u64
}

fn read_bundled(name: &str) -> Option<String> {
    let bytes: &[u8] = match name {
        "easylist.txt.gz" => include_bytes!("../../assets/filters/easylist.txt.gz").as_slice(),
        "easyprivacy.txt.gz" => include_bytes!("../../assets/filters/easyprivacy.txt.gz").as_slice(),
        "peterlowe.txt.gz" => include_bytes!("../../assets/filters/peterlowe.txt.gz").as_slice(),
        "fanboy-annoyance.txt.gz" => include_bytes!("../../assets/filters/fanboy-annoyance.txt.gz").as_slice(),
        "urlhaus.txt.gz" => include_bytes!("../../assets/filters/urlhaus.txt.gz").as_slice(),
        _ => return None,
    };
    use flate2::read::GzDecoder;
    let mut out = String::new();
    std::io::Read::read_to_string(&mut GzDecoder::new(bytes), &mut out).ok()?;
    Some(out)
}

fn update_list(id: &str) -> Option<(u64, bool)> {
    let spec = LISTS.iter().find(|s| s.id == id)?;
    match ureq::get(spec.url).call() {
        Ok(resp) => {
            let mut text = String::new();
            use std::io::Read as _;
            let mut limited = resp.into_reader().take(40 * 1024 * 1024);
            match std::io::Read::read_to_string(&mut limited, &mut text) {
                Ok(_) if !text.is_empty() => {
                    let dir = util::lists_dir();
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = std::fs::write(dir.join(format!("{id}.txt")), &text);
                    Some((count_rules(&text), true))
                }
                _ => Some((0, false)),
            }
        }
        Err(_) => Some((0, false)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_custom_rules_via_actor() {
        let e = AdBlockEngine::new(BlockConfig {
            custom: "||ads.example.com^\n||tracker.example.net^\n###ad-banner\n".into(),
            ..Default::default()
        });
        // wait for engine readiness
        for _ in 0..100 {
            if e.is_ready() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(e.is_ready());
        assert_eq!(e.check_network("https://ads.example.com/banner.js", "ads.example.com"), Some("ad"));
        assert_eq!(e.check_network("https://tracker.example.net/pixel.gif", "tracker.example.net"), Some("tracker"));
        assert_eq!(e.check_network("https://example.org/", "example.org"), None);
    }

    #[test]
    fn heuristic_catches_telemetry() {
        assert!(heuristic_tracker("telemetry.vendor.io"));
        assert!(!heuristic_tracker("docs.python.org"));
    }
}

#[cfg(test)]
mod cosmetic_tests {
    use super::*;

    #[test]
    fn generic_cosmetic_finds_custom_id_rule() {
        let e = AdBlockEngine::new(BlockConfig {
            custom: "###ad-banner\n##.ad-slot\n".into(),
            ..Default::default()
        });
        for _ in 0..200 {
            if e.is_ready() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let mut ids = vec!["ad-banner".to_string()];
        let classes: Vec<String> = vec![];
        let mut exceptions = std::collections::HashSet::new();
        // seed exceptions from the url resources
        if let Some(res) = e.cosmetic_for("http://x.test/page.html") {
            exceptions = res.exceptions.clone();
        }
        let out = e.generic_cosmetic(classes, ids, &exceptions);
        assert!(out.is_some(), "generic_cosmetic returned None");
        let out = out.unwrap();
        eprintln!("generic selectors: {:?}", out);
        assert!(out.iter().any(|s| s.contains("ad-banner")), "no #ad-banner in {:?}", out);
    }
}
