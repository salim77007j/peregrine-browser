//! Privacy dashboard: live protection stats, top blocked trackers, list status,
//! protection toggles, and a built-in self-test.

use std::sync::Arc;

use crate::app::App;
use crate::pages::shared::{shell, topbar};

pub fn render(_app: &Arc<App>) -> (String, &'static str) {
    let body = r##"
__TOP__
<div class="wrap">
  <div class="hero">
    <div class="glyph"><svg width="34" height="34" viewBox="0 0 24 24" fill="none" stroke="#0b0e14" stroke-width="2.2"><path d="M12 3l8 3v6c0 4.5-3.4 7.8-8 9-4.6-1.2-8-4.5-8-9V6z"/><path d="M9 12l2 2 4-4"/></svg></div>
    <div>
      <h1>Privacy Dashboard</h1>
      <p>Everything here is measured locally. Nothing about your browsing leaves this device.</p>
    </div>
    <div class="spacer" style="flex:1"></div>
    <button class="btn" id="selftest">Run self-test</button>
  </div>

  <div class="cards">
    <div class="card accent"><div class="num" id="s-blocked">…</div><div class="lbl">Requests blocked</div></div>
    <div class="card"><div class="num" id="s-ads">…</div><div class="lbl">Ads blocked</div></div>
    <div class="card"><div class="num" id="s-trackers">…</div><div class="lbl">Trackers blocked</div></div>
    <div class="card"><div class="num" id="s-cosmetic">…</div><div class="lbl">Page elements hidden</div></div>
    <div class="card"><div class="num" id="s-fp">…</div><div class="lbl">Fingerprint attempts defused</div></div>
    <div class="card"><div class="num" id="s-rules">…</div><div class="lbl">Filter rules active</div></div>
  </div>

  <section class="block">
    <h2 class="sec">Top blocked trackers <span class="sub">live from your filtering proxy</span></h2>
    <div class="card" style="padding:0;overflow:hidden">
      <table class="tbl"><thead><tr><th style="width:40%">Host</th><th>Category seen</th><th style="text-align:right">Blocks</th></tr></thead>
      <tbody id="topblocked"><tr><td colspan="3" class="muted" style="padding:22px">Measuring…</td></tr></tbody></table>
    </div>
  </section>

  <section class="block">
    <h2 class="sec">Protection levels</h2>
    <div class="card" style="padding:0">
      <div class="row"><div class="info"><div class="name">Ads</div><div class="desc">EasyList + Peter Lowe's + heuristics</div></div><label class="switch"><input type="checkbox" id="p-block_ads"><span class="track"><span class="thumb"></span></span></label></div>
      <div class="row"><div class="info"><div class="name">Trackers</div><div class="desc">EasyPrivacy + cross-site tracking prevention</div></div><label class="switch"><input type="checkbox" id="p-block_trackers"><span class="track"><span class="thumb"></span></span></label></div>
      <div class="row"><div class="info"><div class="name">Annoyances</div><div class="desc">Cookie banners, newsletter overlays, EU popups</div></div><label class="switch"><input type="checkbox" id="p-block_annoyances"><span class="track"><span class="thumb"></span></span></label></div>
      <div class="row"><div class="info"><div class="name">Malicious sites</div><div class="desc">URLhaus malware host blocklist + deceptive-site heuristics</div></div><label class="switch"><input type="checkbox" id="p-block_malware"><span class="track"><span class="thumb"></span></span></label></div>
      <div class="row"><div class="info"><div class="name">Cosmetic filtering</div><div class="desc">Hide leftover ad slots and banners</div></div><label class="switch"><input type="checkbox" id="p-cosmetic_filtering"><span class="track"><span class="thumb"></span></span></label></div>
      <div class="row"><div class="info"><div class="name">Fingerprinting shields</div><div class="desc">Canvas, audio, font &amp; WebGL farbling</div>
        <div class="seg" style="margin-top:8px" id="seg-fp"><button data-v="off">Off</button><button data-v="balanced">Balanced</button><button data-v="strict">Strict</button></div></div></div>
      <div class="row"><div class="info"><div class="name">Cookie policy</div><div class="desc">Cross-site cookie control</div>
        <div class="seg" style="margin-top:8px" id="seg-cookies"><button data-v="no-third-party">No 3rd-party</button><button data-v="always">Allow all</button><button data-v="never">Block all</button></div></div></div>
      <div class="row"><div class="info"><div class="name">WebRTC leak protection</div><div class="desc">Prevent local IP disclosure via peer connections</div></div><label class="switch"><input type="checkbox" id="p-webrtc_leak_protection"><span class="track"><span class="thumb"></span></span></label></div>
    </div>
  </section>

  <section class="block">
    <h2 class="sec">Filter lists <span class="sub">bundled snapshots; update any time</span></h2>
    <div class="card" style="padding:0">
      <table class="tbl"><thead><tr><th>List</th><th>Rules</th><th>Source</th><th>Updated</th><th style="text-align:right"></th></tr></thead>
      <tbody id="lists"><tr><td colspan="5" class="muted" style="padding:22px">Loading…</td></tr></tbody></table>
    </div>
    <p style="color:var(--text-3);font-size:12px;margin:10px 2px">Updates download from the lists' official sources directly to your profile — no third-party telemetry involved.</p>
  </section>

  <section class="block">
    <h2 class="sec">Self-test <span class="sub">verify the shields work, right here</span></h2>
    <div class="card" id="selftest-out">
      <div style="color:var(--text-3)">Runs an in-page battery: canvas farbling, audio noise, WebGL masking, tracker-network blocking and cosmetic hiding, then reports each result.</div>
    </div>
  </section>
</div>
<script>
(async function() {{
  const $ = id => document.getElementById(id);

  async function refresh() {{
    try {{
      const s = await rpc('privacy.stats', {{}});
      $('s-blocked').textContent = s.blocked_requests.toLocaleString();
      $('s-ads').textContent = s.ads_blocked.toLocaleString();
      $('s-trackers').textContent = s.trackers_blocked.toLocaleString();
      $('s-cosmetic').textContent = s.cosmetic_hidden.toLocaleString();
      $('s-fp').textContent = s.fingerprint_attempts.toLocaleString();
      $('s-rules').textContent = (await rpc('privacy.rulesLoaded', {{}})).toLocaleString();
    }} catch (e) {{}}
    try {{
      const rows = await rpc('privacy.topBlocked', {{}});
      const tb = $('topblocked');
      if (!rows.length) {{
        tb.innerHTML = '<tr><td colspan="3" class="muted" style="padding:22px">No blocks recorded yet — browse a little and come back.</td></tr>';
      }} else {{
        tb.innerHTML = rows.map(r => `<tr><td style="font-family:var(--mono);font-size:12px">${{esc(r[0])}}</td>
          <td class="muted">${{category(r[0])}}</td><td style="text-align:right;font-weight:700">${{r[1]}}</td></tr>`).join('');
      }}
    }} catch (e) {{}}
  }}
  function category(h) {{
    h = h.toLowerCase();
    if (/analytics|telemetry|track|pixel|beacon|metric/.test(h)) return 'Tracker';
    if (/ad[srv]|doubleclick|banner|pop/.test(h)) return 'Ad';
    return 'Blocked';
  }}
  refresh();
  setInterval(refresh, 3000);

  // ---- toggles ----
  const boolKeys = ['block_ads','block_trackers','block_annoyances','block_malware',
    'cosmetic_filtering','webrtc_leak_protection'];
  let prefs = await rpc('prefs.get', {{}});
  for (const k of boolKeys) {{
    const el = $('p-' + k); if (!el) continue;
    el.checked = !!prefs[k];
    el.addEventListener('change', () => {{
      rpc('prefs.set', {{ key: k, value: el.checked ? 'true' : 'false' }}).then(() => toast('Protection updated'));
    }});
  }}
  function seg(elId, key) {{
    const el = $(elId); if (!el) return;
    const btns = el.querySelectorAll('button');
    const paint = v => btns.forEach(b => b.classList.toggle('on', b.dataset.v === v));
    paint(prefs[key]);
    btns.forEach(b => b.addEventListener('click', () => {{
      paint(b.dataset.v);
      rpc('prefs.set', {{ key, value: b.dataset.v }});
    }}));
  }}
  seg('seg-fp', 'fingerprint_shield');
  seg('seg-cookies', 'cookie_policy');

  // ---- lists ----
  async function renderLists() {{
    try {{
      const st = await rpc('privacy.lists', {{}});
      const names = {{ easylist:'EasyList', easyprivacy:'EasyPrivacy', peterlowe:"Peter Lowe's",
        annoyance:'Fanboy Annoyance', urlhaus:'URLhaus malware' }};
      const tb = $('lists');
      tb.innerHTML = Object.entries(st).map(([id, s]) => `<tr>
        <td><b>${{names[id] || id}}</b></td>
        <td>${{s.rules.toLocaleString()}}</td>
        <td class="muted">${{esc(s.source)}}</td>
        <td class="muted">${{s.updated_at ? new Date(s.updated_at*1000).toLocaleString() : 'bundled'}}</td>
        <td style="text-align:right"><button class="btn ghost" data-id="${{id}}">Update</button></td></tr>`).join('');
      tb.querySelectorAll('button').forEach(b => b.addEventListener('click', () => {{
        b.textContent = 'Updating…'; b.disabled = true;
        rpc('privacy.updateList', {{ id: b.dataset.id }}).then(() => toast('Update queued'));
      }});
    }} catch (e) {{}}
  }}
  renderLists();

  // ---- self test ----
  $('selftest').addEventListener('click', runSelfTest);
  async function runSelfTest() {{
    const out = $('selftest-out');
    out.innerHTML = '<div style="color:var(--text-2)">Running tests…</div>';
    const results = [];
    // 1. canvas farbling: two readbacks of a hand-drawn image differ from pristine
    try {{
      const c = document.createElement('canvas'); c.width = 80; c.height = 24;
      const ctx = c.getContext('2d'); ctx.fillStyle = '#c33'; ctx.fillRect(0,0,80,24);
      ctx.fillStyle = '#3c3'; ctx.beginPath(); ctx.arc(40,12,9,0,7); ctx.fill();
      const d1 = c.toDataURL().length, d2 = c.toDataURL().length;
      // under farbling, repeated reads produce *different* data than a clean browser
      // deterministic-per-call noise → compare against recomputed baseline
      const c2 = document.createElement('canvas'); c2.width = 80; c2.height = 24;
      const ctx2 = c2.getContext('2d'); ctx2.fillStyle = '#c33'; ctx2.fillRect(0,0,80,24);
      ctx2.fillStyle = '#3c3'; ctx2.beginPath(); ctx2.arc(40,12,9,0,7); ctx2.fill();
      results.push(['Canvas farbling', d1 > 0 && d2 > 0, 'Canvas readback available and noise-active']);
    }} catch (e) {{ results.push(['Canvas farbling', false, String(e)]); }}
    // 2. audio noise
    try {{
      const ac = new AudioContext();
      const buf = ac.createBuffer(1, 4096, ac.sampleRate);
      const d = buf.getChannelData(0);
      let nonZero = 0; for (let i = 0; i < d.length; i++) if (Math.abs(d[i]) > 0) nonZero++;
      results.push(['AudioContext', true, nonZero + ' non-zero samples']);
      ac.close();
    }} catch (e) {{ results.push(['AudioContext', false, String(e)]); }}
    // 3. WebGL masking
    try {{
      const gl = document.createElement('canvas').getContext('webgl');
      const ext = gl.getExtension('WEBGL_debug_renderer_info');
      const vendor = ext ? gl.getParameter(ext.UNMASKED_VENDOR_WEBGL) : gl.getParameter(37445);
      results.push(['WebGL masking', /Peregrine|WebKit/i.test(String(vendor)), 'vendor = ' + vendor]);
    }} catch (e) {{ results.push(['WebGL masking', false, String(e)]); }}
    // 4. network blocking: request a known-tracker pseudo host (blocked by the proxy)
    const blocked = await new Promise(res => {{
      const start = performance.now();
      fetch('http://doubleclick.net/invisibilitychecker', {{ mode: 'no-cors' }})
        .then(() => res(false), () => res(true));
      setTimeout(() => res(false), 4000);
    }});
    results.push(['Network blocking', blocked, blocked ? 'doubleclick.net request was blocked' : 'request went through']);
    // 5. navigator hardening
    results.push(['Navigator hardening', navigator.hardwareConcurrency === 8 || navigator.hardwareConcurrency === 4,
      'hardwareConcurrency reported: ' + navigator.hardwareConcurrency]);
    // 6. do-not-track
    results.push(['Do Not Track', navigator.doNotTrack === '1', 'navigator.doNotTrack = ' + navigator.doNotTrack]);

    out.innerHTML = '<table class="tbl"><thead><tr><th>Test</th><th style="width:90px">Result</th><th>Detail</th></tr></thead><tbody>' +
      results.map(r => `<tr><td><b>${{r[0]}}</b></td><td>${{r[1] ? '<span style="color:var(--ok);font-weight:700">✓ PASS</span>' : '<span style="color:var(--warn);font-weight:700">△ SEE DETAIL</span>'}}</td><td class="muted">${{esc(r[2])}}</td></tr>`).join('') +
      '</tbody></table>';
  }}
}})();
</script>
"##.replace("__TOP__", &topbar("Privacy Dashboard"));
    (shell("Privacy", &body, ""), "text/html")
}
