//! Safe-browsing: malicious host blocklist + deceptive-site heuristics + interstitials.

use std::collections::HashSet;
use std::sync::OnceLock;

use crate::privacy::adblock::is_malware_host;

/// A short list of the most impersonated brands for lookalike detection.
const TOP_BRANDS: &[(&str, &str)] = &[
    ("paypal", "paypal.com"),
    ("apple", "apple.com"),
    ("microsoft", "microsoft.com"),
    ("google", "google.com"),
    ("amazon", "amazon.com"),
    ("netflix", "netflix.com"),
    ("facebook", "facebook.com"),
    ("instagram", "instagram.com"),
    ("whatsapp", "whatsapp.com"),
    ("linkedin", "linkedin.com"),
    ("github", "github.com"),
    ("dropbox", "dropbox.com"),
    ("steam", "steampowered.com"),
    ("binance", "binance.com"),
    ("coinbase", "coinbase.com"),
    ("chase", "chase.com"),
    ("wellsfargo", "wellsfargo.com"),
    ("hsbc", "hsbc.com"),
];

#[derive(Debug, Clone, PartialEq)]
pub enum Threat {
    Malware,
    Phishing,
    Lookalike { brand: String, real: String },
    IdnHomograph,
    IpAddress,
    ExcessiveSubdomains,
}

impl Threat {
    pub fn title(&self) -> &'static str {
        match self {
            Threat::Malware => "Dangerous site blocked",
            Threat::Phishing => "Phishing site blocked",
            Threat::Lookalike { .. } => "Possible deceptive site",
            Threat::IdnHomograph => "Suspicious international domain",
            Threat::IpAddress => "Direct IP address",
            Threat::ExcessiveSubdomains => "Suspiciously deep subdomain",
        }
    }
    pub fn severity(&self) -> &'static str {
        match self {
            Threat::Malware | Threat::Phishing => "block",
            _ => "warn",
        }
    }
}

/// Analyze a URL for threats. Cheap, offline, no network calls.
pub fn analyze(uri: &str) -> Option<Threat> {
    let url = url::Url::parse(uri).ok()?;
    let host = url.host_str()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let lower = host.to_ascii_lowercase();
    let base = crate::util::base_domain(&lower);

    // 1. known-malware hosts
    if is_malware_host(&lower) {
        return Some(Threat::Malware);
    }

    // 2. direct IP address URLs
    if url::Host::parse(&lower).map(|h| matches!(h, url::Host::Ipv4(_) | url::Host::Ipv6(_))).unwrap_or(false) {
        return Some(Threat::IpAddress);
    }

    // 3. punycode / mixed-script homograph
    for label in lower.split('.') {
        if let Some(prefix) = label.strip_prefix("xn--") {
            let _ = prefix;
            // decoded contains non-ASCII → possible homograph of an ASCII brand
            return Some(Threat::IdnHomograph);
        }
    }

    // 4. excessive subdomains (a common trick: login.paypal.com.secure-ev1l.io)
    let labels: Vec<&str> = lower.split('.').collect();
    if labels.len() > 5 {
        return Some(Threat::ExcessiveSubdomains);
    }

    // 5. brand lookalikes: brand name in a domain whose base domain is NOT the brand's
    for (brand, real) in TOP_BRANDS {
        if lower.contains(brand) && base != *real && !base.ends_with(&format!(".{}", real)) {
            // allow the real one and official subdomains; flag everything else
            return Some(Threat::Lookalike {
                brand: brand.to_string(),
                real: real.to_string(),
            });
        }
    }

    None
}

/// A small set of known phishing keywords combined with suspicious TLDs.
pub fn phishing_keywords() -> &'static HashSet<&'static str> {
    static S: OnceLock<HashSet<&'static str>> = OnceLock::new();
    S.get_or_init(|| {
        [
            "secure-login", "verify-account", "account-verify", "login-secure", "secure-verify",
            "signin-verification", "update-payment", "wallet-connect", "metamask-login",
        ]
        .into_iter()
        .collect()
    })
}

/// Whitelist (user chose "proceed anyway") lives in the prefs DB via the app layer.
#[derive(Debug, Clone)]
pub struct SafeBrowsingDecision {
    pub threat: Threat,
    pub host: String,
    pub allow_proceed: bool,
}

pub fn decide(uri: &str, whitelisted_hosts: &HashSet<String>) -> Option<SafeBrowsingDecision> {
    let threat = analyze(uri)?;
    let host = crate::util::host_of_uri(uri)?;
    if whitelisted_hosts.contains(&host) {
        return None;
    }
    let allow_proceed = threat.severity() == "warn";
    Some(SafeBrowsingDecision { threat, host, allow_proceed })
}

/// The interstitial HTML shown when a threat is detected. It talks to the browser
/// through the `shields` message handler.
pub fn interstitial_html(decision: &SafeBrowsingDecision, uri: &str) -> String {
    let t = &decision.threat;
    let (brand_note, detail) = match t {
        Threat::Lookalike { brand, real } => (
            format!("This site imitates <b>{}</b>", brand),
            format!("The real site is <b>{}</b>. Attackers register lookalike domains to steal credentials.", real),
        ),
        Threat::IdnHomograph => (
            "International domain detected".into(),
            "This hostname uses characters that closely resemble a well-known brand. This is a classic homograph attack.".into(),
        ),
        Threat::IpAddress => (
            "Direct IP address".into(),
            "Legitimate websites almost always use domain names. Direct IP URLs are common in malware distribution.".into(),
        ),
        Threat::ExcessiveSubdomains => (
            "Excessive subdomains".into(),
            "Very long subdomain chains are used to hide the real site you are connecting to.".into(),
        ),
        Threat::Malware => (
            "Known malicious site".into(),
            "This host matches known malware distribution URLs.".into(),
        ),
        Threat::Phishing => (
            "Known phishing site".into(),
            "This site was reported for credential theft.".into(),
        ),
    };
    let proceed = if decision.allow_proceed {
        r#"<button class="ghost" onclick="window.webkit.messageHandlers.shields.postMessage(JSON.stringify({t:'proceed', u:location.href}))">Proceed anyway (not recommended)</button>"#
    } else {
        ""
    };
    format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"><style>
:root {{ color-scheme: dark; }}
body {{ margin:0; font-family: system-ui, sans-serif; background:#14161d; color:#e8eaf0;
  display:flex; align-items:center; justify-content:center; height:100vh; }}
.card {{ max-width:560px; padding:40px; background:#1b1f2a; border-radius:16px;
  border:1px solid #2a3040; text-align:center; }}
.icon {{ font-size:52px; }}
h1 {{ font-size:22px; margin:12px 0 6px; color:#fbbf24; }}
.host {{ color:#9aa3b2; font-size:13px; margin-bottom:18px; word-break:break-all; }}
p {{ font-size:14px; line-height:1.6; color:#c3c9d4; }}
.url {{ font-family:monospace; background:#12141c; padding:8px 12px; border-radius:8px;
  font-size:12px; color:#8b93a5; word-break:break-all; margin:16px 0; }}
button {{ background:#22d3ee; color:#0b0e14; border:none; padding:10px 22px; border-radius:8px;
  font-weight:600; font-size:14px; cursor:pointer; }}
button.ghost {{ background:transparent; color:#9aa3b2; border:1px solid #333a4a; margin-left:10px; }}
</style></head><body><div class="card">
<div class="icon">🛡️</div>
<h1>{title}</h1>
<div class="host">{host}</div>
<p><b>{brand_note}.</b> {detail}</p>
<div class="url">{uri}</div>
<button onclick="history.back()">Go back to safety</button>
{proceed}
</div></body></html>"#,
        title = t.title(),
        host = decision.host,
        uri = escape(uri),
    )
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_lookalike() {
        let t = analyze("https://secure-paypal.com.login.example.io/x").unwrap();
        assert!(matches!(t, Threat::Lookalike { .. } | Threat::ExcessiveSubdomains));
    }

    #[test]
    fn allows_real_brand() {
        assert!(analyze("https://www.paypal.com/signin").is_none());
    }

    #[test]
    fn detects_ip() {
        assert!(matches!(analyze("http://192.168.1.5/pay"), Some(Threat::IpAddress)));
    }

    #[test]
    fn detects_punycode() {
        assert!(matches!(analyze("https://xn--pypal-4ve.com/"), Some(Threat::IdnHomograph)));
    }
}
