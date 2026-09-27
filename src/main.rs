//! Peregrine — an ultra-fast, privacy-first web browser built in Rust.
//!
//! Binary entry point: CLI parsing, app bootstrap, lifecycle.

mod app;
mod data;
mod downloads;
mod ffi;
mod headless;
mod menu;
mod omnibox;
mod pages;
mod privacy;
mod shortcuts;
mod tabs;
mod theme;
mod util;
mod window;
mod window_ops;
mod webview;

use gtk4::glib;
use gtk4::prelude::*;

fn print_version() {
    println!("Peregrine {} — the fastest browser on Earth", env!("CARGO_PKG_VERSION"));
    println!("WebKitGTK 6.0 engine · GTK4/libadwaita UI · Rust core");
}

fn usage() {
    print_version();
    println!();
    println!("Usage: peregrine [OPTIONS] [URL]");
    println!();
    println!("Options:");
    println!("  --profile DIR    use a custom profile directory");
    println!("  --self-test      run the built-in end-to-end test suite (for CI)");
    println!("  --new-window     open the URL in a new window");
    println!("  --version        print version");
    println!("  --help           show this help");
    println!();
    println!("Environment:");
    println!("  PEREGRINE_PROFILE     profile directory override");
    println!("  PEREGRINE_DOWNLOADS   default download directory");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut url: Option<String> = None;
    let mut new_window = false;
    let mut self_test = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--version" | "-v" => {
                print_version();
                return;
            }
            "--help" | "-h" => {
                usage();
                return;
            }
            "--self-test" => self_test = true,
            "--new-window" => new_window = true,
            "--profile" => {
                if i + 1 < args.len() {
                    std::env::set_var("PEREGRINE_PROFILE", &args[i + 1]);
                    i += 1;
                }
            }
            a if a.starts_with('-') => {
                eprintln!("unknown option: {a} (see --help)");
            }
            a => url = Some(a.to_string()),
        }
        i += 1;
    }

    // ---- bootstrap ----
    if self_test {
        // deterministic engine rules for the self-test (applied before engine build)
        std::env::set_var("PEREGRINE_SELFTEST", "1");
    }
    let app = app::App::bootstrap();

    // CLI URL: hand it to the activated window
    if let Some(u) = url.clone() {
        let app2 = app.clone();
        app.app.connect_activate(move |_| {
            if new_window {
                window_ops::open_new_window(&app2, Some(&u));
            } else {
                app2.open_url(&u, false);
            }
        });
    }

    // theme must load before windows appear
    app.app.connect_activate({
        let app = app.clone();
        move |_| {
            theme::load();
        }
    });

    // self-test mode
    if self_test {
        // windows must be created inside the activate handler (GApplication rule);
        // the self-test installs its own timers and exit code.
        let app2 = app.clone();
        app.app.connect_activate(move |_| {
            headless::run(app2.clone());
        });
    }

    // ---- post-construction wiring ----
    pages::register(app.clone(), &app.context);
    downloads::wire_hub(&app);

    // graceful shutdown on SIGINT/SIGTERM (glib 0.22 has no unix_signal_add;
    // libc handler + polled flag — async-signal-safe)
    static SHUTDOWN_FLAG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    extern "C" fn on_signal(_sig: libc::c_int) {
        SHUTDOWN_FLAG.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    unsafe {
        libc::signal(libc::SIGINT, on_signal as usize);
        libc::signal(libc::SIGTERM, on_signal as usize);
    }
    let app_poll = app.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
        if SHUTDOWN_FLAG.load(std::sync::atomic::Ordering::SeqCst) {
            app_poll.clean_shutdown();
            std::process::exit(0);
        }
        glib::ControlFlow::Continue
    });

    // default activation: open/restore windows (skipped in self-test mode)
    if !self_test {
        let app3 = app.clone();
        app.app.connect_activate(move |_| {
            window_ops::activate(&app3);
        });
    }

    // shutdown hook (clean exit path)
    let app4 = app.clone();
    app.app.connect_shutdown(move |_| {
        app4.clean_shutdown();
    });

    // Run the GTK application loop with NO argv: our custom CLI options were
    // parsed manually above, and GApplication's GOption parser would reject them.
    app.app.run_with_args(&["peregrine"]);
}
