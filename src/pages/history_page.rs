//! History manager page.

use std::sync::Arc;

use crate::app::App;
use crate::pages::shared::{shell, topbar};

pub fn render(_app: &Arc<App>) -> (String, &'static str) {
    let body = r##"
__TOP__
<div class="wrap">
  <div class="hero" style="margin-bottom:14px">
    <div><h1 style="font-size:22px">History</h1><p id="count"></p></div>
    <div style="flex:1"></div>
    <input type="text" id="q" placeholder="Search history…" style="background:var(--bg-elev);border:1px solid var(--border);border-radius:10px;padding:9px 14px;color:var(--text);outline:none;width:280px;font-size:13px">
    <button class="btn danger" id="clear">Clear all</button>
  </div>
  <div class="card" style="padding:0;overflow:hidden">
    <table class="tbl">
      <thead><tr><th style="width:150px">When</th><th style="width:32%">Title</th><th>URL</th><th style="width:190px;text-align:right"></th></tr></thead>
      <tbody id="rows"><tr><td colspan="4" class="muted" style="padding:24px">Loading…</td></tr></tbody>
    </table>
  </div>
</div>
<script>
const $ = id => document.getElementById(id);
async function render(q) {{
  const items = await rpc('history.search', {{ q: q || '' }});
  $('count').textContent = items.length + ' entries';
  const rows = $('rows');
  if (!items.length) {{
    rows.innerHTML = '<tr><td colspan="4"><div class="empty"><div class="big">🕐</div>No history found.</div></td></tr>';
    return;
  }}
  rows.innerHTML = items.map(h2 => `<tr>
    <td class="muted">${{fmtTime(h2.ts)}}</td>
    <td><b>${{esc(h2.title || h2.url)}}</b></td>
    <td class="muted" style="font-family:var(--mono);font-size:11.5px;max-width:300px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${{esc(h2.url)}}</td>
    <td style="text-align:right;white-space:nowrap">
      <button class="btn ghost" data-act="open" data-url="${{esc(h2.url)}}">Open</button>
      <button class="btn ghost" data-act="forgetSite" data-host="${{esc(hostOf(h2.url))}}">Forget site</button>
      <button class="btn ghost" data-act="del" data-id="${{h2.id}}">Delete</button>
    </td></tr>`).join('');
  rows.querySelectorAll('button').forEach(btn => btn.addEventListener('click', () => {{
    const act = btn.dataset.act;
    if (act === 'open') return rpc('nav.open', {{ url: btn.dataset.url }});
    if (act === 'del') return rpc('history.delete', {{ id: +btn.dataset.id }}).then(() => render($('q').value));
    if (act === 'forgetSite') return rpc('history.deleteHost', {{ host: btn.dataset.host }}).then(() => render($('q').value));
  }}));
}}
function hostOf(u) {{ try {{ return new URL(u).hostname; }} catch (e) {{ return ''; }} }}
render('');
let h; $('q').addEventListener('input', () => {{ clearTimeout(h); h = setTimeout(() => render($('q').value), 180); }});
$('clear').addEventListener('click', () => {{
  if (confirm('Delete ALL browsing history?')) rpc('history.clear', {{}}).then(() => render(''));
}});
</script>
"##.replace("__TOP__", &topbar("History"));
    (shell("History", &body, ""), "text/html")
}
