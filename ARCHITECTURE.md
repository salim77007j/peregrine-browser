# Peregrine Architecture

Version 0.1 · September 2026

## 1. Design goals (in priority order)

1. **Privacy-first** — block ads/trackers/fingerprinting/malware locally, leak nothing.
2. **Fast & light** — cold start < 1s, tab switch < 50ms, idle RAM/CPU minimal.
3. **Small, maintainable codebase** — smart architecture over millions of lines.
4. **A real daily browser** — no fake UI, everything wired.

## 2. The engine decision (the big one)

An independent browser needs a rendering engine. The 2026 field:

| Option | Verdict |
|--------|---------|
| **Write our own engine** | A complete HTML/CSS/JS engine is multi-year, hundreds-of-engineer work. Antithetical to the "lean engineering" goal. Rejected. |
| **Servo** | Pure Rust, memory-safe, and beautiful — but its web-platform coverage still chases Chrome/WebKit/Safari. Complex SPAs break. Embedding requires building the full Servo tree (2h+ CI, huge dep graph). **Watchlist, not production choice for v0.1.** |
| **Qt WebEngine / CEF (Chromium)** | Chromium's RAM footprint and attack surface are exactly what Peregrine exists to avoid. Rejected. |
| **System WebView shells (wry/tauri)** | No API surface for request interception, cookie policy control, or script injection at the fidelity a privacy browser needs. Rejected. |
| **WebKitGTK 6.0** ✅ | Battle-tested engine (Safari lineage), **multi-process with bubblewrap sandboxing**, native ITP, full privacy API surface, GTK4-integrated, and dramatically lighter than Chromium. Bindings consumed from Rust. |

**The seam:** all engine interaction goes through `webview.rs` + `app.rs` (context/session
construction). A future `EngineAdapter` trait isolates view creation, navigation, script
evaluation and signal wiring — the documented path for a Servo backend when it matures.

## 3. Process model

```
peregrine (Rust, UI + privacy core)
├── WebKitNetworkProcess      (all traffic → through OUR loopback filtering proxy)
├── WebKitWebProcess ×N       (one per tab, bubblewrap-sandboxed)
├── peregrine-adblock         (engine actor thread, Brave adblock-rs, 215k rules)
├── peregrine-proxy           (tokio runtime, 2 threads, loopback-only)
└── db-task threads           (SQLite writes off the UI thread)
```

Per-tab web processes give us site isolation for free: a compromised renderer is confined to
one tab and sandboxed by WebKit's bubblewrap seccomp profile.

## 4. The privacy stack (defense in depth)

### Layer 1 — Network filtering (the proxy)
WebKit's `NetworkSession` is pointed at `127.0.0.1:<ephemeral>` (our in-process proxy).
- **HTTPS**: we see the `CONNECT` hostname only. Decision by hostname rules; the tunnel is
  relayed byte-for-byte — **no TLS interception ever**. The full EasyList host-rule set plus
  URLhaus + heuristics apply.
- **HTTP**: full URL visibility, all rules apply.
- Loopback (`127.0.0.1`, `localhost`) bypasses the proxy (local dev + test servers).
- Every block is counted, categorized (ad/tracker/annoyance/malware) and attributed per-host —
  this powers the Privacy Dashboard with *measured* numbers, not estimates.
- Zero logging; counters are in-memory atomics.

### Layer 2 — Cosmetic filtering
On `load-committed`, the engine's `url_cosmetic_resources()` yields per-URL hide selectors,
injected as a `<style id="peregrine-hide">`. A document-start user script inventories the DOM's
ids/classes and round-trips them to the engine (`hidden_class_id_selectors`) for **generic**
rules — the uBlock-Origin two-phase approach. Hidden element counts feed the stats.

### Layer 3 — Fingerprint shields (farbling)
A document-start user script (main world) wraps:
- `HTMLCanvasElement.toDataURL/toBlob`, `CanvasRenderingContext2D.getImageData` — deterministic
  per-session noise (same session = same fingerprint, different session = different).
- `AudioBuffer.getChannelData`, `AnalyserNode.getFloatFrequencyData` — micro-noise.
- `WebGLRenderingContext.getParameter` / `WEBGL_debug_renderer_info` — generic vendor/renderer.
- `CanvasRenderingContext2D.measureText` — font-metric jitter (2% strict / 0.4% balanced).
- `navigator.hardwareConcurrency/deviceMemory/doNotTrack/getBattery`, screen avail*.
- `RTCPeerConnection` — empty iceServers; host-candidate IPs stripped (leak protection).
Levels: `off` / `balanced` (default) / `strict`.

### Layer 4 — Cookies & storage
WebKit ITP (Intelligent Tracking Prevention) + our cookie policy (`no-third-party` default,
`always`/`never`), per-profile cookie jar, clear-on-exit option.

### Layer 5 — Safe browsing
Bundled URLhaus malware host snapshot + heuristics: IDN/punycode homographs, brand-lookalike
domains (PayPal/Apple/… impersonation), direct-IP URLs, >5-label subdomain chains. Threats get
styled interstitials; "warn"-class threats offer a user-override (whitelisted per-host).

### Layer 6 — Permission hardening
Every WebKit permission request (camera, mic, geolocation, notifications, …) routes through our
policy engine: per-origin exceptions (SQLite) → default policy (prompt/deny/allow) →
`adw::AlertDialog` with "always" options.

## 5. The adblock engine (actor pattern)

`adblock::Engine` contains `Rc` internals → **not `Send`**. Peregrine confines it to a dedicated
thread (`peregrine-adblock`); the proxy and UI talk to it via channel round-trips (~50µs). This
is the same confinement strategy Brave ships. Rebuilds (preference changes) happen on the actor
thread — queries queue behind them, so there is never a torn engine state.

## 6. Performance engineering

- **Startup**: GTK app pattern with async everything — filter lists parse on the engine thread,
  DB opens lazily behind the first paint, the proxy binds in ~1ms before the context needs it.
- **Tab sleeping**: hidden tabs (unpinned, silent, past the timeout) are replaced with
  placeholder pages; their WebKitWebProcess exits and RAM is reclaimed. Selecting the tab
  reloads it (documented trade-off: back-history of sleeping tabs is not preserved in 0.1).
- **Idle**: no polling loops except a 20s sleeper tick and a 1s download-badge refresh (only
  while the popover is open); WebKit's background-tab timer throttling is engine-native.
- **Rendering**: WebKit's Skia compositing with GPU (DMABUF) where stable, software
  (llvmpipe) fallback — automatic, env-tunable.
- **Caching**: WebKit disk/memory cache in the profile dir, HSTS + ITP caches engine-managed.

## 7. Security posture

- Memory-safe core; the single `unsafe` block (30 lines of FFI for
  `webkit_website_data_manager_clear`) is documented and audited.
- Downloads: filename sanitization (basename-only), executable-bit stripping on completion,
  no auto-open.
- No `disable_web_security`, no universal file access, no modal dialogs, data-URL top navigation
  refused, hyperlink auditing (`<a ping>`) off.
- Custom filter-list URLs restricted to `https://`.
- Passwords: AES-256-GCM, Argon2id(19MB, t=2) key schedule, key file `0600` in the profile.
- Session crash recovery via a "running" sentinel file.

## 8. Internal pages & RPC

`peregrine://newtab|settings|privacy|bookmarks|history|downloads` are served from a registered
URI scheme (Rust-side rendering, `MemoryInputStream`). Pages talk to the core through a
JSON-RPC bridge: `window.webkit.messageHandlers.bridge.postMessage({id, action, params})`,
replies via `window.__rpc(id, result)`. **Handler signals are connected before name
registration** (WebKitGTK-documented race).

## 9. Codebase map (~10k lines)

```
src/
├── main.rs            CLI + lifecycle
├── app.rs             App state, WebKit context/session, prefs application
├── window.rs          The browser window (tabs, toolbar, find, zoom, prompts)
├── window_ops.rs      Window lifecycle (leaked &'static windows — GTK owns them)
├── webview.rs         View creation + every WebKit signal
├── tabs.rs            Tab state
├── omnibox.rs         Smart address bar (local-only suggestions)
├── menu.rs            Menu + window actions
├── shortcuts.rs       Accelerators
├── downloads.rs       DownloadHub + safe handling + popover
├── theme.rs / ui/     GTK CSS design system
├── headless.rs        --self-test: drives the real UI, 12 end-to-end checks
├── ffi.rs             30 audited lines of unsafe FFI
├── data/              prefs, history, bookmarks, downloads, password vault
├── privacy/           adblock actor, filtering proxy, shields, safe-browsing
└── pages/             internal pages (HTML/CSS/JS) + RPC dispatch
```

## 10. Known limitations (honesty section)

- Sleeping tabs lose back/forward history (reload on wake).
- Scriptlet injection resources ship a minimal set (uBO's full `resources.json` licensing
  review pending).
- Servo backend is future work behind the EngineAdapter seam.
- Back/forward session restore stores the current entry per tab.
