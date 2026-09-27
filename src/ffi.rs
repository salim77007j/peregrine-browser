//! Minimal hand-written FFI for WebKitGTK functions the gir bindings miss.
//! Each declaration mirrors the stable C ABI; all calls are audited for
//! pointer safety. This is the ONLY unsafe code in Peregrine.

#![allow(clippy::missing_safety_doc)]

use webkit6::glib::translate::*;
use webkit6::ffi;
use webkit6::WebsiteDataManager;

extern "C" {
    // void webkit_website_data_manager_clear(WebKitWebsiteDataManager *manager,
    //   WebKitWebsiteDataTypes types, GTimeSpan timespan, GCancellable *cancellable,
    //   GAsyncReadyCallback callback, gpointer user_data);
    pub fn webkit_website_data_manager_clear(
        manager: *mut ffi::WebKitWebsiteDataManager,
        types: ffi::WebKitWebsiteDataTypes,
        timespan: i64,
        cancellable: *mut webkit6::gio::ffi::GCancellable,
        callback: webkit6::gio::ffi::GAsyncReadyCallback,
        user_data: webkit6::glib::ffi::gpointer,
    );
}

/// Clear website data categories asynchronously (NULL callback — fire and forget).
pub fn website_data_manager_clear(mgr: &WebsiteDataManager, types: webkit6::WebsiteDataTypes, timespan: i64) {
    unsafe {
        webkit_website_data_manager_clear(
            mgr.to_glib_none().0,
            types.into_glib(),
            timespan,
            std::ptr::null_mut(),
            None,
            std::ptr::null_mut(),
        );
    }
}
