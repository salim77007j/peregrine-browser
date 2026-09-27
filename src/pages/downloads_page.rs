//! Downloads manager page.

use std::sync::Arc;

use crate::app::App;
use crate::pages::shared::{shell, topbar};

pub fn render(_app: &Arc<App>) -> (String, &'static str) {
    let body = r##"
__TOP__
<div class="wrap">
  <div class="hero" style="margin-bottom:14px">
    <div><h1 style="font-size:22px">Downloads</h1><p id="count"></p></div>
    <div style="flex:1"></div>
    <button class="btn ghost" id="clear">Clear list</button>
  </div>
  <div class="card" style="padding:0;overflow:hidden">
    <table class="tbl">
      <thead><tr><th style="width:38%">File</th><th>From</th><th style="width:110px">Size</th><th style="width:120px">Status</th><th style="width:150px;text-align:right"></th></tr></thead>
      <tbody id="rows"><tr><td colspan="5" class="muted" style="padding:24px">Loading…</td></tbody>
    </table>
  </div>
</div>
<script>
const $ = id => document.getElementById(id);
const stateColor = {{ finished: 'var(--ok)', running: 'var(--accent)', cancelled: 'var(--text-3)', failed: 'var(--danger)' }};
async function render() {{
  const items = await rpc('downloads.list', {{}});
  $('count').textContent = items.length + ' items';
  const rows = $('rows');
  if (!items.length) {{
    rows.innerHTML = '<tr><td colspan="5"><div class="empty"><div class="big">⬇️</div>No downloads yet.</div></td></tr>';
    return;
  }}
  rows.innerHTML = items.map(d => `<tr>
    <td><b>${{esc(nameOf(d.path))}}</b><div class="muted" style="font-size:11px;font-family:var(--mono)">${{esc(d.path)}}</div></td>
    <td class="muted" style="font-family:var(--mono);font-size:11.5px;max-width:260px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${{esc(hostOf(d.url))}}</td>
    <td>${{fmtBytes(d.size)}}</td>
    <td style="color:${{stateColor[d.state] || 'var(--text-2)'}};font-weight:700;font-size:12px">${{esc(d.state)}}</td>
    <td style="text-align:right">
      ${{d.state === 'finished' ? `<button class="btn ghost" data-act="open" data-p="${{esc(d.path)}}">Open</button>
        <button class="btn ghost" data-act="folder" data-p="${{esc(d.path)}}">Folder</button>` : ''}}
    </td></tr>`).join('');
  rows.querySelectorAll('button').forEach(btn => btn.addEventListener('click', () => {{
    if (btn.dataset.act === 'open') rpc('downloads.open', {{ path: btn.dataset.p }});
    if (btn.dataset.act === 'folder') rpc('downloads.reveal', {{ path: btn.dataset.p }});
  }}));
}}
function nameOf(p) {{ return p.split('/').pop() || p; }}
function hostOf(u) {{ try {{ return new URL(u).hostname; }} catch (e) {{ return u; }} }}
render();
setInterval(render, 3000);
$('clear').addEventListener('click', () => rpc('downloads.clear', {{}}).then(render));
</script>
"##.replace("__TOP__", &topbar("Downloads"));
    (shell("Downloads", &body, ""), "text/html")
}
