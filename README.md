# Peregrine

**The fastest browser on Earth.** An ultra-fast, ultra-lightweight, privacy-first web browser
built in Rust on the WebKit engine.

```
   ██▄     Peregrine 0.1
  ██▀▀█    Rust core · WebKitGTK 6.0 engine · GTK4/libadwaita UI
 ██   ▀    Ads blocked · Trackers blocked · Fingerprints farbled
```

## What makes Peregrine different

| Area | Peregrine's approach |
|------|----------------------|
| **Privacy engine** | Brave's `adblock-rs` engine (215k+ rules: EasyList, EasyPrivacy, Peter Lowe, Fanboy Annoyance, URLhaus), running in a dedicated thread-confined actor |
| **Network blocking** | An in-process filtering proxy — every request is inspected *before* it leaves your machine. HTTPS is never MITM'd (host-level filtering only) |
| **Cosmetic filtering** | uBlock-Origin-style: per-URL hide selectors + generic class/id rules + element counting |
| **Fingerprint shields** | Canvas/audio/font/WebGL farbling (deterministic per-session noise), WebRTC IP-leak protection, navigator/screen standardization |
| **Safe browsing** | URLhaus malware host blocklist + deceptive-site heuristics (IDN homographs, brand lookalikes, IP URLs) + warning interstitials |
| **Memory & CPU** | Per-tab WebKit processes, *tab sleeping* (RAM freed for hidden tabs), zero telemetry, ~0% idle CPU |
| **Passwords** | AES-256-GCM vault with Argon2id key derivation, at-rest encrypted |
| **Data** | Everything local: SQLite history/bookmarks/downloads, no accounts, no sync, no suggest-leak |

## The stack (and why)

- **Rust core** — privacy engine, filtering proxy, data layer, permissions, safe-browsing, UI
  logic: ~10k lines of memory-safe Rust. The only `unsafe` in the tree is a 30-line audited FFI
  shim for two WebKitGTK functions the bindings miss.
- **WebKitGTK 6.0** (WebKit 2.52) — the rendering engine. Chosen after evaluating Servo (not
  production-ready for daily browsing in 2026), QtWebEngine/Chromium (contradicts the
  lightweight goal), and bare WebView shells (no API surface for privacy hooks). WebKit gives us
  per-tab process isolation, a bubblewrap-sandboxed web process, native ITP, and complete
  privacy-relevant APIs — with far lower RAM than Chromium. The engine sits behind an
  `EngineAdapter` seam so a future Servo backend can be swapped in. See `ARCHITECTURE.md`.
- **GTK4 + libadwaita** — the UI. Rust bindings all the way down (the C++/Qt option was
  rejected: a second language, heavier toolchain, no advantage for a browser shell).

## Features

Tabs (pin, mute, duplicate, reopen closed, sleeping) · smart omnibox with local-only suggestions
· bookmarks (folders, import/export HTML) · history (fuzzy search, forget-site) · downloads
(safe-handling: basename sanitization, exec-bit stripping) · full settings UI · privacy dashboard
with a live self-test · new-tab start page · session restore · find-in-page · print · save page
(MHTML) · zoom with per-site memory · fullscreen · WebKit inspector (devtools) · permission
manager (camera/mic/location/notifications/clipboard/popups with per-site exceptions) ·
keyboard shortcuts · mouse gestures (right-drag ⇄ back/forward) · custom filter lists (ABP
syntax) · filter-list updater · Do Not Track.

## Build

Dependencies (Debian/Ubuntu): `libwebkitgtk-6.0-dev libgtk-4-dev libadwaita-1-dev` and a Rust
toolchain (1.85+).

```bash
cargo build --release
./target/release/peregrine            # run
./target/release/peregrine --self-test
```

Profile lives at `~/.local/share/peregrine` (override with `PEREGRINE_PROFILE`).

## Verification

`peregrine --self-test` drives the real browser under a virtual display and verifies end-to-end:
network-level ad blocking, cosmetic filtering, canvas-fingerprint farbling, navigator hardening,
tab lifecycle, session persistence, and internal pages. CI runs it on every push.

## License

MPL-2.0. Filter lists belong to their respective upstream authors (EasyList, EasyPrivacy,
Peter Lowe, Fanboy, URLhaus/abuse.ch).
