//! Settings page — deep customization, all wired to real preferences.

use std::sync::Arc;

use crate::app::App;
use crate::pages::shared::{shell, topbar};

pub fn render(_app: &Arc<App>) -> (String, &'static str) {
    let body = r##"
__TOP__
<div class="wrap">
  <div class="layout">
    <nav class="sidenav" id="nav">
      <a data-s="general" class="on">General</a>
      <a data-s="appearance">Appearance</a>
      <a data-s="privacy">Privacy &amp; Security</a>
      <a data-s="content">Permissions</a>
      <a data-s="performance">Performance</a>
      <a data-s="advanced">Advanced</a>
      <a data-s="about">About</a>
    </nav>

    <main>
    <!-- ==================== GENERAL ==================== -->
    <section class="pane" id="pane-general">
      <h2 class="sec">Startup</h2>
      <div class="card" style="padding:0">
        <div class="row"><div class="info"><div class="name">Home page</div><div class="desc">Opened by the home button</div></div>
          <input type="text" id="p-home_page" style="min-width:260px"></div>
        <div class="row"><div class="info"><div class="name">Restore session on startup</div><div class="desc">Reopen tabs from your last session</div></div>
          <label class="switch"><input type="checkbox" id="p-startup_restore_session"><span class="track"><span class="thumb"></span></span></label></div>
      </div>
      <h2 class="sec">Search</h2>
      <div class="card" style="padding:0">
        <div class="row"><div class="info"><div class="name">Default search engine</div><div class="desc">Used by the address bar</div></div>
          <select id="p-search_engine">
            <option value="duckduckgo">DuckDuckGo — private by default</option>
            <option value="startpage">Startpage</option>
            <option value="brave">Brave Search</option>
            <option value="mojeek">Mojeek</option>
            <option value="google">Google</option>
            <option value="bing">Bing</option>
            <option value="wikipedia">Wikipedia</option>
            <option value="custom">Custom…</option>
          </select></div>
        <div class="row"><div class="info"><div class="name">Custom engine URL</div><div class="desc">Use <code>{{}}</code> where the query goes, e.g. https://search.example/?q={{}}</div></div>
          <input type="text" id="p-custom_search_engine" placeholder="https://search.example/?q={{}}" style="min-width:260px"></div>
      </div>
      <h2 class="sec">Downloads</h2>
      <div class="card" style="padding:0">
        <div class="row"><div class="info"><div class="name">Save files to</div><div class="desc">Default download folder</div></div>
          <input type="text" id="p-downloads_dir" style="min-width:260px"></div>
        <div class="row"><div class="info"><div class="name">Ask where to save each file</div><div class="desc">Show a dialog before every download</div></div>
          <label class="switch"><input type="checkbox" id="p-ask_where_to_save"><span class="track"><span class="thumb"></span></span></label></div>
      </div>
    </section>

    <!-- ==================== APPEARANCE ==================== -->
    <section class="pane hidden" id="pane-appearance">
      <h2 class="sec">Theme</h2>
      <div class="card" style="padding:0">
        <div class="row"><div class="info"><div class="name">Color scheme</div><div class="desc">Peregrine looks best in dark</div></div>
          <div class="seg" id="seg-theme"><button data-v="dark">Dark</button><button data-v="light">Light</button><button data-v="system">System</button></div></div>
        <div class="row"><div class="info"><div class="name">Bookmark bar</div><div class="desc">Show bookmarks under the toolbar</div></div>
          <label class="switch"><input type="checkbox" id="p-show_bookmark_bar"><span class="track"><span class="thumb"></span></span></label></div>
        <div class="row"><div class="info"><div class="name">Compact tabs</div><div class="desc">Slimmer tab strip</div></div>
          <label class="switch"><input type="checkbox" id="p-compact_tabs"><span class="track"><span class="thumb"></span></span></label></div>
      </div>
    </section>

    <!-- ==================== PRIVACY ==================== -->
    <section class="pane hidden" id="pane-privacy">
      <h2 class="sec">Shields</h2>
      <div class="card" style="padding:0">
        <div class="row"><div class="info"><div class="name">Block ads</div><div class="desc">Network-level ad filtering</div></div><label class="switch"><input type="checkbox" id="p-block_ads"><span class="track"><span class="thumb"></span></span></label></div>
        <div class="row"><div class="info"><div class="name">Block trackers</div><div class="desc">Tracker domains &amp; telemetry</div></div><label class="switch"><input type="checkbox" id="p-block_trackers"><span class="track"><span class="thumb"></span></span></label></div>
        <div class="row"><div class="info"><div class="name">Block annoyances</div><div class="desc">Cookie banners &amp; overlay popups</div></div><label class="switch"><input type="checkbox" id="p-block_annoyances"><span class="track"><span class="thumb"></span></span></label></div>
        <div class="row"><div class="info"><div class="name">Block malicious sites</div><div class="desc">URLhaus blocklist + heuristics</div></div><label class="switch"><input type="checkbox" id="p-block_malware"><span class="track"><span class="thumb"></span></span></label></div>
        <div class="row"><div class="info"><div class="name">Heuristic blocking</div><div class="desc">Catch new trackers by naming patterns</div></div><label class="switch"><input type="checkbox" id="p-heuristic_blocking"><span class="track"><span class="thumb"></span></span></label></div>
        <div class="row"><div class="info"><div class="name">Cosmetic filtering</div><div class="desc">Hide ad slots left behind</div></div><label class="switch"><input type="checkbox" id="p-cosmetic_filtering"><span class="track"><span class="thumb"></span></span></label></div>
      </div>
      <h2 class="sec">Fingerprinting</h2>
      <div class="card" style="padding:0">
        <div class="row"><div class="info"><div class="name">Fingerprint shields</div><div class="desc">Canvas / audio / font / WebGL farbling</div></div>
          <div class="seg" id="seg-fp"><button data-v="off">Off</button><button data-v="balanced">Balanced</button><button data-v="strict">Strict</button></div></div>
        <div class="row"><div class="info"><div class="name">WebRTC leak protection</div><div class="desc">Hide local IPs from peer connections</div></div>
          <label class="switch"><input type="checkbox" id="p-webrtc_leak_protection"><span class="track"><span class="thumb"></span></span></label></div>
      </div>
      <h2 class="sec">Cookies &amp; storage</h2>
      <div class="card" style="padding:0">
        <div class="row"><div class="info"><div class="name">Cookie policy</div><div class="desc">Cross-site cookie control</div></div>
          <div class="seg" id="seg-cookies"><button data-v="no-third-party">No 3rd-party</button><button data-v="always">Allow all</button><button data-v="never">Block all</button></div></div>
        <div class="row"><div class="info"><div class="name">Intelligent Tracking Prevention</div><div class="desc">WebKit's ITP classifier (per-site tracking data)</div></div>
          <label class="switch"><input type="checkbox" id="p-enable_itp"><span class="track"><span class="thumb"></span></span></label></div>
        <div class="row"><div class="info"><div class="name">Clear data on exit</div><div class="desc">Wipe cookies, cache &amp; history when closing</div></div>
          <label class="switch"><input type="checkbox" id="p-clear_browsing_on_exit"><span class="track"><span class="thumb"></span></span></label></div>
      </div>
      <h2 class="sec">Clear browsing data</h2>
      <div class="card" style="padding:18px">
        <div style="display:flex;gap:18px;flex-wrap:wrap;color:var(--text-2);font-size:13px;margin-bottom:14px">
          <label><input type="checkbox" id="cd-history" checked> History</label>
          <label><input type="checkbox" id="cd-cookies" checked> Cookies &amp; site data</label>
          <label><input type="checkbox" id="cd-cache" checked> Cached files</label>
        </div>
        <button class="btn danger" id="clear-btn">Clear selected data</button>
      </div>
      <h2 class="sec">Custom filters <span class="sub">ABP syntax, applied instantly</span></h2>
      <div class="card" style="padding:18px">
        <textarea id="custom-filters" placeholder="||ads.example.com^&#10;##.ad-banner&#10;@@||site-you-like.com^" spellcheck="false"></textarea>
        <div style="margin-top:12px;display:flex;gap:10px">
          <button class="btn" id="save-filters">Apply custom filters</button>
          <button class="btn ghost" id="rebuild-engine">Rebuild engine</button>
        </div>
      </div>
      <h2 class="sec">Additional filter lists <span class="sub">by URL</span></h2>
      <div class="card" style="padding:18px">
        <div id="listurls" style="margin-bottom:12px"></div>
        <div style="display:flex;gap:10px">
          <input type="text" id="newlisturl" placeholder="https://example.org/list.txt" style="flex:1">
          <button class="btn" id="addlisturl">Add</button>
        </div>
      </div>
    </section>

    <!-- ==================== PERMISSIONS ==================== -->
    <section class="pane hidden" id="pane-content">
      <h2 class="sec">Default permission policy <span class="sub">sites can still ask, unless set to deny</span></h2>
      <div class="card" style="padding:0">
        <div class="row"><div class="info"><div class="name">Camera</div></div><div class="seg" data-pk="perm_camera"><button data-v="prompt">Ask</button><button data-v="deny">Deny</button><button data-v="allow">Allow</button></div></div>
        <div class="row"><div class="info"><div class="name">Microphone</div></div><div class="seg" data-pk="perm_microphone"><button data-v="prompt">Ask</button><button data-v="deny">Deny</button><button data-v="allow">Allow</button></div></div>
        <div class="row"><div class="info"><div class="name">Location</div></div><div class="seg" data-pk="perm_location"><button data-v="prompt">Ask</button><button data-v="deny">Deny</button><button data-v="allow">Allow</button></div></div>
        <div class="row"><div class="info"><div class="name">Notifications</div></div><div class="seg" data-pk="perm_notifications"><button data-v="prompt">Ask</button><button data-v="deny">Deny</button><button data-v="allow">Allow</button></div></div>
        <div class="row"><div class="info"><div class="name">Clipboard</div></div><div class="seg" data-pk="perm_clipboard"><button data-v="prompt">Ask</button><button data-v="deny">Deny</button><button data-v="allow">Allow</button></div></div>
        <div class="row"><div class="info"><div class="name">Pop-up windows</div></div><div class="seg" data-pk="perm_popups"><button data-v="prompt">Ask</button><button data-v="deny">Deny</button><button data-v="allow">Allow</button></div></div>
      </div>
      <h2 class="sec">Site exceptions</h2>
      <div class="card" style="padding:0">
        <table class="tbl"><thead><tr><th>Origin</th><th>Permission</th><th>Value</th><th></th></tr></thead>
        <tbody id="perms"><tr><td colspan="4" class="muted" style="padding:20px">No exceptions set.</td></tr></tbody></table>
      </div>
    </section>

    <!-- ==================== PERFORMANCE ==================== -->
    <section class="pane hidden" id="pane-performance">
      <h2 class="sec">Resource usage</h2>
      <div class="card" style="padding:0">
        <div class="row"><div class="info"><div class="name">Put inactive tabs to sleep after</div><div class="desc">Frees RAM for the tabs you're using; sleeping tabs reload on wake. 0 = never.</div></div>
          <input type="number" id="p-tab_sleep_minutes" min="0" max="240" style="width:90px"></div>
        <div class="row"><div class="info"><div class="name">WebGL</div><div class="desc">GPU-accelerated 3D canvas</div></div>
          <label class="switch"><input type="checkbox" id="p-enable_webgl"><span class="track"><span class="thumb"></span></span></label></div>
        <div class="row"><div class="info"><div class="name">Smooth scrolling</div></div>
          <label class="switch"><input type="checkbox" id="p-smooth_scrolling"><span class="track"><span class="thumb"></span></span></label></div>
        <div class="row"><div class="info"><div class="name">DNS prefetching</div><div class="desc">Resolves links before you click — faster, but leaks the links you see</div></div>
          <label class="switch"><input type="checkbox" id="p-prefetch_dns"><span class="track"><span class="thumb"></span></span></label></div>
      </div>
    </section>

    <!-- ==================== ADVANCED ==================== -->
    <section class="pane hidden" id="pane-advanced">
      <h2 class="sec">Browser identity</h2>
      <div class="card" style="padding:0">
        <div class="row"><div class="info"><div class="name">User agent</div><div class="desc">Empty = Peregrine default. Changing this reduces fingerprint resistance.</div></div>
          <input type="text" id="p-user_agent" style="min-width:300px"></div>
        <div class="row"><div class="info"><div class="name">Send Do Not Track</div></div>
          <label class="switch"><input type="checkbox" id="p-do_not_track"><span class="track"><span class="thumb"></span></span></label></div>
        <div class="row"><div class="info"><div class="name">Developer tools</div><div class="desc">Right-click → Inspect Element</div></div>
          <label class="switch"><input type="checkbox" id="p-developer_extras"><span class="track"><span class="thumb"></span></span></label></div>
      </div>
      <h2 class="sec">Saved passwords</h2>
      <div class="card" style="padding:0">
        <table class="tbl"><thead><tr><th>Site</th><th>Username</th><th></th></tr></thead>
        <tbody id="pwlist"><tr><td colspan="3" class="muted" style="padding:20px">No passwords saved.</td></tr></tbody></table>
      </div>
      <h2 class="sec">Bookmarks tools</h2>
      <div class="card" style="padding:18px;display:flex;gap:10px;flex-wrap:wrap">
        <button class="btn ghost" id="bm-export">Export bookmarks (HTML)</button>
        <label class="btn ghost" style="cursor:pointer">Import bookmarks (HTML)<input type="file" id="bm-import" accept=".html,.htm" style="display:none"></label>
      </div>
    </section>

    <!-- ==================== ABOUT ==================== -->
    <section class="pane hidden" id="pane-about">
      <div class="card" style="padding:26px">
        <h2 class="sec" style="margin-top:0">Peregrine <span id="ver"></span></h2>
        <p style="color:var(--text-2)">An ultra-fast, privacy-first browser built in Rust on the WebKit engine.
        The core — privacy engine, filtering proxy, data layer, and UI logic — is memory-safe Rust.
        Pages render in sandboxed, per-tab WebKit processes.</p>
        <p style="color:var(--text-2)">Filter lists: EasyList, EasyPrivacy, Peter Lowe's, Fanboy Annoyance,
        URLhaus — updated straight from their official sources.</p>
        <p style="color:var(--text-3);font-size:12px">Engine: WebKitGTK 6.0 · UI: GTK4/libadwaita · Core: Rust</p>
      </div>
    </section>
    </main>
  </div>
</div>
<style>
.layout {{ display: grid; grid-template-columns: 210px 1fr; gap: 26px; }}
.sidenav {{ display: flex; flex-direction: column; gap: 2px; position: sticky; top: 84px; align-self: start; }}
.sidenav a {{ padding: 9px 14px; border-radius: 9px; color: var(--text-2); font-weight: 600; font-size: 13px; cursor: pointer; }}
.sidenav a:hover {{ background: rgba(255,255,255,.04); text-decoration: none; }}
.sidenav a.on {{ background: rgba(34,211,238,.12); color: var(--accent); }}
.pane.hidden {{ display: none; }}
@media (max-width: 760px) {{ .layout {{ grid-template-columns: 1fr; }} .sidenav {{ flex-direction: row; flex-wrap: wrap; }} }}
</style>
<script>
(async function() {{
  const $ = id => document.getElementById(id);

  // ---- navigation ----
  document.querySelectorAll('.sidenav a').forEach(a => a.addEventListener('click', () => {{
    document.querySelectorAll('.sidenav a').forEach(x => x.classList.toggle('on', x === a));
    document.querySelectorAll('.pane').forEach(p => p.classList.toggle('hidden', p.id !== 'pane-' + a.dataset.s));
  }}));

  const prefs = await rpc('prefs.get', {{}});

  // ---- text/number inputs ----
  const textKeys = ['home_page','custom_search_engine','downloads_dir','user_agent'];
  for (const k of textKeys) {{
    const el = $('p-' + k); if (!el) continue;
    el.value = prefs[k] || '';
    let h; el.addEventListener('input', () => {{ clearTimeout(h); h = setTimeout(() =>
      rpc('prefs.set', {{ key: k, value: el.value }}), 500); }});
  }}
  $('p-tab_sleep_minutes').value = prefs.tab_sleep_minutes;
  $('p-tab_sleep_minutes').addEventListener('change', e =>
    rpc('prefs.set', {{ key: 'tab_sleep_minutes', value: e.target.value }}));

  // ---- selects ----
  $('p-search_engine').value = prefs.search_engine;
  $('p-search_engine').addEventListener('change', e =>
    rpc('prefs.set', {{ key: 'search_engine', value: e.target.value }}));

  // ---- switches ----
  const boolKeys = ['startup_restore_session','ask_where_to_save','show_bookmark_bar','compact_tabs',
    'block_ads','block_trackers','block_annoyances','block_malware','heuristic_blocking','cosmetic_filtering',
    'webrtc_leak_protection','enable_itp','clear_browsing_on_exit','enable_webgl','smooth_scrolling',
    'prefetch_dns','do_not_track','developer_extras'];
  for (const k of boolKeys) {{
    const el = $('p-' + k); if (!el) continue;
    el.checked = !!prefs[k];
    el.addEventListener('change', () => {{
      rpc('prefs.set', {{ key: k, value: el.checked ? 'true' : 'false' }})
        .then(() => toast('Setting saved'))
        .catch(e => toast('Failed: ' + e));
    }});
  }}

  // ---- segmented controls ----
  function seg(elId, key) {{
    const el = typeof elId === 'string' ? $(elId) : elId;
    if (!el) return;
    const btns = el.querySelectorAll('button');
    const paint = v => btns.forEach(b => b.classList.toggle('on', b.dataset.v === v));
    paint(prefs[key]);
    btns.forEach(b => b.addEventListener('click', () => {{
      paint(b.dataset.v);
      rpc('prefs.set', {{ key, value: b.dataset.v }}).then(() => toast('Saved'));
    }}));
  }}
  seg('seg-theme', 'theme');
  seg('seg-fp', 'fingerprint_shield');
  seg('seg-cookies', 'cookie_policy');
  document.querySelectorAll('.seg[data-pk]').forEach(el => seg(el, el.dataset.pk));

  // ---- clear browsing data ----
  $('clear-btn').addEventListener('click', () => {{
    const what = [];
    if ($('cd-history').checked) what.push('history');
    if ($('cd-cookies').checked) what.push('cookies');
    if ($('cd-cache').checked) what.push('cache');
    if (!what.length) return toast('Select something to clear');
    rpc('settings.clearData', {{ what: what.join(',') }}).then(() => toast('Browsing data cleared'));
  }});

  // ---- custom filters ----
  $('custom-filters').value = prefs.custom_filters || '';
  $('save-filters').addEventListener('click', () => {{
    rpc('settings.customFilters', {{ text: $('custom-filters').value }})
      .then(() => toast('Custom filters applied')).catch(e => toast('Failed: ' + e));
  }});
  $('rebuild-engine').addEventListener('click', () => {{
    rpc('settings.rebuildEngine', {{}}).then(() => toast('Engine rebuilding in background'));
  }});

  // ---- list urls ----
  async function renderLists() {{
    try {{
      const urls = await rpc('settings.listUrls', {{}});
      const el = $('listurls');
      el.innerHTML = urls.length ? urls.map(u => `<div class="row" style="padding:8px 0">
        <div class="info"><div class="name" style="font-family:var(--mono);font-size:12px">${{esc(u)}}</div></div>
        <button class="btn ghost" data-u="${{esc(u)}}">Remove</button></div>`).join('')
        : '<div class="muted" style="color:var(--text-3);font-size:12.5px">No additional lists.</div>';
      el.querySelectorAll('button').forEach(b => b.addEventListener('click', () =>
        rpc('settings.removeListUrl', {{ url: b.dataset.u }}).then(renderLists)));
    }} catch (e) {{}}
  }}
  renderLists();
  $('addlisturl').addEventListener('click', () => {{
    const u = $('newlisturl').value.trim();
    if (!u.startsWith('https://')) return toast('Only https:// list URLs are accepted');
    rpc('settings.addListUrl', {{ url: u }}).then(() => {{ $('newlisturl').value=''; renderLists(); }});
  }});

  // ---- site exceptions ----
  async function renderPerms() {{
    try {{
      const rows = await rpc('settings.perms', {{}});
      const el = $('perms');
      el.innerHTML = rows.length ? rows.map(r => `<tr><td>${{esc(r[0])}}</td>
        <td class="muted">${{esc(r[1])}}</td><td>${{esc(r[2])}}</td>
        <td style="text-align:right"><button class="btn ghost" data-o="${{esc(r[0])}}" data-p="${{esc(r[1])}}">Reset</button></td></tr>`).join('')
        : '<tr><td colspan="4" class="muted" style="padding:20px">No exceptions set.</td></tr>';
      el.querySelectorAll('button').forEach(b => b.addEventListener('click', () =>
        rpc('settings.removePerm', {{ origin: b.dataset.o, perm: b.dataset.p }}).then(renderPerms)));
    }} catch (e) {{}}
  }}
  renderPerms();

  // ---- passwords list ----
  async function renderPw() {{
    try {{
      const rows = await rpc('passwords.listOrigins', {{}});
      const el = $('pwlist');
      el.innerHTML = rows.length ? rows.map(r => `<tr><td>${{esc(r[0])}}</td><td>${{esc(r[1])}}</td>
        <td style="text-align:right"><button class="btn ghost" data-o="${{esc(r[0])}}" data-u="${{esc(r[1])}}">Forget</button></td></tr>`).join('')
        : '<tr><td colspan="3" class="muted" style="padding:20px">No passwords saved. Peregrine offers to save when you sign in.</td></tr>';
      el.querySelectorAll('button').forEach(b => b.addEventListener('click', () =>
        rpc('passwords.remove', {{ origin: b.dataset.o, username: b.dataset.u }}).then(renderPw)));
    }} catch (e) {{}}
  }}
  renderPw();

  // ---- bookmarks import/export ----
  $('bm-export').addEventListener('click', () =>
    rpc('bookmarks.export', {{}}).then(r => toast('Exported to ' + r.path)));
  $('bm-import').addEventListener('change', e => {{
    const f = e.target.files[0]; if (!f) return;
    const rd = new FileReader();
    rd.onload = () => rpc('settings.importBookmarksFile', {{ html: rd.result }})
      .then(r => toast('Imported ' + r.imported + ' bookmarks'));
    rd.readAsText(f);
  }});

  // ---- about ----
  try {{ const s = await rpc('privacy.rulesLoaded', {{}}); $('ver').textContent = '· ' + s.toLocaleString() + ' filter rules active'; }} catch (e) {{}}
}})();
</script>
"##.replace("__TOP__", &topbar("Settings"));
    (shell("Settings", &body, ""), "text/html")
}
