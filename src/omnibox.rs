//! The omnibox: smart address bar with security indicators, shield stats chip,
//! and local-only suggestions (history + bookmarks + search — zero network suggest,
//! a deliberate privacy feature).

use std::sync::Arc;

use gtk4::glib::{self, clone};
use gtk4::prelude::*;
use webkit6::prelude::*;

use crate::app::App;
use crate::window::BrowserWindow;

pub struct Omnibox {
    root: gtk4::Box,
    entry: gtk4::Entry,
    security: gtk4::Label,
    shield: gtk4::Label,
    popover: gtk4::Popover,
    list: gtk4::ListBox,
    app: Arc<App>,
}

impl Omnibox {
    /// &'static re-derivation (Omnibox is owned by a leaked BrowserWindow).
    #[inline]
    pub fn static_ref(&self) -> &'static Self {
        unsafe { &*(self as *const Self) }
    }

    pub fn new(app: Arc<App>) -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        root.add_css_class("peregrine-omnibox");

        let security = gtk4::Label::new(Some("🔒"));
        security.add_css_class("peregrine-omni-sec");

        let entry = gtk4::Entry::new();
        entry.set_hexpand(true);
        entry.set_placeholder_text(Some("Search or enter address"));
        entry.set_width_request(320);
        entry.set_halign(gtk4::Align::Fill);

        let shield = gtk4::Label::new(Some(""));
        shield.add_css_class("peregrine-omni-shield");
        shield.set_visible(false);

        root.append(&security);
        root.append(&entry);
        root.append(&shield);

        // suggestion popover
        let popover = gtk4::Popover::new();
        popover.set_autohide(false);
        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        scroll.set_max_content_height(360);
        scroll.set_propagate_natural_height(true);
        let list = gtk4::ListBox::new();
        list.set_css_classes(&["peregrine-suggest"]);
        scroll.set_child(Some(&list));
        popover.set_child(Some(&scroll));

        Self { root, entry, security, shield, popover, list, app }
    }

    pub fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub fn entry(&self) -> &gtk4::Entry {
        &self.entry
    }

    pub fn set_text(&self, t: &str) {
        self.entry.set_text(t);
        self.entry.set_position(-1);
    }

    pub fn grab_focus(&self) {
        self.entry.grab_focus();
    }

    /// Wire entry events (needs the window, hence separate from ::new).
    pub fn wire(&self, win: &'static BrowserWindow) {
        // navigate on Enter
        let omni = self.static_ref();
        self.entry.connect_activate(move |_| {
            omni.popover.popdown();
            let text = omni.entry.text().to_string();
            if let Some(url) = resolve_input(&text, &omni.app) {
                win.navigate(&url);
            }
        });

        // suggestions as you type (local only, zero network)
        let timeout_source: std::cell::RefCell<Option<glib::SourceId>> = std::cell::RefCell::new(None);
        let omni2 = self.static_ref();
        self.entry.connect_changed(move |_| {
            let text = omni2.entry.text().to_string();
            omni2.shield.set_visible(false);
            let mut src = timeout_source.borrow_mut();
            if let Some(s) = src.take() {
                s.remove();
            }
            if text.trim().len() < 2 {
                omni2.popover.popdown();
                return;
            }
            let omni3 = omni2.static_ref();
            *src = Some(glib::timeout_add_local(
                std::time::Duration::from_millis(120),
                move || {
                    omni3.render_suggestions(win, &omni3.entry.text().to_string());
                    glib::ControlFlow::Break
                },
            ));
        });

        // keyboard nav: Down focuses suggestions, Escape closes
        let omni4 = self.static_ref();
        let key_ctl = gtk4::EventControllerKey::new();
        key_ctl.connect_key_pressed(move |_ctl, key, _x, _y| {
            let kv = key;
            let down = gtk4::gdk::Key::from_name("Down");
            let esc = gtk4::gdk::Key::from_name("Escape");
            if down.map(|d| kv == d).unwrap_or(false) {
                if omni4.popover.is_visible() {
                    omni4.list.grab_focus();
                } else {
                    omni4.render_suggestions(win, &omni4.entry.text().to_string());
                }
                glib::Propagation::Stop
            } else if esc.map(|e| kv == e).unwrap_or(false) {
                omni4.popover.popdown();
                glib::Propagation::Proceed
            } else {
                glib::Propagation::Proceed
            }
        });
        self.entry.add_controller(key_ctl);

        // clicking away closes the popover
        let click = gtk4::GestureClick::new();
        let omni5 = self.static_ref();
        click.connect_pressed(move |_g, _n, _x, _y| {
            omni5.popover.popdown();
        });
        self.root.add_controller(click);
    }

    fn render_suggestions(&self, win: &'static BrowserWindow, text: &str) {
        let q = text.trim().to_string();
        if q.len() < 2 {
            self.popover.popdown();
            return;
        }
        while let Some(row) = self.list.first_child() {
            self.list.remove(&row);
        }
        let app = &self.app;

        let mut rows: Vec<Suggestion> = vec![];
        // 1. primary action (URL or search)
        if let Some(url) = resolve_input(&q, app) {
            let is_search = !looks_like_url(&q);
            let display_url = if is_search {
                format!("Search with {}", engine_name(app))
            } else {
                crate::util::ellipsize(&url, 80)
            };
            rows.push(Suggestion {
                icon: if is_search { "🔍" } else { "🌐" },
                primary: q.clone(),
                secondary: display_url,
                url,
            });
        }
        // 2. history matches
        for h in app.data.history.search(&q, 6) {
            rows.push(Suggestion {
                icon: "🕘",
                primary: if h.title.is_empty() { h.url.clone() } else { h.title.clone() },
                secondary: crate::util::ellipsize(&h.url, 70),
                url: h.url.clone(),
            });
        }
        // 3. bookmark matches
        for b in app.data.bookmarks.search(&q, 4) {
            rows.push(Suggestion {
                icon: "⭐",
                primary: if b.title.is_empty() { b.url.clone() } else { b.title.clone() },
                secondary: crate::util::ellipsize(&b.url, 70),
                url: b.url.clone(),
            });
        }
        if rows.is_empty() {
            self.popover.popdown();
            return;
        }
        for s in rows.into_iter().take(10) {
            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
            row.set_margin_top(4);
            row.set_margin_bottom(4);
            row.set_margin_start(8);
            row.set_margin_end(8);
            let icon = gtk4::Label::new(Some(s.icon));
            let mid = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            let primary = gtk4::Label::new(Some(&s.primary));
            primary.set_halign(gtk4::Align::Start);
            primary.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            let secondary = gtk4::Label::new(Some(&s.secondary));
            secondary.set_halign(gtk4::Align::Start);
            secondary.add_css_class("dim-label");
            secondary.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            mid.append(&primary);
            mid.append(&secondary);
            row.append(&icon);
            row.append(&mid);
            let lrow = gtk4::ListBoxRow::new();
            lrow.set_child(Some(&row));
            let url = s.url.clone();
            let omni_s = self.static_ref();
            lrow.connect_activate(move |_| {
                omni_s.popover.popdown();
                win.navigate(&url);
            });
            self.list.append(&lrow);
        }
        if !self.popover.is_visible() {
            self.popover.set_pointing_to(None::<&gtk4::gdk::Rectangle>);
            self.popover.popup();
        }
    }

    /// Security + shield indicator updates from the current view state.
    pub fn on_uri(&self, uri: &str) {
        let (icon, tooltip) = if uri.starts_with("peregrine://") {
            ("🛡", "Peregrine internal page")
        } else if uri.starts_with("https://") {
            ("🔒", "Secure connection — your traffic is encrypted")
        } else if uri.starts_with("http://") {
            ("⚠️", "Not secure — this site uses plain HTTP")
        } else if uri.starts_with("file://") {
            ("📁", "Local file")
        } else {
            ("", "")
        };
        self.security.set_label(icon);
        self.security.set_tooltip_text(Some(tooltip));
        // shield chip: blocked count for the session
        let stats = self.app.privacy.stats();
        let total = stats.blocked_requests + stats.cosmetic_hidden;
        if total > 0 && !uri.starts_with("peregrine://") {
            self.shield.set_visible(true);
            self.shield.set_label(&format!("🛡 {}", total));
            let tip = format!(
                "{} requests and {} page elements blocked for you this session",
                stats.blocked_requests, stats.cosmetic_hidden
            );
            self.shield.set_tooltip_text(Some(&tip));
        } else {
            self.shield.set_visible(false);
        }
    }

    pub fn sync_from_view(&self) {
        // fill the entry with the current URI unless focused
        if !self.entry.has_focus() {
            let wv = self.app.active_window().and_then(|w| w.current_webview());
            if let Some(wv) = wv {
                let uri = wv.uri().map(|u| u.to_string()).unwrap_or_default();
                let show = if uri.starts_with("peregrine://newtab") || uri == "about:blank" {
                    String::new()
                } else {
                    uri
                };
                self.entry.set_text(&show);
            }
        }
    }
}

struct Suggestion {
    icon: &'static str,
    primary: String,
    secondary: String,
    url: String,
}

/// Resolve omnibox input to a URL: direct URL, or search via the configured engine.
pub fn resolve_input(text: &str, app: &Arc<App>) -> Option<String> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    // explicit schemes
    let lower = t.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("file://")
        || lower.starts_with("peregrine://") || lower.starts_with("about:") || lower.starts_with("data:")
    {
        return Some(t.to_string());
    }
    if looks_like_url(t) {
        return Some(format!("https://{}", t));
    }
    // search
    let prefs = app.data.prefs.get();
    let template = crate::data::prefs::SEARCH_ENGINES::url_for(&prefs.search_engine, &prefs.custom_search_engine)
        .unwrap_or_else(|| "https://duckduckgo.com/?q={}".into());
    Some(template.replace("{}", &urlencoding_lite(t)))
}

fn looks_like_url(t: &str) -> bool {
    // has a dot with a plausible TLD and no spaces
    if t.contains(' ') || t.contains('\t') {
        return false;
    }
    if t.starts_with("localhost") {
        return true;
    }
    if let Some((host, _)) = t.split_once('/') {
        let h = host;
        if let Some((_, suffix)) = h.rsplit_once('.') {
            if suffix.len() >= 2 && suffix.chars().all(|c| c.is_ascii_alphabetic()) {
                return true;
            }
        }
    }
    // bare hostname without path
    if let Some((_, suffix)) = t.rsplit_once('.') {
        if suffix.len() >= 2 && suffix.chars().all(|c| c.is_ascii_alphabetic()) {
            return true;
        }
    }
    false
}

fn engine_name(app: &Arc<App>) -> String {
    let prefs = app.data.prefs.get();
    for (id, name, _) in crate::data::prefs::SEARCH_ENGINES::all() {
        if id == prefs.search_engine {
            return name.to_string();
        }
    }
    "the search engine".into()
}

/// Minimal percent-encoding for query components (no extra dependency).
fn urlencoding_lite(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_like_url_cases() {
        assert!(looks_like_url("example.com"));
        assert!(looks_like_url("example.com/path"));
        assert!(looks_like_url("localhost:8000"));
        assert!(!looks_like_url("how to train a falcon"));
        assert!(!looks_like_url("what is 2+2"));
        assert!(looks_like_url("news.bbc.co.uk"));
    }

    #[test]
    fn urlencoding() {
        assert_eq!(urlencoding_lite("a b&c"), "a%20b%26c");
        assert_eq!(urlencoding_lite("safe"), "safe");
    }
}
