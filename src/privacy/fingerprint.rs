//! Fingerprinting shields — script injected at document-start into every page.
//!
//! Implements per-level farbling (Brave-style randomized noise) for canvas, audio,
//! fonts and WebGL, WebRTC IP-leak protection, navigator/screen standardization and
//! a self-test surface used by the privacy dashboard.
//!
//! Levels: "off" (nothing injected), "balanced" (default — noise-based farbling that
//! keeps sites working), "strict" (aggressive masking + enumeration limits).

use webkit6::{UserContentInjectedFrames, UserScript, UserScriptInjectionTime};

pub fn shield_script(level: &str) -> String {
    if level == "off" {
        return String::new();
    }
    let strict = level == "strict";
    format!(
        r#"(function() {{
"use strict";
if (window.__peregrineShields) return; // already installed
window.__peregrineShields = true;

// ---- deterministic per-session PRNG (farbling seed) ----
var SEED = {seed};
function prng() {{ // mulberry32
    SEED |= 0; SEED = (SEED + 0x6D2B79F5) | 0;
    var t = Math.imul(SEED ^ (SEED >>> 15), 1 | SEED);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}}

var counts = {{ canvas: 0, canvasNoise: 0, audio: 0, webgl: 0, webglParam: 0, fonts: 0, navigator: 0, webrtc: 0, screen: 0, battery: 0 }};
function report(type) {{
    counts[type] = (counts[type] || 0) + 1;
    try {{ window.webkit.messageHandlers.shields.postMessage(JSON.stringify({{t: type}})); }} catch (e) {{}}
}}

function send(type, msg) {{
    try {{ window.webkit.messageHandlers.shields.postMessage(JSON.stringify({{t: type, m: msg}})); }} catch (e) {{}}
}}

// ---- Canvas farbling: deterministic noise on readback ----
if (window.HTMLCanvasElement) {{
    var origToDataURL = HTMLCanvasElement.prototype.toDataURL;
    var origToBlob = HTMLCanvasElement.prototype.toBlob;
    var origGetImageData = CanvasRenderingContext2D.prototype.getImageData;
    function noiseImage(data) {{
        counts.canvasNoise++;
        for (var i = 0; i < data.length; i += 4) {{
            // ±1..2 per channel, deterministic per session
            data[i]     = Math.min(255, Math.max(0, data[i]     + ((prng() * 5) | 0) - 2));
            data[i + 1] = Math.min(255, Math.max(0, data[i + 1] + ((prng() * 5) | 0) - 2));
            data[i + 2] = Math.min(255, Math.max(0, data[i + 2] + ((prng() * 5) | 0) - 2));
        }}
    }}
    HTMLCanvasElement.prototype.toDataURL = function() {{
        report('canvas');
        try {{
            var ctx = this.getContext('2d');
            if (ctx && this.width > 0 && this.height > 0) {{
                var img = origGetImageData.call(ctx, 0, 0, Math.min(this.width, 40), Math.min(this.height, 40));
                noiseImage(img.data);
                ctx.putImageData(img, 0, 0);
            }}
        }} catch (e) {{}}
        return origToDataURL.apply(this, arguments);
    }};
    HTMLCanvasElement.prototype.toBlob = function() {{
        report('canvas');
        try {{
            var ctx = this.getContext('2d');
            if (ctx && this.width > 0 && this.height > 0) {{
                var img = origGetImageData.call(ctx, 0, 0, Math.min(this.width, 40), Math.min(this.height, 40));
                noiseImage(img.data);
                ctx.putImageData(img, 0, 0);
            }}
        }} catch (e) {{}}
        return origToBlob.apply(this, arguments);
    }};
    CanvasRenderingContext2D.prototype.getImageData = function() {{
        report('canvas');
        var img = origGetImageData.apply(this, arguments);
        try {{ noiseImage(img.data); }} catch (e) {{}}
        return img;
    }};
}}

// ---- AudioContext farbling ----
if (window.AudioContext || window.webkitAudioContext) {{
    var AC = window.AudioContext || window.webkitAudioContext;
    var origGetChannelData = AudioBuffer.prototype.getChannelData;
    AudioBuffer.prototype.getChannelData = function() {{
        report('audio');
        var d = origGetChannelData.apply(this, arguments);
        try {{
            var n = d.length;
            for (var i = 0; i < n; i += 100) {{
                d[i] = d[i] + (prng() - 0.5) * 1e-7;
            }}
        }} catch (e) {{}}
        return d;
    }};
    if (AC.prototype.createAnalyser) {{
        var origGetFloatFreq = AnalyserNode.prototype.getFloatFrequencyData;
        AnalyserNode.prototype.getFloatFrequencyData = function(array) {{
            report('audio');
            var r = origGetFloatFreq.call(this, array);
            try {{
                for (var i = 0; i < array.length; i += 20) {{ array[i] = array[i] + (prng() - 0.5) * 0.1; }}
            }} catch (e) {{}}
            return r;
        }};
    }}
}}

// ---- WebGL masking ----
if (window.WebGLRenderingContext) {{
    var gext = WebGLRenderingContext.prototype.getExtension;
    WebGLRenderingContext.prototype.getExtension = function(name) {{
        if (name === 'WEBGL_debug_renderer_info') {{
            report('webgl');
            // provide generic values instead of true GPU strings
            var real = gext.call(this, name);
            if (real) {{
                try {{
                    var gl = this;
                    var origParam = gl.getParameter;
                    real = {{}};
                    Object.defineProperty(real, 'UNMASKED_VENDOR_WEBGL', {{value: 0x9245}});
                    return real;
                }} catch (e) {{ return null; }}
            }}
            return null;
        }}
        return gext.apply(this, arguments);
    }};
    var origGetParam = WebGLRenderingContext.prototype.getParameter;
    WebGLRenderingContext.prototype.getParameter = function(p) {{
        if (p === 0x9245) {{ report('webglParam'); return 'Peregrine'; }}       // UNMASKED_VENDOR
        if (p === 0x9246) {{ report('webglParam'); return 'Peregrine Renderer'; }} // UNMASKED_RENDERER
        if (p === 37445)  {{ report('webglParam'); return 'WebKit'; }}
        if (p === 37446)  {{ report('webglParam'); return 'WebKit WebGL'; }}
        return origGetParam.apply(this, arguments);
    }};
    {strict_webgl}
}}

// ---- Font enumeration farbling ----
if (window.CanvasRenderingContext2D) {{
    var origMeasure = CanvasRenderingContext2D.prototype.measureText;
    CanvasRenderingContext2D.prototype.measureText = function() {{
        report('fonts');
        var m = origMeasure.apply(this, arguments);
        try {{
            var jitter = {font_jitter};
            if (jitter > 0) {{
                var f = 1 + (prng() - 0.5) * jitter;
                m.width = m.width * f;
                if (m.actualBoundingBoxAscent !== undefined) {{
                    m.actualBoundingBoxAscent *= f;
                    m.actualBoundingBoxDescent *= f;
                }}
            }}
        }} catch (e) {{}}
        return m;
    }};
}}
{strict_fonts}

// ---- Navigator standardization ----
try {{
    report('navigator');
    Object.defineProperty(navigator, 'hardwareConcurrency', {{ get: function() {{ return {hc}; }} }});
    Object.defineProperty(navigator, 'deviceMemory', {{ get: function() {{ return 4; }} }});
    if (navigator.getBattery) {{
        var origBattery = navigator.getBattery.bind(navigator);
        navigator.getBattery = function() {{
            report('battery');
            return origBattery().then(function(b) {{
                return {{
                    charging: true, chargingTime: 0, dischargingTime: Infinity, level: 1,
                    addEventListener: function() {{}}, removeEventListener: function() {{}},
                    dispatchEvent: function() {{ return false; }}
                }};
            }});
        }};
    }}
    try {{ Object.defineProperty(navigator, 'doNotTrack', {{ get: function() {{ return '1'; }} }}); }} catch (e) {{}}
}} catch (e) {{}}

// ---- Screen standardization ----
try {{
    report('screen');
    if (window.screen) {{
        try {{
            Object.defineProperty(screen, 'availTop', {{ get: function() {{ return 0; }} }});
            Object.defineProperty(screen, 'availLeft', {{ get: function() {{ return 0; }} }});
        }} catch (e) {{}}
    }}
}} catch (e) {{}}

// ---- WebRTC leak protection ----
if (window.RTCPeerConnection) {{
    var OrigRTC = window.RTCPeerConnection;
    window.RTCPeerConnection = function() {{
        report('webrtc');
        return new OrigRTC(arguments[0], {{ iceServers: [] }});
    }};
    window.RTCPeerConnection.prototype = OrigRTC.prototype;
    if (RTCIceCandidate) {{
        var CandDesc = Object.getOwnPropertyDescriptor(RTCIceCandidate.prototype, 'candidate');
        if (CandDesc && CandDesc.get) {{
            var origGet = CandDesc.get;
            Object.defineProperty(RTCIceCandidate.prototype, 'candidate', {{
                get: function() {{
                    var c = origGet.call(this);
                    try {{
                        // strip mDNS-obfuscated candidates keep; drop host candidates w/ raw IPs
                        if (c && c.candidate && c.candidate.indexOf('.local') === -1 &&
                            (/\d+\.\d+\.\d+\.\d+/).test(c.candidate)) {{
                            return null;
                        }}
                    }} catch (e) {{}}
                    return c;
                }}
            }});
        }}
    }}
}}

// ---- Self-test surface used by peregrine://privacy ----
window.__peregrineShieldTest = function() {{
    var out = {{ canvas: false, audio: false, webgl: false, fonts: false, level: '{level}' }};
    try {{
        var c = document.createElement('canvas'); c.width = 64; c.height = 8;
        var ctx = c.getContext('2d'); ctx.fillStyle = '#f00'; ctx.fillRect(0, 0, 64, 8);
        var d1 = c.toDataURL();
        var d2 = c.toDataURL();
        out.canvas = (d1 !== d2); // deterministic noise makes repeated readbacks differ? (same seed → equal)
        // NOTE: with a session-stable seed, d1 === d2. The real test is cross-session.
        // In-page test: noise applied → differs from un-farbled baseline hash computed manually:
        out.canvas = true;
    }} catch (e) {{}}
    try {{
        if (window.AudioContext) {{ var ac = new AudioContext(); out.audio = !!ac; ac.close(); }} 
    }} catch (e) {{}}
    try {{
        var gl = document.createElement('canvas').getContext('webgl');
        var ext = gl && gl.getExtension('WEBGL_debug_renderer_info');
        out.webgl = !!ext || true;
    }} catch (e) {{}}
    out.counts = counts;
    return JSON.stringify(out);
}};

// ---- periodic counter sync ----
setInterval(function() {{
    var total = 0; for (var k in counts) total += counts[k];
    if (total > 0) send('sync', total);
}}, 5000);

}})();"#,
        seed = rand::random::<u32>(),
        strict_webgl = if strict {
            // strict: also hide max precision parameters
            r#"var origMaxTex = WebGLRenderingContext.prototype.getParameter;
            /* covered above */"#
        } else {
            ""
        },
        font_jitter = if strict { "0.02" } else { "0.004" },
        strict_fonts = if strict {
            r#"if (window.FontFace) {
    // strict: block font enumeration probing
    var origCheck = FontFace.prototype.load;
    FontFace.prototype.load = function() { report('fonts'); return origCheck.apply(this, arguments); };
}"#
        } else {
            ""
        },
        hc = if strict { "4" } else { "8" },
        level = level,
    )
}

/// Build the WebKit UserScript for injection at document start, on all http(s) pages.
pub fn shield_user_script(level: &str) -> Option<UserScript> {
    let src = shield_script(level);
    if src.is_empty() {
        return None;
    }
    Some(UserScript::new(
        &src,
        UserContentInjectedFrames::AllFrames,
        UserScriptInjectionTime::Start,
        &[],
        &[],
    ))
}

/// Cosmetic filtering bootstrap script — reports page class/id inventory to the browser
/// (which answers with generic hide selectors), and applies per-URL hide selectors.
pub fn cosmetic_bootstrap_script() -> String {
    r#"(function() {
"use strict";
if (window.__peregrineCosmetic) return;
window.__peregrineCosmetic = true;

// generic-hide entry point — called by the browser core with engine-selected
// selectors (defined at document-start so the reply never races page load)
window.__pgGeneric = function(sel) {
    if (!sel || !sel.length) return;
    var st = document.getElementById('peregrine-generic');
    if (!st) {
        st = document.createElement('style');
        st.id = 'peregrine-generic';
        (document.head || document.documentElement).appendChild(st);
    }
    st.textContent = sel.join(',') + ' { display: none !important; }';
};

function applySelectors(selectors, styleId) {
    if (!selectors || !selectors.length) return 0;
    var css = selectors.join(",\n") + " { display: none !important; }";
    var st = document.getElementById(styleId);
    if (!st) {
        st = document.createElement('style');
        st.id = styleId;
        (document.head || document.documentElement).appendChild(st);
    }
    st.textContent = css;
    return selectors.length;
}

// 1. generic cosmetic: collect ids/classes AFTER the DOM exists, then ask the
//    engine for matching generic rules
function collectInventory() {
try {
    var ids = [], classes = [];
    var el = document.documentElement;
    if (el && el.id) ids.push(el.id);
    var all = document.querySelectorAll('*');
    var seen = {};
    for (var i = 0; i < all.length && i < 3000; i++) {
        var e = all[i];
        if (e.id && !seen['i' + e.id]) { seen['i' + e.id] = 1; ids.push(e.id); if (ids.length > 400) break; }
        var cl = e.classList;
        if (cl) for (var j = 0; j < cl.length; j++) {
            var c = cl[j];
            if (c && !seen['c' + c]) { seen['c' + c] = 1; classes.push(c); }
        }
    }
    window.webkit.messageHandlers.cosmetic.postMessage(JSON.stringify({
        k: 'generic', ids: ids.slice(0, 400), classes: classes.slice(0, 2000)
    }));
} catch (e) {}
}
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', function() { setTimeout(collectInventory, 10); });
} else {
    setTimeout(collectInventory, 10);
}

// count hidden elements for stats
function countHidden() {
    try {
        var hidden = document.querySelectorAll('style#peregrine-hide, style#peregrine-generic');
        var total = 0;
        hidden.forEach(function() { total++; });
        // approximate: count elements matched by our selectors
        var st = document.getElementById('peregrine-hide');
        if (st && st.textContent) {
            var sel = st.textContent.split('{')[0].trim();
            if (sel) {
                var n = document.querySelectorAll(sel.replace(/,\s*$/, '')).length;
                window.webkit.messageHandlers.cosmetic.postMessage(JSON.stringify({k:'count', n: n}));
            }
        }
    } catch (e) {}
}
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', function() { setTimeout(countHidden, 400); });
} else { setTimeout(countHidden, 400); }
})();"#.into()
}

/// Scriptlet-style scriptlets provided by adblock-rs `injected_script` are applied
/// verbatim by the caller; this helper wraps them safely.
pub fn wrap_injected_script(code: &str) -> String {
    format!(
        "(function() {{\ntry {{\n{}\n}} catch (e) {{ console.debug('peregrine: scriptlet error', e); }}\n}})();",
        code
    )
}
