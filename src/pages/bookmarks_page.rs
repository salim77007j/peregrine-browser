//! Bookmarks manager page.

use std::sync::Arc;

use crate::app::App;
use crate::pages::shared::{shell, topbar};

pub fn render(_app: &Arc<App>) -> (String, &'static str) {
    let body = r##"
__TOP__
<div class="wrap">
  <div class="hero" style="margin-bottom:14px">
    <div>
      <h1 style="font-size:22px">Bookmarks</h1>
      <p id="count"></p>
    </div>
    <div style="flex:1"></div>
    <input type="text" id="q" placeholder="Search bookmarks…" style="background:var(--bg-elev);border:1px solid var(--border);border-radius:10px;padding:9px 14px;color:var(--text);outline:none;width:280px;font-size:13px">
  </div>
  <div class="card" style="padding:0;overflow:hidden">
    <table class="tbl">
      <thead><tr><th style="width:34%">Title</th><th>URL</th><th style="width:120px">Folder</th><th style="width:130px;text-align:right"></th></tr></thead>
      <tbody id="rows"><tr><td colspan="4" class="muted" style="padding:24px">Loading…</td></tr></tbody>
    </table>
  </div>
</div>
<script>
const $ = id => document.getElementById(id);
let all = [];
async function render(q) {{
  all = await rpc('bookmarks.list', {{}});
  $('count').textContent = all.length + ' saved';
  const rows = $('rows');
  const f = (q || '').toLowerCase();
  const items = f ? all.filter(b =>
    b.title.toLowerCase().includes(f) || b.url.toLowerCase().includes(f) || b.folder.toLowerCase().includes(f)) : all;
  if (!items.length) {{
    rows.innerHTML = '<tr><td colspan="4"><div class="empty"><div class="big">🔖</div>No bookmarks yet. Star any page to save it here.</div></td></tr>';
    return;
  }}
  rows.innerHTML = items.map(b => `<tr>
    <td><b>${{esc(b.title || b.url)}}</b></td>
    <td class="muted" style="font-family:var(--mono);font-size:11.5px;max-width:340px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${{esc(b.url)}}</td>
    <td class="muted">${{esc(b.folder)}}</td>
    <td style="text-align:right;white-space:nowrap">
      <button class="btn ghost" data-act="open" data-url="${{esc(b.url)}}">Open</button>
      <button class="btn ghost" data-act="edit" data-id="${{b.id}}" data-title="${{esc(b.title)}}" data-url="${{esc(b.url)}}" data-folder="${{esc(b.folder)}}">Edit</button>
      <button class="btn ghost" data-act="del" data-id="${{b.id}}">Delete</button>
    </td></tr>`).join('');
  rows.querySelectorAll('button').forEach(btn => btn.addEventListener('click', () => {{
    const act = btn.dataset.act;
    if (act === 'open') return rpc('nav.open', {{ url: btn.dataset.url }});
    if (act === 'del') return rpc('bookmarks.remove', {{ id: +btn.dataset.id }}).then(() => render($('q').value));
    if (act === 'edit') {{
      const title = prompt('Title', btn.dataset.title); if (title === null) return;
      const url = prompt('URL', btn.dataset.url); if (url === null) return;
      const folder = prompt('Folder (/ for root)', btn.dataset.folder); if (folder === null) return;
      rpc('bookmarks.update', {{ id: +btn.dataset.id, title, url, folder }}).then(() => render($('q').value));
    }}
  }}));
}}
render('');
let h; $('q').addEventListener('input', () => {{ clearTimeout(h); h = setTimeout(() => render($('q').value), 180); }});
</script>
"##.replace("__TOP__", &topbar("Bookmarks"));
    (shell("Bookmarks", &body, ""), "text/html")
}
