//! Theme: custom CSS layer on top of libadwaita. All chrome styling lives here —
//! the design system is data-driven so it can be re-skinned in one place.

use gtk4::prelude::*;

pub const CHROME_CSS: &str = include_str!("../ui/peregrine.css");

pub fn load() {
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(CHROME_CSS);
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("no display"),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
