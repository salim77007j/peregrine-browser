//! Privacy subsystem: ad/tracker blocking, filtering proxy, fingerprint shields,
//! safe browsing, and the shared PrivacyManager that ties them together.

pub mod adblock;
pub mod fingerprint;
pub mod proxy;
pub mod safebrowsing;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::data::prefs::Prefs;

pub struct PrivacyManager {
    pub engine: Arc<adblock::AdBlockEngine>,
    pub proxy_port: std::sync::Arc<std::sync::atomic::AtomicU16>,
    pub proxy_running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    shutdown: Arc<tokio::sync::Notify>,
}

impl PrivacyManager {
    pub fn new(prefs: &Prefs) -> Self {
        let engine = adblock::AdBlockEngine::new(adblock::BlockConfig {
            ads: prefs.block_ads,
            trackers: prefs.block_trackers,
            annoyances: prefs.block_annoyances,
            malware: prefs.block_malware,
            heuristics: prefs.heuristic_blocking,
            cosmetic: prefs.cosmetic_filtering,
            custom: prefs.custom_filters.clone(),
        });
        Self {
            engine,
            proxy_port: std::sync::Arc::new(std::sync::atomic::AtomicU16::new(0)),
            proxy_running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            shutdown: Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// Start the in-process filtering proxy + engine load in background threads.
    /// Returns immediately; the port is published atomically when ready.
    pub fn start(&self) {
        let engine = self.engine.clone();
        let shutdown = self.shutdown.clone();
        let port_cell = self.proxy_port.clone();
        let running_cell = self.proxy_running.clone();
        std::thread::Builder::new()
            .name("peregrine-proxy".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_io()
                    .enable_time()
                    .build()
                    .expect("tokio runtime");
                let shutdown_park = shutdown.clone();
                rt.block_on(async move {
                    match proxy::run(engine, shutdown).await {
                        Ok(p) => {
                            port_cell.store(p.port, Ordering::SeqCst);
                            running_cell.store(true, Ordering::SeqCst);
                        }
                        Err(e) => {
                            eprintln!("peregrine: filtering proxy failed to start: {e} (traffic flows unfiltered)");
                        }
                    }
                    // park the runtime thread until shutdown
                    shutdown_park.notified().await;
                });
            })
            .ok();
        // (the engine actor thread builds its filter lists on startup by itself)
    }

    pub fn proxy_addr(&self) -> Option<String> {
        if self.proxy_running.load(Ordering::Relaxed) {
            let p = self.proxy_port.load(Ordering::Relaxed);
            if p != 0 {
                return Some(format!("http://127.0.0.1:{p}"));
            }
        }
        None
    }

    pub fn stats(&self) -> adblock::BlockStats {
        self.engine.stats_snapshot()
    }
}

/// Convert prefs → proxy/network configuration summary for logging.
pub fn describe_protection(prefs: &Prefs) -> String {
    format!(
        "ads={} trackers={} annoyances={} malware={} heuristics={} cosmetic={} cookies={} itp={} fp={} webrtc={}",
        prefs.block_ads,
        prefs.block_trackers,
        prefs.block_annoyances,
        prefs.block_malware,
        prefs.heuristic_blocking,
        prefs.cosmetic_filtering,
        prefs.cookie_policy,
        prefs.enable_itp,
        prefs.fingerprint_shield,
        prefs.webrtc_leak_protection
    )
}
