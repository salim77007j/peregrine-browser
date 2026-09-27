//! Headless self-test mode (`peregrine --self-test`): drives the real browser UI
//! under Xvfb, loads local test pages, and verifies privacy systems + core
//! behaviors end-to-end. Used by CI and sandbox validation.

use std::sync::Arc;

use gtk4::glib;
use gtk4::prelude::*;
use webkit6::prelude::*;

use crate::app::App;
use crate::window::BrowserWindow;

pub struct TestReport {
    pub checks: Vec<(String, bool, String)>,
}

impl TestReport {
    fn add(&mut self, name: &str, ok: bool, detail: String) {
        println!("  {} {} — {}", if ok { "PASS" } else { "FAIL" }, name, detail);
        self.checks.push((name.to_string(), ok, detail));
    }
    pub fn passed(&self) -> usize {
        self.checks.iter().filter(|c| c.1).count()
    }
    pub fn total(&self) -> usize {
        self.checks.len()
    }
    pub fn summary_json(&self) -> String {
        serde_json::json!({
            "passed": self.passed(),
            "total": self.total(),
            "checks": self.checks.iter().map(|(n, ok, d)| serde_json::json!({
                "name": n, "ok": ok, "detail": d
            })).collect::<Vec<_>>(),
        })
        .to_string()
    }
}

/// A tiny local HTTP server for deterministic offline tests.
fn serve_test_pages() -> u16 {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 2048];
            let Ok(n) = stream.read(&mut buf) else { continue };
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
            let (body, mime) = match path.as_str() {
                "/adtest.html" => (
                    AD_TEST_PAGE.to_string(),
                    "text/html",
                ),
                "/fp.html" => (FINGERPRINT_PAGE.to_string(), "text/html"),
                _ => (
                    format!(
                        "<html><body><h1>Test page {}</h1><p>Peregrine local test server.</p></body></html>",
                        path
                    ),
                    "text/html",
                ),
            };
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                mime,
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    port
}

pub fn run(app: Arc<App>) -> i32 {
    println!("peregrine: self-test starting");

    let port = serve_test_pages();
    let report = Arc::new(std::sync::Mutex::new(TestReport { checks: vec![] }));
    let win = match crate::window_ops::open_new_window(&app, Some("peregrine://newtab")) {
        Some(w) => w,
        None => {
            eprintln!("self-test: could not open window");
            return 2;
        }
    };

    // state machine steps
    #[derive(PartialEq)]
    enum Step {
        NewTab,
        AdTest,
        FpTest,
        PrivacyPage,
        TabsExercise,
        Done,
    }
    let step = Arc::new(std::sync::Mutex::new(Step::NewTab));
    let start = std::time::Instant::now();

    let report_t = report.clone();
    let step_t = step.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(400), { let win = win.static_ref(); let app = app.clone(); move || {
        let mut rep = report_t.lock().unwrap();
        let mut st = step_t.lock().unwrap();
        let elapsed = start.elapsed().as_secs();
        match *st {
            Step::NewTab => {
                // verify new tab page rendered
                let title = win.current_title();
                let ok = title.contains("New tab") || title.contains("Peregrine");
                rep.add("new-tab page", ok, format!("title='{}'", title));
                // navigate to ad test
                win.navigate(&format!("http://127.0.0.1:{port}/adtest.html"));
                *st = Step::AdTest;
            }
            Step::AdTest => {
                if elapsed < 4 && win.current_webview().map(|w| w.is_loading()).unwrap_or(true) {
                    return glib::ControlFlow::Continue;
                }
                let stats = app.privacy.stats();
                let ok = stats.blocked_requests >= 1;
                rep.add(
                    "network ad blocking",
                    ok,
                    format!("blocked_requests={} (ads={})", stats.blocked_requests, stats.ads_blocked),
                );
                // cosmetic filtering check — evaluated ON the ad test page
                if let Some(wv) = win.current_webview() {
                    wv.evaluate_javascript(
                        "(function(){
                            var el = document.getElementById('ad-banner');
                            var el2 = document.querySelector('.ad-slot');
                            var cosmetic = !!el && getComputedStyle(el).display === 'none';
                            var cosmetic2 = !!el2 && getComputedStyle(el2).display === 'none';
                            return JSON.stringify({c1: cosmetic, c2: cosmetic2});
                        })()",
                        None, None, None::<&gtk4::gio::Cancellable>,
                        glib::clone!(#[strong] report_t, move |res| {
                            let mut rep = report_t.lock().unwrap();
                            match res {
                                Ok(v) => {
                                    let g = v.to_string();
                                    let s = g.as_str().to_string();
                                    if let Ok(j) = serde_json::from_str::<serde_json::Value>(&s) {
                                        let c1 = j.get("c1").and_then(|x| x.as_bool()).unwrap_or(false);
                                        let c2 = j.get("c2").and_then(|x| x.as_bool()).unwrap_or(false);
                                        rep.add("cosmetic filtering", c1 && c2, format!("###ad-banner hidden={} .ad-slot hidden={}", c1, c2));
                                    }
                                }
                                Err(e) => rep.add("cosmetic filtering", false, e.to_string()),
                            }
                        }),
                    );
                }
                win.navigate(&format!("http://127.0.0.1:{port}/fp.html"));
                *st = Step::FpTest;
            }
            Step::FpTest => {
                if elapsed < 6 && win.current_webview().map(|w| w.is_loading()).unwrap_or(true) {
                    return glib::ControlFlow::Continue;
                }
                *st = Step::PrivacyPage;
            }
            Step::PrivacyPage => {
                if elapsed < 8 {
                    return glib::ControlFlow::Continue;
                }
                // cosmetic + fingerprint assertions via page JS
                if let Some(wv) = win.current_webview() {
                    let wv2 = wv.clone();
                    wv.evaluate_javascript(
                        "(function(){
                            var c = document.createElement('canvas'); c.width=64; c.height=8;
                            var ctx = c.getContext('2d'); ctx.fillStyle='#c33'; ctx.fillRect(0,0,64,8);
                            var d1 = c.toDataURL(); var d2 = c.toDataURL();
                            return JSON.stringify({canvasFarbling: d1 !== d2,
                                hc: navigator.hardwareConcurrency, dnt: navigator.doNotTrack});
                        })()",
                        None, None, None::<&gtk4::gio::Cancellable>,
                        glib::clone!(#[strong] report_t, move |res| {
                            let mut rep = report_t.lock().unwrap();
                            match res {
                                Ok(v) => {
                                    let s = v.to_string().to_string();
                                    if let Ok(j) = serde_json::from_str::<serde_json::Value>(&s) {
                                        let far = j.get("canvasFarbling").and_then(|x| x.as_bool()).unwrap_or(false);
                                        rep.add("canvas fingerprint farbling", far, "readbacks differ under noise".into());
                                        let hc = j.get("hc").and_then(|x| x.as_i64()).unwrap_or(0);
                                        let dnt = j.get("dnt").and_then(|x| x.as_str()).unwrap_or("").to_string();
                                        rep.add("navigator hardening", hc == 8 || hc == 4, format!("hardwareConcurrency={hc} doNotTrack={dnt}"));
                                    } else {
                                        rep.add("page JS evaluation", false, format!("raw: {s}"));
                                    }
                                }
                                Err(e) => rep.add("page JS evaluation", false, e.to_string()),
                            }
                        }),
                    );
                    let _ = wv2;
                }
                // privacy dashboard renders
                win.navigate("peregrine://privacy");
                *st = Step::TabsExercise;
            }
            Step::TabsExercise => {
                if elapsed < 10 {
                    return glib::ControlFlow::Continue;
                }
                let title = win.current_title();
                let uri = win.current_uri();
                rep.add("privacy dashboard page", uri.contains("peregrine://privacy") || title.contains("Privacy"), format!("uri='{uri}' title='{title}'"));
                // tabs: open, close, count
                let before = win.tab_view.n_pages();
                win.new_tab_url("peregrine://settings");
                win.new_tab_url("peregrine://bookmarks");
                let after = win.tab_view.n_pages();
                rep.add("tab open", after == before + 2, format!("{before} → {after}"));
                win.close_current_tab();
                let after_close = win.tab_view.n_pages();
                rep.add("tab close", after_close == after - 1, format!("{after} → {after_close}"));
                win.reopen_closed_tab();
                let after_reopen = win.tab_view.n_pages();
                rep.add("tab reopen (Ctrl+Shift+T)", after_reopen == after_close + 1, format!("{after_close} → {after_reopen}"));

                // internal pages render
                let s_uri = win.current_uri();
                rep.add("internal pages render", s_uri.starts_with("peregrine://"), format!("uri='{s_uri}'"));

                // session save/load roundtrip
                app.save_session();
                let sess = app.load_session();
                let ok = sess
                    .as_ref()
                    .and_then(|s| s.get("windows"))
                    .and_then(|w| w.as_array())
                    .map(|a| !a.is_empty())
                    .unwrap_or(false);
                rep.add("session persistence", ok, "session.json written with tabs".into());

                // stats present
                let stats = app.privacy.stats();
                rep.add(
                    "privacy stats aggregation",
                    stats.blocked_requests >= 1 || stats.cosmetic_hidden >= 1,
                    format!("blocked={} cosmetic={}", stats.blocked_requests, stats.cosmetic_hidden),
                );
                *st = Step::Done;
            }
            Step::Done => {
                return glib::ControlFlow::Continue;
            }
        }
        glib::ControlFlow::Continue
    } });

    // finish after 14s
    let app2 = app.clone();
    glib::timeout_add_local(std::time::Duration::from_secs(14), glib::clone!(#[strong] app, #[strong] report, move || {
        let rep = report.lock().unwrap();
        let json = rep.summary_json();
        let _ = std::fs::write("/tmp/peregrine-selftest.json", &json);
        println!("\nperegrine: self-test {} — {} / {} checks passed",
            if rep.passed() == rep.total() { "PASSED" } else { "FAILED" }, rep.passed(), rep.total());
        let code = if rep.passed() == rep.total() { 0 } else { 1 };
        let _ = app2;
        std::process::exit(code);
    }));

    // keep the window reference alive for the duration
    std::mem::forget(win);
    0
}

const AD_TEST_PAGE: &str = r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>AdBlock Test</title></head>
<body>
<h1>Ad-block test page</h1>
<div id="ad-banner" style="width:728px;height:90px;background:#f90">BUY NOW AD SLOT</div>
<div class="ad-slot" style="width:300px;height:250px;background:#f09">SIDEBAR AD</div>
<p>This page loads a known tracker script; the filtering proxy must block it.</p>
<script src="http://doubleclick.net/invisibility.js"></script>
<script src="http://www.google-analytics.com/analytics.js"></script>
<img src="http://ad.doubleclick.net/pixel.gif" width="1" height="1">
</body></html>"#;

const FINGERPRINT_PAGE: &str = r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>Fingerprint Test</title></head>
<body>
<h1>Fingerprint shield test page</h1>
<canvas id="c" width="200" height="40"></canvas>
<p>Probe farbling: canvas readback noise, audio noise, WebGL masking.</p>
<script>
(function(){
  var c = document.getElementById('c');
  var ctx = c.getContext('2d');
  ctx.fillStyle = '#c33'; ctx.fillRect(0, 0, 200, 40);
  ctx.fillStyle = '#3c3';
  ctx.beginPath(); ctx.arc(100, 20, 15, 0, 7); ctx.fill();
})();
</script>
</body></html>"#;
