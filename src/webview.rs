//! WebView creation and full signal wiring: navigation policy, safe browsing,
//! permissions, downloads, TLS warnings, favicons, create-for-new-tab, forms
//! (password save), find controller, print, inspector, mouse gestures.

use std::sync::Arc;

use gtk4::glib;
use gtk4::prelude::*;
use webkit6::prelude::*;
use webkit6::{
    NavigationPolicyDecision, PermissionRequest, PolicyDecisionType, ResponsePolicyDecision,
    UserContentInjectedFrames, UserContentManager, UserScript, UserScriptInjectionTime, WebView,
};

use crate::app::App;
use crate::privacy::fingerprint as fp;
use crate::window::BrowserWindow;

pub struct ViewContext {
    pub ucm: UserContentManager,
}

/// Create a fully wired webview owned by `win`.
pub fn create_view(win: &BrowserWindow, app: &Arc<App>) -> WebView {
    let app: Arc<App> = app.clone();
    let prefs = app.data.prefs.get();
    let ucm = UserContentManager::new();

    // --- RPC bridges ---
    // (connect BEFORE registering, per WebKitGTK docs, to avoid message races)
    ucm.connect_script_message_received(Some("bridge"), { let win = win.static_ref(); let app = app.clone(); move |_ucm, value| {
        let g = value.to_string();
        let s = g.as_str().to_string();
        if let Some(wv) = win.current_webview() {
            crate::pages::handle_rpc(app.clone(), &wv, &s);
        }
    }});

    // --- privacy shields + cosmetic bootstrap ---
    let no_scripts = std::env::var("PEREGRINE_NO_SCRIPTS").is_ok();
    if !no_scripts {
        if let Some(script) = fp::shield_user_script(&prefs.fingerprint_shield) {
            ucm.add_script(&script);
        }
    }
    if !no_scripts {
        let cosmetic = UserScript::new(
            &fp::cosmetic_bootstrap_script(),
            UserContentInjectedFrames::AllFrames,
            UserScriptInjectionTime::Start,
            &[],
            &[],
        );
        ucm.add_script(&cosmetic);
    }

    // --- cosmetic filtering replies (generic selectors) ---
    ucm.connect_script_message_received(
        Some("cosmetic"),
        { let win = win.static_ref(); let app = app.clone(); move |_ucm, value| {
            let g = value.to_string();
            let msg = g.as_str().to_string();
            handle_cosmetic_message(&win, &app, &msg);
        }},
    );

    // --- fingerprint attempt counting ---
    ucm.connect_script_message_received(Some("shields"), { let win = win.static_ref(); let app = app.clone(); move |_ucm, value| {
        let g = value.to_string();
        let msg = g.as_str().to_string();
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&msg) {
            if v.get("t").and_then(|t| t.as_str()) == Some("proceed") {
                // safe-browsing interstitial "proceed anyway"
                if let Some(u) = v.get("u").and_then(|u| u.as_str()) {
                    app.sb_whitelist_add(&crate::util::host_of_uri(u).unwrap_or_default());
                    app.open_url(u, false);
                }
                return;
            }
            app.privacy.engine.record_fingerprint();
        }
    }});

    // register the handler names (after all connections)
    if !no_scripts {
        let _ = ucm.register_script_message_handler("bridge", None::<&str>);
        let _ = ucm.register_script_message_handler("cosmetic", None::<&str>);
        let _ = ucm.register_script_message_handler("shields", None::<&str>);
    }

    // --- the view itself ---
    let use_default_session = std::env::var("PEREGRINE_DEFAULT_SESSION").is_ok();
    let view = if use_default_session {
        WebView::builder()
            .web_context(&app.context)
            .user_content_manager(&ucm)
            .settings(&app.settings)
            .build()
    } else {
        WebView::builder()
            .web_context(&app.context)
            .network_session(&app.session)
            .user_content_manager(&ucm)
            .settings(&app.settings)
            .build()
    };

    wire_view(&view, win, &app);
    view
}

fn handle_cosmetic_message(win: &BrowserWindow, app: &Arc<App>, msg: &str) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(msg) else { return };
    let kind = v.get("k").and_then(|k| k.as_str()).unwrap_or("");
    let prefs = app.data.prefs.get();
    if !prefs.cosmetic_filtering {
        return;
    }
    match kind {
        "generic" => {
            // DOM class/id inventory → engine → generic hide selectors
            let ids: Vec<String> = v
                .get("ids")
                .and_then(|x| x.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            let classes: Vec<String> = v
                .get("classes")
                .and_then(|x| x.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            let mut exceptions = std::collections::HashSet::new();
            if let Some(res) = app.privacy.engine.cosmetic_for(&win.current_uri()) {
                for e in &res.exceptions {
                    exceptions.insert(e.clone());
                }
                if let Some(selectors) = app.privacy.engine.generic_cosmetic(classes, ids, &exceptions) {
                    if !selectors.is_empty() {
                        let json = serde_json::to_string(&selectors).unwrap_or_default();
                        let js = format!(
                            "try {{ window.__pgGeneric && window.__pgGeneric({json}); }} catch (e) {{}}"
                        );
                        if let Some(wv) = win.current_webview() {
                            wv.evaluate_javascript(&js, None, None, None::<&gtk4::gio::Cancellable>, |_| {});
                        }
                    }
                }
            }
        }
        "count" => {
            let n = v.get("n").and_then(|x| x.as_i64()).unwrap_or(0);
            if n > 0 {
                app.privacy.engine.record_cosmetic(n);
            }
        }
        _ => {}
    }
}

fn wire_view(view: &WebView, win: &BrowserWindow, app: &Arc<App>) {
    let app: Arc<App> = app.clone();
    // ---------- navigation & URL state ----------
    view.connect_uri_notify({ let win = win.static_ref(); move |v| {
        let uri = v.uri().map(|u| u.to_string()).unwrap_or_default();
        win.on_uri_changed(&uri);
    } });
    view.connect_title_notify({ let win = win.static_ref(); move |v| {
        let t = v.title().map(|t| t.to_string()).unwrap_or_default();
        win.on_title_changed(&t);
    } });
    view.connect_is_loading_notify({ let win = win.static_ref(); move |v| {
        let loading = v.is_loading();
        win.on_loading_changed(loading);
    } });
    view.connect_favicon_notify({ let win = win.static_ref(); move |v| {
        if let Some(tex) = v.favicon() {
            win.on_favicon(&tex);
        }
    } });
    view.connect_is_playing_audio_notify({ let win = win.static_ref(); move |_| {
        win.refresh_audio_indicators();
    } });

    view.connect_load_changed({ let win = win.static_ref(); let app = app.clone(); move |v, evt| {
        use webkit6::LoadEvent;
        match evt {
            LoadEvent::Committed => {
                let uri = v.uri().map(|u| u.to_string()).unwrap_or_default();
                // apply per-site zoom + cosmetic filters for this URL
                let selectors = app.privacy.engine.cosmetic_for(&uri)
                    .map(|r| r.hide_selectors.into_iter().collect::<Vec<_>>())
                    .unwrap_or_default();
                let injected = app.privacy.engine.cosmetic_for(&uri)
                    .map(|r| r.injected_script.clone())
                    .unwrap_or_default();
                let json = serde_json::to_string(&selectors).unwrap_or_else(|_| "[]".into());
                let inj = if injected.is_empty() {
                    String::new()
                } else {
                    format!("try {{ {} }} catch (e) {{}}", crate::privacy::fingerprint::wrap_injected_script(&injected))
                };
                let js = format!(
                    r#"try {{
                        window.__pgGeneric = function(sel) {{
                            var st = document.getElementById('peregrine-generic');
                            if (!st) {{ st = document.createElement('style'); st.id = 'peregrine-generic';
                                (document.head || document.documentElement).appendChild(st); }}
                            st.textContent = sel.join(',') + ' {{ display: none !important; }}';
                        }};
                        var sels = {json};
                        if (sels.length) {{
                            var st = document.getElementById('peregrine-hide');
                            if (!st) {{ st = document.createElement('style'); st.id = 'peregrine-hide';
                                (document.head || document.documentElement).appendChild(st); }}
                            st.textContent = sels.join(',') + ' {{ display: none !important; }}';
                        }}
                        {inj}
                    }} catch (e) {{}}"#
                );
                v.evaluate_javascript(&js, None, None, None::<&gtk4::gio::Cancellable>, |_| {});
                // history recording (background)
                let data = app.data.clone();
                let uri2 = uri.clone();
                let title = v.title().map(|t| t.to_string()).unwrap_or_default();
                crate::data::spawn_db_task(move || data.history.visit(&uri2, &title));
                win.on_load_committed(&uri);
            }
            LoadEvent::Finished => {
                win.on_load_finished();
            }
            _ => {}
        }
    } });

    view.connect_load_failed({ let win = win.static_ref();  move |_v, _evt, uri, err| {
        win.on_load_failed(uri, err);
        false
    } });

    // ---------- TLS errors ----------
    view.connect_load_failed_with_tls_errors(
        { let win = win.static_ref(); let app = app.clone(); move |v, failing_uri, cert, _errors| {
            let host = crate::util::host_of_uri(failing_uri).unwrap_or_default();
            let html = tls_warning_html(failing_uri);
            v.stop_loading();
            v.load_html(&html, Some(failing_uri));
            // remember an "allow once" path: the page posts via shields handler
            let _ = (win.clone(), app.clone(), cert);
            true
        } },
    );

    // ---------- policy decisions (navigation, new window, downloads) ----------
    if std::env::var("PEREGRINE_NO_POLICY").is_err() {
    view.connect_decide_policy({ let win = win.static_ref(); let app = app.clone(); move |_v, decision, dtype| {
        let mut handled = false;
        match dtype {
            PolicyDecisionType::NavigationAction => {
                if let Ok(nav) = decision.clone().downcast::<NavigationPolicyDecision>() {
                    if let Some(action) = nav.navigation_action() {
                        let uri = action.request().and_then(|r| r.uri()).map(|u| u.to_string()).unwrap_or_default();
                        // safe browsing check
                        let wl = app.sb_whitelisted();
                        if let Some(d) = crate::privacy::safebrowsing::decide(&uri, &wl) {
                            if d.threat.severity() == "block" {
                                decision.ignore();
                                let html = crate::privacy::safebrowsing::interstitial_html(&d, &uri);
                                if let Some(v) = win.current_webview() {
                                    v.stop_loading();
                                    v.load_html(&html, Some(&uri));
                                }
                                app.privacy.engine.record_block(&d.host, "malware", 0);
                                handled = true;
                            }
                        }
                        if !handled {
                            decision.use_();
                        }
                        handled = true;
                    }
                }
            }
            PolicyDecisionType::NewWindowAction => {
                // new windows become tabs (never OS windows)
                decision.use_();
                handled = true;
            }
            PolicyDecisionType::Response => {
                if let Ok(resp) = decision.clone().downcast::<ResponsePolicyDecision>() {
                    let mime = resp.response().and_then(|r| r.mime_type()).map(|m| m.to_string()).unwrap_or_default();
                    let is_html = mime.contains("text/html") || mime.is_empty();
                    if !is_html && !mime.starts_with("text/") && !mime.starts_with("image/")
                        && !mime.starts_with("application/xhtml") && !mime.is_empty() {
                        // non-renderable → download
                        decision.download();
                    } else {
                        decision.use_();
                    }
                    handled = true;
                }
            }
            _ => {}
        }
        handled
    } });
    }

    // ---------- new tab / window creation ----------
    view.connect_create({ let win = win.static_ref(); let app = app.clone(); move |_v, _action| {
        let wv = create_view(&win, &app);
        win.tab_for_created_webview(&wv);
        Some(wv.upcast())
    } });

    // ---------- permissions ----------
    view.connect_permission_request({ let win = win.static_ref(); let app = app.clone(); move |_v, req| {
        handle_permission(&win, &app, req);
        true
    } });

    // ---------- downloads ----------
    view.connect_mouse_target_changed({ let win = win.static_ref(); move |_v, hit, _mods| {
        let label = hit.link_uri().map(|u| u.to_string()).unwrap_or_default();
        win.status_message(if label.is_empty() { None } else { Some(label) });
    } });

    // ---------- print ----------
    view.connect_print({ let win = win.static_ref(); move |v, _op| {
        let op = webkit6::PrintOperation::new(v);
        op.run_dialog(Some(win.widget()));
        true
    } });

    // ---------- form submission → password save prompt ----------
    view.connect_submit_form({ let win = win.static_ref(); let app = app.clone(); move |v, _form| {
        let uri = v.uri().map(|u| u.to_string()).unwrap_or_default();
        let origin = origin_of(&uri);
        let app2 = app.clone();
        v.evaluate_javascript(
            r#"(function(){
                var f = document.activeElement && document.activeElement.form;
                if (!f) return null;
                var u = null, p = null;
                for (var i = 0; i < f.elements.length; i++) {
                    var e = f.elements[i];
                    if (e.type === 'password' && p === null) p = e.value;
                    if ((e.type === 'text' || e.type === 'email') && u === null) u = e.value;
                }
                return (p !== null && p.length) ? JSON.stringify({u: u || '', p: p}) : null;
            })()"#,
            None, None, None::<&gtk4::gio::Cancellable>,
            glib::clone!(#[strong] app2, #[strong(rename_to = win2)] win, move |res| {
                if let Ok(val) = res {
                    let g = val.to_string();
                    let s = g.as_str().to_string();
                    if !s.is_empty() {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                            let user = v.get("u").and_then(|x| x.as_str()).unwrap_or("");
                            let pass = v.get("p").and_then(|x| x.as_str()).unwrap_or("");
                            if !pass.is_empty() {
                                win2.prompt_save_password(&app2, &origin, user, pass);
                            }
                        }
                    }
                }
            }),
        );
    } });

    // ---------- audio mute indicator ----------
    view.connect_is_muted_notify({ let win = win.static_ref(); move |_| {
        win.refresh_audio_indicators();
    } });
}

fn origin_of(uri: &str) -> String {
    url::Url::parse(uri)
        .map(|u| u.origin().ascii_serialization())
        .unwrap_or_else(|_| uri.to_string())
}

fn handle_permission(win: &BrowserWindow, app: &Arc<App>, req: &PermissionRequest) {
    use webkit6 as wk;
    let prefs = app.data.prefs.get();
    // determine permission kind + effective policy (origin exception > default)
    let (kind, default): (&str, &str) = if req.is::<wk::UserMediaPermissionRequest>() {
        let umr = req.clone().downcast::<wk::UserMediaPermissionRequest>().unwrap();
        if umr.is_for_video_device() && !umr.is_for_audio_device() {
            ("camera", prefs.perm_camera.as_str())
        } else {
            ("microphone", prefs.perm_microphone.as_str())
        }
    } else if req.is::<wk::GeolocationPermissionRequest>() {
        ("location", prefs.perm_location.as_str())
    } else if req.is::<wk::NotificationPermissionRequest>() {
        ("notifications", prefs.perm_notifications.as_str())
    } else {
        ("other", "prompt")
    };

    // resolve origin from the requesting view
    let origin = win.current_origin();

    let policy = app
        .data
        .prefs
        .perm_for(&origin, kind)
        .unwrap_or_else(|| default.to_string());

    match policy.as_str() {
        "allow" => req.allow(),
        "deny" => req.deny(),
        _ => {
            // ask the user (async answer is allowed: we hold the request object)
            win.prompt_permission(req, kind, &origin);
        }
    }
}

fn tls_warning_html(uri: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"><style>
body {{ margin:0; font-family: system-ui, sans-serif; background:#14161d; color:#e8eaf0;
display:flex; align-items:center; justify-content:center; height:100vh; }}
.card {{ max-width:540px; padding:40px; background:#1b1f2a; border-radius:16px; border:1px solid #2a3040; text-align:center; }}
h1 {{ font-size:22px; color:#f87171; margin:12px 0; }}
.url {{ font-family:monospace; background:#12141c; padding:8px 12px; border-radius:8px; font-size:12px;
color:#8b93a5; word-break:break-all; margin:16px 0; }}
p {{ font-size:14px; line-height:1.6; color:#c3c9d4; }}
button {{ background:#22d3ee; color:#0b0e14; border:none; padding:10px 22px; border-radius:8px; font-weight:600; cursor:pointer; }}
</style></head><body><div class="card">
<div style="font-size:48px">🔓</div>
<h1>Your connection is not private</h1>
<p>The certificate presented by this site could not be verified. Attackers may be impersonating
this site to steal your data.</p>
<div class="url">{uri}</div>
<button onclick="history.back()">Go back to safety</button>
</div></body></html>"#,
        uri = crate::pages::esc(uri)
    )
}
