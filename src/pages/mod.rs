//! Internal pages: the `peregrine://` URI scheme + JSON-RPC bridge between
//! internal pages (newtab, settings, privacy, bookmarks, history, downloads)
//! and the browser core.
//!
//! RPC protocol (both directions):
//!   page → core:  window.webkit.messageHandlers.bridge.postMessage(JSON.stringify({id, action, params}))
//!   core → page:  window.__rpc(id, resultJsonString)   (error → {__error: "..."})

pub mod bookmarks_page;
pub mod downloads_page;
pub mod history_page;
pub mod newtab;
pub mod privacy_dash;
pub mod settings;
pub mod shared;

use std::sync::Arc;

use gtk4::gio::MemoryInputStream;
use gtk4::glib;
use gtk4::glib::Bytes as GBytes;
use webkit6::prelude::*;
use webkit6::{URISchemeRequest, WebContext};

use crate::app::App;

/// Well-known internal routes.
pub const ROUTES: &[&str] = &[
    "newtab", "settings", "privacy", "bookmarks", "history", "downloads", "blocked", "error",
];

/// Global app handle — the scheme handler must be registered BEFORE the
/// NetworkSession exists (WebKitGTK routes scheme IPC per-network-process),
/// while `Arc<App>` only materializes later.
///
/// Safety: the pointer is published once from the main thread before any view
/// exists, and the scheme callback is only ever invoked on the GTK main thread.
struct AppPtr(Arc<App>);
unsafe impl Send for AppPtr {}
unsafe impl Sync for AppPtr {}
static APP: std::sync::OnceLock<AppPtr> = std::sync::OnceLock::new();

pub fn publish_app(app: Arc<App>) {
    let _ = APP.set(AppPtr(app));
}

pub fn register(context: &WebContext) {
    // Treat the scheme like a secure local document so fetch/XHR/mixed-content behave.
    let sm = context.security_manager().expect("security manager");
    sm.register_uri_scheme_as_secure("peregrine");
    sm.register_uri_scheme_as_cors_enabled("peregrine");
    sm.register_uri_scheme_as_local("peregrine");
    sm.register_uri_scheme_as_no_access("peregrine");

    context.register_uri_scheme("peregrine", |request: &URISchemeRequest| {
        if let Some(ptr) = APP.get() {
            let app: Arc<App> = ptr.0.clone();
            handle_request(app, request);
        }
    });
}

fn handle_request(app: Arc<App>, request: &URISchemeRequest) {
    let uri = request.uri().map(|u| u.to_string()).unwrap_or_default();
    // NOTE: for custom schemes WebKit parses `peregrine://privacy` with
    // "privacy" as the URI *authority* (host), not the path — so we derive the
    // route from the URI string itself.
    let after_scheme = uri
        .strip_prefix("peregrine://")
        .or_else(|| uri.strip_prefix("peregrine:"))
        .unwrap_or("");
    let route_raw = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .to_string();
    let route = crate::util::percent_decode(&route_raw);
    let route = if route.is_empty() { "newtab".to_string() } else { route };
    let query = uri.split_once('?').map(|(_, q)| q.to_string()).unwrap_or_default();

    let (html, mime) = match route.as_str() {
        "newtab" => crate::pages::newtab::render(&app),
        "settings" => crate::pages::settings::render(&app),
        "privacy" => crate::pages::privacy_dash::render(&app),
        "bookmarks" => crate::pages::bookmarks_page::render(&app),
        "history" => crate::pages::history_page::render(&app),
        "downloads" => crate::pages::downloads_page::render(&app),
        "version" => (format!("<pre>Peregrine {}</pre>", env!("CARGO_PKG_VERSION")), "text/html"),
        _ => crate::pages::newtab::render(&app),
    };
    let _ = query;
    let bytes = GBytes::from_owned(html.into_bytes());
    let stream = MemoryInputStream::from_bytes(&bytes);
    request.finish(&stream, bytes.len() as i64, Some(mime));
}

/// JSON helpers for RPC responses.
pub fn ok_json(v: serde_json::Value) -> String {
    v.to_string()
}
pub fn err_json(msg: &str) -> String {
    serde_json::json!({ "__error": msg }).to_string()
}

/// Handle an RPC message coming from an internal page.
pub fn handle_rpc(app: Arc<App>, webview: &webkit6::WebView, raw: &str) {
    let v: serde_json::Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(e) => {
            reply(webview, 0, &err_json(&format!("bad rpc: {e}")));
            return;
        }
    };
    let id = v.get("id").and_then(|x| x.as_i64()).unwrap_or(0);
    let action = v.get("action").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let params = v.get("params").cloned().unwrap_or(serde_json::Value::Null);

    let result = dispatch(&app, &action, &params);
    let payload = match result {
        Ok(val) => val.to_string(),
        Err(e) => err_json(&e),
    };
    reply(webview, id, &payload);
}

fn reply(webview: &webkit6::WebView, id: i64, payload: &str) {
    let js = format!(
        "try {{ window.__rpc && window.__rpc({}, {}); }} catch (e) {{}}",
        id, payload
    );
    webview.evaluate_javascript(&js, None, None, None::<&gtk4::gio::Cancellable>, |_| {});
}

/// Escape a string for embedding in generated HTML.
pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn dispatch(app: &Arc<App>, action: &str, p: &serde_json::Value) -> Result<serde_json::Value, String> {
    let data = &app.data;
    match action {
        // ---- common ----
        "prefs.get" => {
            let prefs = data.prefs.get();
            serde_json::to_value(&prefs).map_err(|e| e.to_string())
        }
        "prefs.set" => {
            let key = p.get("key").and_then(|x| x.as_str()).ok_or("key required")?;
            let value = p
                .get("value")
                .map(|v| match v {
                    serde_json::Value::Bool(b) => if *b { "true".into() } else { "false".into() },
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    _ => "".into(),
                })
                .ok_or("value required")?;
            data.prefs.set(key, &value);
            app.apply_prefs();
            Ok(serde_json::json!({"ok": true}))
        }

        // ---- history ----
        "history.search" => {
            let q = p.get("q").and_then(|x| x.as_str()).unwrap_or("");
            let rows: Vec<serde_json::Value> = data
                .history
                .search(q, 100)
                .into_iter()
                .map(|h| serde_json::json!({"id": h.id, "url": h.url, "title": h.title, "count": h.visit_count, "ts": h.last_visit}))
                .collect();
            Ok(serde_json::json!(rows))
        }
        "history.clear" => {
            data.history.clear();
            Ok(serde_json::json!({"ok": true}))
        }
        "history.delete" => {
            if let Some(id) = p.get("id").and_then(|x| x.as_i64()) {
                data.history.remove(id);
            }
            Ok(serde_json::json!({"ok": true}))
        }
        "history.deleteHost" => {
            if let Some(host) = p.get("host").and_then(|x| x.as_str()) {
                data.history.remove_urls_matching(host);
            }
            Ok(serde_json::json!({"ok": true}))
        }

        // ---- bookmarks ----
        "bookmarks.list" => {
            let rows: Vec<serde_json::Value> = data
                .bookmarks
                .all()
                .into_iter()
                .map(|b| serde_json::json!({"id": b.id, "url": b.url, "title": b.title, "folder": b.folder}))
                .collect();
            Ok(serde_json::json!(rows))
        }
        "bookmarks.add" => {
            let url = p.get("url").and_then(|x| x.as_str()).ok_or("url")?;
            let title = p.get("title").and_then(|x| x.as_str()).unwrap_or("");
            let folder = p.get("folder").and_then(|x| x.as_str()).unwrap_or("/");
            let added = data.bookmarks.add(url, title, folder);
            Ok(serde_json::json!({"added": added}))
        }
        "bookmarks.remove" => {
            if let Some(id) = p.get("id").and_then(|x| x.as_i64()) {
                data.bookmarks.remove(id);
            }
            Ok(serde_json::json!({"ok": true}))
        }
        "bookmarks.update" => {
            let id = p.get("id").and_then(|x| x.as_i64()).ok_or("id")?;
            let title = p.get("title").and_then(|x| x.as_str()).unwrap_or("");
            let url = p.get("url").and_then(|x| x.as_str()).ok_or("url")?;
            let folder = p.get("folder").and_then(|x| x.as_str()).unwrap_or("/");
            data.bookmarks.update(id, title, url, folder);
            Ok(serde_json::json!({"ok": true}))
        }
        "bookmarks.export" => {
            let html = data.bookmarks.export_html();
            let dir = crate::util::profile_dir();
            let path = dir.join("bookmarks-export.html");
            std::fs::write(&path, &html).map_err(|e| e.to_string())?;
            Ok(serde_json::json!({"path": path.to_string_lossy()}))
        }
        "bookmarks.import" => {
            let html = p
                .get("html")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string())
                .unwrap_or_default();
            let n = data.bookmarks.import_html(&html);
            Ok(serde_json::json!({"imported": n}))
        }

        // ---- newtab ----
        "newtab.topSites" => {
            let rows: Vec<serde_json::Value> = data
                .history
                .top_sites(12)
                .into_iter()
                .map(|h| serde_json::json!({"url": h.url, "title": h.title, "count": h.visit_count}))
                .collect();
            Ok(serde_json::json!(rows))
        }
        "newtab.stats" => Ok(serde_json::to_value(app.privacy.stats()).map_err(|e| e.to_string())?),
        "newtab.pinnedTiles" => {
            let v = app.pinned_tiles();
            Ok(serde_json::json!(v))
        }
        "newtab.pinTile" => {
            let url = p.get("url").and_then(|x| x.as_str()).ok_or("url")?;
            let title = p.get("title").and_then(|x| x.as_str()).unwrap_or("");
            let mut tiles = app.pinned_tiles();
            if !tiles.iter().any(|t| t.0 == url) {
                tiles.push((url.to_string(), title.to_string()));
                app.set_pinned_tiles(tiles);
            }
            Ok(serde_json::json!({"ok": true}))
        }
        "newtab.unpinTile" => {
            let url = p.get("url").and_then(|x| x.as_str()).ok_or("url")?;
            let tiles = app.pinned_tiles();
            let tiles: Vec<(String, String)> = tiles.into_iter().filter(|t| t.0 != url).collect();
            app.set_pinned_tiles(tiles);
            Ok(serde_json::json!({"ok": true}))
        }

        // ---- privacy dashboard ----
        "privacy.stats" => Ok(serde_json::to_value(app.privacy.stats()).map_err(|e| e.to_string())?),
        "privacy.topBlocked" => {
            let rows = app.privacy.engine.top_blocked_hosts(15);
            Ok(serde_json::json!(rows))
        }
        "privacy.lists" => {
            let status = app.privacy.engine.lists_status.lock().unwrap().clone();
            Ok(serde_json::to_value(&status).map_err(|e| e.to_string())?)
        }
        "privacy.updateList" => {
            let id = p.get("id").and_then(|x| x.as_str()).ok_or("id")?.to_string();
            let engine = app.privacy.engine.clone();
            std::thread::spawn(move || {
                let _ = engine.update_list(&id);
            });
            Ok(serde_json::json!({"started": true}))
        }
        "privacy.rulesLoaded" => Ok(serde_json::json!(app.privacy.engine.rules_loaded.load(std::sync::atomic::Ordering::Relaxed))),
        "privacy.engineReady" => Ok(serde_json::json!(app.privacy.engine.is_ready())),

        // ---- settings-only actions ----
        "settings.clearData" => {
            let what = p.get("what").and_then(|x| x.as_str()).unwrap_or("");
            app.clear_browsing_data(what);
            Ok(serde_json::json!({"ok": true}))
        }
        "settings.customFilters" => {
            let text = p.get("text").and_then(|x| x.as_str()).unwrap_or("");
            data.prefs.set("custom_filters", text);
            app.apply_prefs();
            Ok(serde_json::json!({"ok": true}))
        }
        "settings.addListUrl" => {
            // custom list URLs: stored as extra lists, applied on rebuild
            let url = p.get("url").and_then(|x| x.as_str()).ok_or("url")?;
            let mut lists = app.custom_lists();
            if !lists.iter().any(|l| l == url) {
                lists.push(url.to_string());
                app.set_custom_lists(lists);
            }
            Ok(serde_json::json!({"ok": true}))
        }
        "settings.listUrls" => Ok(serde_json::json!(app.custom_lists())),
        "settings.removeListUrl" => {
            let url = p.get("url").and_then(|x| x.as_str()).ok_or("url")?;
            let lists = app.custom_lists();
            let lists: Vec<String> = lists.into_iter().filter(|l| l != url).collect();
            app.set_custom_lists(lists);
            Ok(serde_json::json!({"ok": true}))
        }
        "settings.perms" => {
            let rows = data.prefs.all_perms();
            Ok(serde_json::json!(rows))
        }
        "settings.removePerm" => {
            let origin = p.get("origin").and_then(|x| x.as_str()).ok_or("origin")?;
            let perm = p.get("perm").and_then(|x| x.as_str()).ok_or("perm")?;
            data.prefs.set_perm(origin, perm, "prompt");
            Ok(serde_json::json!({"ok": true}))
        }
        "settings.importBookmarksFile" => {
            // html arrives as string param (file read by the page's file input)
            let html = p.get("html").and_then(|x| x.as_str()).unwrap_or("");
            let n = data.bookmarks.import_html(html);
            Ok(serde_json::json!({"imported": n}))
        }
        "settings.rebuildEngine" => {
            app.apply_prefs();
            Ok(serde_json::json!({"ok": true}))
        }

        // ---- downloads page ----
        "downloads.list" => {
            let rows: Vec<serde_json::Value> = data
                .downloads
                .list(100)
                .into_iter()
                .map(|d| serde_json::json!({"id": d.id, "url": d.url, "path": d.path, "mime": d.mime, "size": d.size, "state": d.state, "ts": d.started}))
                .collect();
            Ok(serde_json::json!(rows))
        }
        "downloads.clear" => {
            data.downloads.clear();
            Ok(serde_json::json!({"ok": true}))
        }
        "downloads.open" => {
            let path = p.get("path").and_then(|x| x.as_str()).ok_or("path")?;
            if let Some(parent) = std::path::Path::new(path).parent() {
                let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
            }
            Ok(serde_json::json!({"ok": true}))
        }
        "downloads.reveal" => {
            let path = p.get("path").and_then(|x| x.as_str()).ok_or("path")?;
            if let Some(parent) = std::path::Path::new(path).parent() {
                let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
            }
            Ok(serde_json::json!({"ok": true}))
        }

        // ---- navigation requests from pages ----
        "nav.open" => {
            let url = p.get("url").and_then(|x| x.as_str()).ok_or("url")?;
            app.open_url(url, p.get("newTab").and_then(|x| x.as_bool()).unwrap_or(false));
            Ok(serde_json::json!({"ok": true}))
        }

        // ---- passwords ----
        "passwords.listOrigins" => {
            Ok(serde_json::json!(data.vault.list_origins()))
        }
        "passwords.remove" => {
            let origin = p.get("origin").and_then(|x| x.as_str()).ok_or("origin")?;
            let username = p.get("username").and_then(|x| x.as_str()).ok_or("username")?;
            data.vault.remove(origin, username).map_err(|e| e)?;
            Ok(serde_json::json!({"ok": true}))
        }

        // ---- safe browsing whitelist ----
        "sb.proceed" => {
            let url = p.get("url").and_then(|x| x.as_str()).ok_or("url")?;
            let host = crate::util::host_of_uri(url).ok_or("host")?;
            app.sb_whitelist_add(&host);
            app.open_url(url, false);
            Ok(serde_json::json!({"ok": true}))
        }

        _ => Err(format!("unknown action: {action}")),
    }
}
