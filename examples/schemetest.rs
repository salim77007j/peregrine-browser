use webkit6::prelude::*;
use gtk4::prelude::*;
use gtk4::glib::Bytes as GBytes;
use gtk4::gio::MemoryInputStream;

fn main() {
    let variant = std::env::var("VARIANT").unwrap_or_else(|_| "e".into());
    let app = libadwaita::Application::builder()
        .application_id("com.peregrinebrowser.SchemeTest6")
        .build();
    app.connect_activate(move |app| {
        let ctx = webkit6::WebContext::new();
        ctx.register_uri_scheme("xtest", |request: &webkit6::URISchemeRequest| {
            let uri = request.uri().map(|u| u.to_string()).unwrap_or_default();
            eprintln!("[scheme] {uri}");
            let html = format!("<html><head><title>Page {uri}</title></head><body style='background:#202030;color:#fff'><h1>{uri}</h1></body></html>");
            let bytes = GBytes::from_owned(html.into_bytes());
            let stream = MemoryInputStream::from_bytes(&bytes);
            request.finish(&stream, bytes.len() as i64, Some("text/html"));
        });
        let ucm = webkit6::UserContentManager::new();
        let settings = webkit6::Settings::new();
        settings.set_enable_page_cache(true);
        settings.set_enable_site_specific_quirks(true);
        settings.set_user_agent_with_application_details(Some("Peregrine"), Some("0.1"));
        let view = webkit6::WebView::builder()
            .web_context(&ctx)
            .user_content_manager(&ucm)
            .settings(&settings)
            .build();
        // the browser's load-changed handler that evaluates JS on commit
        view.connect_load_changed(|v, evt| {
            if format!("{evt:?}").contains("Committed") {
                v.evaluate_javascript(
                    "try { var sels = []; if (sels.length) { var st = document.createElement('style'); st.textContent=''; document.head.appendChild(st); } } catch (e) {}",
                    None, None, None::<&gtk4::gio::Cancellable>, |_| {},
                );
            }
            eprintln!("[load] {evt:?}");
        });
        view.connect_uri_notify(|v, _| {
            eprintln!("[uri] {}", v.uri().map(|u| u.to_string()).unwrap_or_default());
        });
        let tv = libadwaita::TabView::new();
        tv.append(&view);
        if variant == "g" {
            // exact browser layout: chrome inside a Revealer + find row + status row
            let bar = libadwaita::TabBar::new();
            bar.set_view(Some(&tv));
            let strip = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            strip.append(&gtk4::WindowControls::new(gtk4::PackType::Start));
            strip.append(&bar);
            strip.append(&gtk4::WindowControls::new(gtk4::PackType::End));
            let toolbar = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
            for icon in ["go-previous-symbolic", "go-next-symbolic", "view-refresh-symbolic"] {
                let b = gtk4::Button::builder().icon_name(icon).css_classes(["flat"]).build();
                toolbar.append(&b);
            }
            let chrome = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            chrome.append(&strip);
            chrome.append(&toolbar);
            let chrome_rev = gtk4::Revealer::new();
            chrome_rev.set_child(Some(&chrome));
            chrome_rev.set_reveal_child(true);
            let find_rev = gtk4::Revealer::new();
            let status_rev = gtk4::Revealer::new();
            let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            root.append(&chrome_rev);
            root.append(&find_rev);
            root.append(&tv);
            root.append(&status_rev);
            let w = libadwaita::ApplicationWindow::builder()
                .application(app).default_width(700).default_height(400).content(&root).build();
            w.present();
        } else if variant == "f" {
            let bar = libadwaita::TabBar::new();
            bar.set_view(Some(&tv));
            let strip = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            strip.append(&gtk4::WindowControls::new(gtk4::PackType::Start));
            strip.append(&bar);
            strip.append(&gtk4::WindowControls::new(gtk4::PackType::End));
            let toolbar = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
            for icon in ["go-previous-symbolic", "go-next-symbolic", "view-refresh-symbolic"] {
                let b = gtk4::Button::builder().icon_name(icon).css_classes(["flat"]).build();
                toolbar.append(&b);
            }
            let chrome = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            chrome.append(&strip);
            chrome.append(&toolbar);
            let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            root.append(&chrome);
            root.append(&tv);
            let w = libadwaita::ApplicationWindow::builder()
                .application(app).default_width(700).default_height(400).content(&root).build();
            w.present();
        } else {
            let w = libadwaita::ApplicationWindow::builder()
                .application(app).default_width(700).default_height(400).content(&tv).build();
            w.present();
        }
        view.load_uri("xtest://one");
        let n = std::cell::Cell::new(0);
        glib::timeout_add_local(std::time::Duration::from_millis(3000), {
            let view = view.clone();
            move || {
                n.set(n.get() + 1);
                if n.get() > 2 { std::process::exit(0); }
                eprintln!("[nav] -> {}", if n.get() == 1 { "two" } else { "three" });
                view.load_uri(if n.get() == 1 { "xtest://two" } else { "xtest://three" });
                glib::ControlFlow::Continue
            }
        });
    });
    app.run_with_args(&["schemetest"]);
}
