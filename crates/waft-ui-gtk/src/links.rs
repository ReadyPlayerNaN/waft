//! URI launching helpers for clickable links and labels.
//!
//! Spawns `xdg-open` directly rather than going through GIO. GIO's
//! `g_app_info_launch_default_for_uri` uses the default `GdkAppLaunchContext`,
//! which calls `gdk_toplevel_export_handle()` to give the portal a parent
//! window. On Wayland that means `zxdg_exporter_v2::export_toplevel`, which is
//! only legal on `xdg_surface` toplevels — our windows are
//! `wl_layer_surface_v1`, so the compositor treats the request as a protocol
//! violation and terminates our connection.

use std::process::{Command, Stdio};
use std::thread;

use gtk::glib;
use log::warn;

/// Open a URI with the user's default handler. Logs on failure.
pub fn open_uri(uri: &str) {
    match Command::new("xdg-open")
        .arg(uri)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(mut child) => {
            // Reap the child off the main thread so we don't accumulate zombies.
            thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(e) => warn!("[links] failed to spawn xdg-open for {uri}: {e}"),
    }
}

/// Wire a `gtk::Label` so that clicks on its `<a href>` markup links open via
/// `open_uri`, suppressing GTK's default `gtk_show_uri()` handler.
pub fn connect_label_link_handler(label: &gtk::Label) {
    label.connect_activate_link(|_, uri| {
        open_uri(uri);
        glib::Propagation::Stop
    });
}
