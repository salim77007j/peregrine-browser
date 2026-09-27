//! Shared JS: RPC bridge + helpers used by all internal pages.
//! The CSS constant lives in css.rs; both are concatenated into page shells.

pub const CSS: &str = include_str!("shared.css");

pub const JS: &str = r##"
var __rpcSeq = 0; const __rpcPending = {};
window.__rpc = function(id, payload) {
  const p = __rpcPending[id]; if (!p) return;
  delete __rpcPending[id];
  try { const v = JSON.parse(payload); (v && v.__error !== undefined) ? p.reject(v.__error) : p.resolve(v); }
  catch (e) { p.resolve(payload); }
};
function rpc(action, params) {
  return new Promise((resolve, reject) => {
    const id = ++__rpcSeq;
    __rpcPending[id] = { resolve, reject };
    window.webkit.messageHandlers.bridge.postMessage(JSON.stringify({ id, action, params }));
  });
}
function esc(s) { return String(s).replace(/[&<>"]/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c])); }
function toast(msg, ok) {
  let t = document.querySelector('.toast');
  if (!t) { t = document.createElement('div'); t.className = 'toast'; document.body.appendChild(t); }
  t.textContent = msg; t.classList.add('show');
  clearTimeout(t._h); t._h = setTimeout(() => t.classList.remove('show'), 2600);
}
function fmtBytes(n) {
  if (!n) return '0 B';
  const u = ['B','KB','MB','GB','TB']; let v = n, i = 0;
  while (v >= 1024 && i < 4) { v /= 1024; i++; }
  return (i ? v.toFixed(1) : v) + ' ' + u[i];
}
function fmtTime(ts) {
  const d = Date.now()/1000 - ts;
  if (d < 60) return 'just now';
  if (d < 3600) return Math.floor(d/60) + ' min ago';
  if (d < 86400) return Math.floor(d/3600) + ' h ago';
  return new Date(ts*1000).toLocaleDateString();
}
"##;

/// SVG logo mark — the Peregrine falcon-wing chevron.
pub const LOGO_SVG: &str = r##"<svg width="26" height="26" viewBox="0 0 48 48" fill="none">
  <defs><linearGradient id="pg" x1="4" y1="6" x2="44" y2="42">
    <stop offset="0" stop-color="#22d3ee"/><stop offset="1" stop-color="#818cf8"/></linearGradient></defs>
  <path d="M6 38c8-2 12-7 14-13l4-13 4 10c2 5 6 9 14 11-6 3-10 4-14 4-5 0-9-1-12-3-3 2-7 3-10 4z" fill="url(#pg)"/>
  <circle cx="26" cy="11" r="3" fill="url(#pg)"/>
</svg>"##;

/// Assemble a full HTML page shell.
pub fn shell(title: &str, body: &str, extra_js: &str) -> String {
    format!(
        r##"<!DOCTYPE html><html><head><meta charset="utf-8"><title>{title} — Peregrine</title>
<style>{css}</style></head><body>{body}<div class="toast"></div>
<script>{js}</script><script>{extra}</script></body></html>"##,
        title = esc(title),
        css = CSS,
        js = JS,
        extra = extra_js
    )
}

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The shared top bar for manager pages.
pub fn topbar(crumbs: &str) -> String {
    format!(
        r##"<div class="topbar"><div class="brand">{logo}<span>Peregrine</span></div>
<div class="crumbs">{crumbs}</div><div class="spacer"></div>
<div class="pill">Private &amp; local</div></div>"##,
        logo = LOGO_SVG,
        crumbs = esc(crumbs)
    )
}
