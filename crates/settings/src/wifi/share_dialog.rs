//! WiFi QR code share dialog.
//!
//! Displays a QR code encoding the WiFi network credentials so that other
//! devices can scan and connect. Uses the `qrcode` crate for QR generation
//! and renders via cairo onto a `gtk::DrawingArea`.

use adw::prelude::*;

use crate::i18n::{t, t_args};

/// Show a dialog with a QR code for the given WiFi network.
///
/// `ssid` is the network name shown as the dialog title.
/// `qr_string` is the WiFi URI string (e.g. `WIFI:T:WPA;S:MyNet;P:pass123;;`).
pub fn show_share_dialog(parent: &impl IsA<gtk::Widget>, ssid: &str, qr_string: &str) {
    let dialog = adw::Dialog::builder()
        .title(t_args("wifi-share-title", &[("ssid", ssid)]))
        .content_width(360)
        .content_height(440)
        .build();

    let header = adw::HeaderBar::new();

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(16)
        .margin_top(16)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .halign(gtk::Align::Center)
        .build();

    // Render QR code to a matrix
    let qr_matrix = match qrcode::QrCode::new(qr_string.as_bytes()) {
        Ok(code) => {
            let matrix = code.to_colors();
            let width = code.width();
            Some((matrix, width))
        }
        Err(e) => {
            log::warn!("[wifi-share] failed to generate QR code: {e}");
            None
        }
    };

    if let Some((matrix, qr_width)) = qr_matrix {
        let drawing_area = gtk::DrawingArea::builder()
            .content_width(280)
            .content_height(280)
            .halign(gtk::Align::Center)
            .build();

        drawing_area.set_draw_func(move |_area, cr, width, height| {
            let quiet_zone = 2;
            let total = qr_width + 2 * quiet_zone;
            let cell_size = (width.min(height) as f64) / total as f64;

            // White background
            cr.set_source_rgb(1.0, 1.0, 1.0);
            if let Err(e) = cr.paint() {
                log::warn!("[wifi-share] cairo paint error: {e}");
                return;
            }

            // Draw dark modules
            cr.set_source_rgb(0.0, 0.0, 0.0);
            for y in 0..qr_width {
                for x in 0..qr_width {
                    let idx = y * qr_width + x;
                    if matrix[idx] == qrcode::Color::Dark {
                        let px = (x + quiet_zone) as f64 * cell_size;
                        let py = (y + quiet_zone) as f64 * cell_size;
                        cr.rectangle(px, py, cell_size, cell_size);
                    }
                }
            }
            if let Err(e) = cr.fill() {
                log::warn!("[wifi-share] cairo fill error: {e}");
            }
        });

        content.append(&drawing_area);
    }

    let description = gtk::Label::builder()
        .label(t("wifi-share-qr-description"))
        .wrap(true)
        .justify(gtk::Justification::Center)
        .css_classes(["dim-label"])
        .build();
    content.append(&description);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&content));
    dialog.set_child(Some(&toolbar));

    dialog.present(Some(parent));
}
