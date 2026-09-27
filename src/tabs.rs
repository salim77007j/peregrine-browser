//! Per-tab state.

use std::time::Instant;

use libadwaita::TabPage;
use webkit6::WebView;

pub struct TabState {
    pub page: TabPage,
    pub webview: WebView,
    /// URL to load lazily (session restore)
    pub pending: Option<String>,
    pub last_active: Instant,
    pub sleeping: bool,
}
