//! New tab / start page: greeting, search, speed dial, privacy stats, quick links.

use std::sync::Arc;

use crate::app::App;
use crate::pages::shared::{shell, LOGO_SVG};

pub fn render(_app: &Arc<App>) -> (String, &'static str) {
    let body = format!(
        r##"
<div class="stage">
  <div class="center">
    <div class="mark">{logo_big}</div>
    <div class="greet" id="greet">Good day</div>
    <div class="tagline">The fastest browser on Earth.</div>
    <form id="searchform" class="searchwrap">
      <svg class="searchicon" width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="#9aa3b2" stroke-width="2.2"><circle cx="11" cy="11" r="7"/><path d="M20 20l-4-4"/></svg>
      <input id="q" type="text" placeholder="Search privately or enter address" autocomplete="off" spellcheck="false"/>
      <div class="engine" id="engine">DuckDuckGo</div>
    </form>
    <div class="tiles" id="tiles"></div>
  </div>
  <div class="shieldcard" id="shield"></div>
</div>
<style>
body {{ overflow-y: auto; }}
.stage {{ min-height: 100vh; display: flex; flex-direction: column; align-items: center;
  background:
    radial-gradient(1200px 500px at 50% -10%, rgba(34,211,238,.10), transparent 60%),
    radial-gradient(900px 400px at 90% 110%, rgba(129,140,248,.09), transparent 60%);
}}
.center {{ display: flex; flex-direction: column; align-items: center; padding-top: 11vh; width: 640px; max-width: 92vw; }}
.mark {{ margin-bottom: 10px; }}
.greet {{ font-size: 30px; font-weight: 800; letter-spacing: -.6px; margin-top: 6px; }}
.tagline {{ color: var(--text-3); font-size: 13px; margin: 6px 0 30px; }}
.searchwrap {{ width: 100%; position: relative; }}
#q {{
  width: 100%; padding: 15px 118px 15px 46px;
  background: var(--bg-elev); border: 1px solid var(--border);
  border-radius: 16px; color: var(--text); font-size: 15px; outline: none;
  transition: border-color .15s, box-shadow .15s;
}}
#q:focus {{ border-color: rgba(34,211,238,.55); box-shadow: 0 0 0 4px rgba(34,211,238,.09); }}
.searchicon {{ position: absolute; left: 17px; top: 16px; }}
.engine {{
  position: absolute; right: 9px; top: 9px; bottom: 9px;
  background: var(--accent-grad); color: #0b0e14; font-weight: 700; font-size: 12px;
  border-radius: 10px; padding: 0 14px; display: flex; align-items: center; cursor: pointer;
  user-select: none;
}}
.engine:active {{ filter: brightness(.92); }}
.tiles {{ display: grid; grid-template-columns: repeat(6, 88px); gap: 14px; margin-top: 38px; }}
.tile {{ display: flex; flex-direction: column; align-items: center; gap: 8px; cursor: pointer;
  padding: 10px 4px; border-radius: 12px; transition: background .12s; position: relative; }}
.tile:hover {{ background: rgba(255,255,255,.035); }}
.tile .ava {{
  width: 48px; height: 48px; border-radius: 14px;
  display: flex; align-items: center; justify-content: center;
  font-weight: 800; font-size: 19px; color: #fff;
  box-shadow: 0 4px 14px rgba(0,0,0,.4);
}}
.tile .lbl {{ font-size: 11px; color: var(--text-2); max-width: 84px; overflow: hidden;
  text-overflow: ellipsis; white-space: nowrap; text-align: center; }}
.tile .x {{ position: absolute; top: 2px; right: 6px; opacity: 0; color: var(--text-3);
  font-size: 12px; transition: opacity .12s; }}
.tile:hover .x {{ opacity: 1; }}
.shieldcard {{
  margin-top: auto; margin-bottom: 26px;
  background: var(--bg-elev); border: 1px solid var(--border-soft); border-radius: 16px;
  padding: 13px 20px; display: flex; align-items: center; gap: 12px; font-size: 13px;
  color: var(--text-2); box-shadow: var(--shadow); cursor: pointer;
}}
.shieldcard .n {{ color: var(--accent); font-weight: 800; font-size: 16px; }}
.shieldcard svg {{ flex: none; }}
@media (max-width: 640px) {{ .tiles {{ grid-template-columns: repeat(4, 88px); }} }}
</style>
<script>
(async function() {{
  // ---- greeting + clock ----
  const h = new Date().getHours();
  document.getElementById('greet').textContent =
    h < 5 ? 'Flying late' : h < 12 ? 'Good morning' : h < 18 ? 'Good afternoon' : 'Good evening';

  // ---- search ----
  const q = document.getElementById('q');
  let engineId = null, engineName = 'DuckDuckGo';
  try {{
    const prefs = await rpc('prefs.get', {{}});
    engineId = prefs.search_engine;
    const el = document.getElementById('engine');
    const names = {{duckduckgo:'DuckDuckGo', startpage:'Startpage', brave:'Brave', mojeek:'Mojeek',
      google:'Google', bing:'Bing', wikipedia:'Wikipedia', custom:'Custom'}};
    engineName = names[prefs.search_engine] || 'DuckDuckGo';
    el.textContent = engineName;
    el.title = 'Search engine — change in Settings';
  }} catch (e) {{}}
  const engines = ['duckduckgo','startpage','brave','mojeek','google','bing','wikipedia'];
  let eIdx = engines.indexOf(engineId); if (eIdx < 0) eIdx = 0;
  document.getElementById('engine').addEventListener('click', () => {{
    eIdx = (eIdx + 1) % engines.length;
    engineId = engines[eIdx];
    const names = {{duckduckgo:'DuckDuckGo', startpage:'Startpage', brave:'Brave', mojeek:'Mojeek',
      google:'Google', bing:'Bing', wikipedia:'Wikipedia'}};
    engineName = names[engineId];
    document.getElementById('engine').textContent = engineName;
    q.focus();
  }});
  function navigate(v) {{
    v = v.trim(); if (!v) return;
    let url = null;
    if (/^[a-z][a-z0-9+.-]*:\/\//i.test(v)) url = v;
    else if (/^https?:\/\//i.test(v)) url = v;
    else if (/^[\w.-]+\.[a-z]{{2,}}(\/|$|\?)/i.test(v) && !v.includes(' ')) url = 'https://' + v;
    else if (v.startsWith('localhost')) url = 'http://' + v;
    let action;
    if (url) {{ action = rpc('nav.open', {{ url: url }}); }}
    else {{
      const tmpl = {{
        duckduckgo:'https://duckduckgo.com/?q=', startpage:'https://www.startpage.com/sp/search?query=',
        brave:'https://search.brave.com/search?q=', mojeek:'https://www.mojeek.com/search?q=',
        google:'https://www.google.com/search?q=', bing:'https://www.bing.com/search?q=',
        wikipedia:'https://en.wikipedia.org/wiki/Special:Search?search='
      }}[engineId] || 'https://duckduckgo.com/?q=';
      action = rpc('nav.open', {{ url: tmpl + encodeURIComponent(v) }});
    }}
    action.catch(e => toast('Could not open: ' + e));
  }}
  document.getElementById('searchform').addEventListener('submit', e => {{
    e.preventDefault(); navigate(q.value);
  }});

  // ---- speed dial ----
  const tiles = document.getElementById('tiles');
  async function renderTiles() {{
    let items = [];
    try {{ items = await rpc('newtab.pinnedTiles', {{}}); }} catch (e) {{}}
    if (!items.length) {{
      try {{ items = (await rpc('newtab.topSites', {{}})).map(x => ({{url: x.url, title: x.title, pinned: false}})); }} catch (e) {{}}
    }} else {{
      items = items.map(x => ({{url: x[0], title: x[1], pinned: true}}));
    }}
    items = items.slice(0, 12);
    if (!items.length) {{
      items = [
        {{url:'https://en.wikipedia.org', title:'Wikipedia', pinned:false}},
        {{url:'https://github.com', title:'GitHub', pinned:false}},
        {{url:'https://news.ycombinator.com', title:'Hacker News', pinned:false}},
      ];
    }}
    tiles.innerHTML = '';
    for (const it of items) {{
      const host = it.url.replace(/^https?:\/\//,'').split('/')[0];
      const letter = (it.title || host).charAt(0).toUpperCase();
      const color = 'hsl(' + (([...host].reduce((a,c)=>a+c.charCodeAt(0),0)*7)%360) + ', 60%, 42%)';
      const t = document.createElement('div');
      t.className = 'tile';
      t.innerHTML = `<div class="ava" style="background:${{color}}">${{esc(letter)}}</div>
        <div class="lbl">${{esc(it.title || host)}}</div>
        ${{it.pinned ? '<div class="x" title="Unpin">✕</div>' : '<div class="x" title="Pin">📌</div>'}}`;
      t.addEventListener('click', ev => {{
        if (ev.target.classList.contains('x')) {{
          ev.stopPropagation();
          const act = it.pinned ? 'newtab.unpinTile' : 'newtab.pinTile';
          rpc(act, {{ url: it.url, title: it.title || host }}).then(renderTiles);
          return;
        }}
        rpc('nav.open', {{ url: it.url }});
      }});
      tiles.appendChild(t);
    }}
  }}
  renderTiles();

  // ---- privacy shield card ----
  const shield = document.getElementById('shield');
  async function renderShield() {{
    try {{
      const s = await rpc('newtab.stats', {{}});
      const total = s.blocked_requests + s.cosmetic_hidden;
      shield.innerHTML = `<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="#22d3ee" stroke-width="2">
        <path d="M12 3l8 3v6c0 4.5-3.4 7.8-8 9-4.6-1.2-8-4.5-8-9V6z"/><path d="M9 12l2 2 4-4"/></svg>
        <span><span class="n">${{total.toLocaleString()}}</span> trackers &amp; ads blocked for you</span>`;
      shield.onclick = () => rpc('nav.open', {{ url: 'peregrine://privacy' }});
    }} catch (e) {{
      shield.style.display = 'none';
    }}
  }}
  renderShield();
  setInterval(renderShield, 4000);
}})();
</script>
"##,
        logo_big = LOGO_SVG_BIG,
    );
    (shell("New tab", &body, ""), "text/html")
}

const LOGO_SVG_BIG: &str = r##"<svg width="72" height="72" viewBox="0 0 48 48" fill="none">
  <defs><linearGradient id="pg2" x1="4" y1="6" x2="44" y2="42">
    <stop offset="0" stop-color="#22d3ee"/><stop offset="1" stop-color="#818cf8"/></linearGradient></defs>
  <path d="M6 38c8-2 12-7 14-13l4-13 4 10c2 5 6 9 14 11-6 3-10 4-14 4-5 0-9-1-12-3-3 2-7 3-10 4z" fill="url(#pg2)"/>
  <circle cx="26" cy="11" r="3" fill="url(#pg2)"/>
</svg>"##;
