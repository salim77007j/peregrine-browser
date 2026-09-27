//! Downloads: the DownloadHub (App-level) owns WebKit download objects, applies
//! safe-download handling, and records metadata; windows show a popover + badge.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use gtk4::glib::{self, clone};
use gtk4::prelude::*;
use webkit6::prelude::*;

use crate::app::App;
use crate::util;
use crate::window::BrowserWindow;

pub struct LiveDownload {
    pub dl: webkit6::Download,
    pub path: std::path::PathBuf,
    pub store_id: i64,
}

pub struct DownloadHub {
    live: Mutex<HashMap<i64, LiveDownload>>,
    counter: AtomicI64,
}

impl DownloadHub {
    pub fn new() -> Self {
        Self {
            live: Mutex::new(HashMap::new()),
            counter: AtomicI64::new(1),
        }
    }

    pub fn active_count(&self) -> i64 {
        self.live.lock().map(|m| m.len() as i64).unwrap_or(0)
    }
}

/// Attach the global download pipeline to the network session. Called once at startup.
pub fn wire_hub(app: &Arc<App>) {
    let session = app.session.clone();
    let app_owned = app.clone();
    session.connect_download_started(move |_s, dl| {
        handle_download(&app_owned, dl);
    });
}

fn handle_download(app: &Arc<App>, dl: &webkit6::Download) {
    // ---- sanitize the suggested filename ----
    let suggested = dl
        .response()
        .and_then(|r| r.suggested_filename())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "download".into());
    let safe_name = sanitize_filename(&suggested);
    if safe_name.is_empty() {
        dl.cancel();
        return;
    }

    let prefs = app.data.prefs.get();
    let dir = if prefs.downloads_dir.trim().is_empty() {
        util::downloads_dir()
    } else {
        std::path::PathBuf::from(prefs.downloads_dir.trim())
    };
    let _ = std::fs::create_dir_all(&dir);
    let mut path = dir.join(&safe_name);
    let mut n = 1;
    while path.exists() {
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "download".into());
        let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        path = dir.join(format!("{}-{}{}", stem, n, ext));
        n += 1;
    }

    // record in store
    let url = dl.request().and_then(|r| r.uri()).map(|u| u.to_string()).unwrap_or_default();
    let mime = dl.response().and_then(|r| r.mime_type()).map(|m| m.to_string()).unwrap_or_default();
    let rec = crate::data::vault::DownloadRecord {
        id: 0,
        url,
        path: path.to_string_lossy().to_string(),
        mime,
        size: 0,
        state: "running".into(),
        started: chrono::Utc::now().timestamp(),
    };
    let store_id = app.data.downloads.insert(&rec);

    let id = app.download_hub.counter.fetch_add(1, Ordering::SeqCst);
    app.download_hub
        .live
        .lock()
        .map(|mut m| {
            m.insert(
                id,
                LiveDownload {
                    dl: dl.clone(),
                    path: path.clone(),
                    store_id,
                },
            )
        })
        .ok();

    // ---- destination decision ----
    let dest_uri = format!("file://{}", path.to_string_lossy());
    let path_dest = path.clone();
    dl.connect_decide_destination(move |dl, suggested| {
        let _ = suggested;
        let uri = format!("file://{}", path_dest.to_string_lossy());
        dl.set_destination(&uri);
        true
    });

    // ---- progress ----
    let app_rd = app.clone();
    dl.connect_received_data(move |dl, _data| {
        let len = dl.received_data_length();
        if let Some(hub_id) = find_hub_id(&app_rd, dl) {
            let live = app_rd.download_hub.live.lock().ok().and_then(|m| m.get(&hub_id).map(|l| l.store_id));
            if let Some(store_id) = live {
                app_rd.data.downloads.update(store_id, len as i64, "running");
            }
        }
    });

    // ---- completion ----
    let app_fin = app.clone();
    dl.connect_finished(move |dl| {
        let size = dl.received_data_length();
        if let Some(hub_id) = find_hub_id(&app_fin, dl) {
            let live = app_fin.download_hub.live.lock().ok().and_then(|mut m| m.remove(&hub_id).map(|l| (l.store_id, l.path.clone())));
            if let Some((store_id, path)) = live {
                app_fin.data.downloads.update(store_id, size as i64, "finished");
                harden_file(&path);
            }
        }
    });

    let app_fail = app.clone();
    dl.connect_failed(move |dl, _err| {
        if let Some(hub_id) = find_hub_id(&app_fail, dl) {
            let live = app_fail.download_hub.live.lock().ok().and_then(|mut m| m.remove(&hub_id).map(|l| l.store_id));
            if let Some(store_id) = live {
                app_fail.data.downloads.update(store_id, 0, "failed");
            }
        }
    });

    let _ = dest_uri;
}

fn find_hub_id(app: &Arc<App>, dl: &webkit6::Download) -> Option<i64> {
    let m = app.download_hub.live.lock().ok()?;
    m.iter().find(|(_, l)| l.dl == *dl).map(|(k, _)| *k)
}

/// Strip path separators, control chars and leading dots; cap length.
pub fn sanitize_filename(name: &str) -> String {
    // take the basename first (a download never escapes the target dir)
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = base
        .chars()
        .map(|c| if c.is_control() || c == '\0' { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_start_matches(['.', '/', '\\']);
    let mut out = String::new();
    for c in trimmed.chars().take(120) {
        out.push(c);
    }
    out.trim().to_string()
}

/// Post-download hardening: remove executable bits (downloaded files should not
/// execute without an explicit user decision), mark common script types.
fn harden_file(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mut perms = meta.permissions();
            if perms.mode() & 0o111 != 0 {
                perms.set_mode(perms.mode() & !0o111);
                let _ = std::fs::set_permissions(path, perms);
            }
        }
    }
}

/// Show the downloads popover anchored to the toolbar button.
pub fn show_downloads_popover(win: &'static BrowserWindow, app: &Arc<App>, anchor: &gtk4::Button) {
    let popover = gtk4::Popover::new();
    let box_ = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    box_.set_margin_top(8);
    box_.set_margin_bottom(8);
    box_.set_margin_start(8);
    box_.set_margin_end(8);
    box_.set_spacing(4);

    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    let title = gtk4::Label::new(Some("<b>Downloads</b>"));
    title.set_use_markup(true);
    let spacer = gtk4::Label::new(Some(""));
    spacer.set_hexpand(true);
    let open_page = gtk4::LinkButton::with_label("peregrine://downloads", "Show all");
    let clear = gtk4::Button::with_label("Clear");
    clear.add_css_class("flat");
    header.append(&title);
    header.append(&spacer);
    header.append(&open_page);
    header.append(&clear);
    box_.append(&header);

    let list_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    box_.append(&list_box);

    let rows = gtk4::ScrolledWindow::new();
    rows.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    rows.set_max_content_height(340);
    rows.set_propagate_natural_height(true);
    rows.set_child(Some(&list_box));
    box_.append(&rows);

    // periodic refresh while the popover is open
    let list_ref = list_box.downgrade();
    let app_ref = Arc::downgrade(app);
    let pop_ref = popover.downgrade();
    let tick = move || {
        let Some(list_box) = list_ref.upgrade() else { return glib::ControlFlow::Continue };
        if pop_ref.upgrade().is_none() {
            return glib::ControlFlow::Break;
        }
        let Some(app) = app_ref.upgrade() else { return glib::ControlFlow::Continue };
        rebuild_rows(&list_box, &app);
        glib::ControlFlow::Continue
    };
    glib::timeout_add_local(std::time::Duration::from_secs(1), tick);

    let app_c = app.clone();
    let list_c = list_box.clone();
    let refresh = move || rebuild_rows(&list_c, &app_c);
    let app_clear = app.clone();
    let list_clear = list_box.clone();
    clear.connect_clicked(move |_| {
        app_clear.data.downloads.clear();
        rebuild_rows(&list_clear, &app_clear);
    });
    let _ = refresh;

    let pop_link = popover.downgrade();
    open_page.connect_activate_link(move |_b| {
        if let Some(pop) = pop_link.upgrade() {
            pop.popdown();
        }
        win.new_tab_url("peregrine://downloads");
        glib::Propagation::Stop
    });

    popover.set_child(Some(&box_));
    popover.set_pointing_to(None::<&gtk4::gdk::Rectangle>);
    popover.set_offset(0, 6);
    popover.popup();
}

/// (helper) rebuild the popover rows from the download store
fn rebuild_rows(list_box: &gtk4::Box, app: &Arc<App>) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }
    for d in app.data.downloads.list(8) {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        row.set_margin_start(4);
        row.set_margin_end(4);
        let name = gtk4::Label::new(Some(&crate::util::ellipsize(
            &d.path.rsplit('/').next().unwrap_or("download"),
            34,
        )));
        name.set_halign(gtk4::Align::Start);
        let info = gtk4::Label::new(Some(&format!("{} · {}", d.state, crate::util::human_bytes(d.size.max(0) as u64))));
        info.add_css_class("dim-label");
        info.set_halign(gtk4::Align::Start);
        info.set_hexpand(true);
        let open_btn = gtk4::Button::from_icon_name("folder-symbolic");
        open_btn.add_css_class("flat");
        open_btn.set_tooltip_text(Some("Reveal in folder"));
        let p = std::path::PathBuf::from(&d.path);
        open_btn.connect_clicked(move |_| {
            reveal_path(&p);
        });
        row.append(&name);
        row.append(&info);
        row.append(&open_btn);
        list_box.append(&row);
    }
    if list_box.first_child().is_none() {
        let empty = gtk4::Label::new(Some("No downloads yet"));
        empty.add_css_class("dim-label");
        list_box.append(&empty);
    }
}

fn reveal_path(path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_sanitized() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd"); // basename only — no traversal
        assert_eq!(sanitize_filename("report.pdf"), "report.pdf");
        assert_eq!(sanitize_filename(".hidden"), "hidden");
        assert_eq!(sanitize_filename("a/b\\c.txt"), "c.txt");
    }
}
