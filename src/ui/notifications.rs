/*!
Notifications widget with grouped notifications and controls.

This module provides a reusable notifications widget that includes:
- Grouped notifications with app names, summaries, bodies, and actions
- Do Not Disturb toggle switch
- Clear all notifications button

The widget follows Adwaita design patterns and integrates with the main overlay UI.
*/

use adw::prelude::*;

/// Represents a single notification with its data
#[derive(Clone, Debug)]
pub struct Notification {
    pub app_name: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<String>,
}

/// Build a complete notifications section with controls and custom data
pub fn build_notifications_section(notifications: Vec<Notification>) -> gtk::Widget {
    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .build();

    // Notifications section (grouped by app name, with actions like GNOME).
    let notif_group = adw::PreferencesGroup::builder()
        .title("Notifications")
        .build();

    // Helper to add a notification "card".
    let add_notif = |group: &adw::PreferencesGroup, notification: &Notification| {
        // We render a custom widget so actions can be placed *under* title/text:
        //
        // Title
        // text
        // button1 | button2
        let row = adw::ActionRow::builder().build();
        row.set_activatable(false);

        // App name as prefix label.
        let app_badge = gtk::Label::builder()
            .label(&notification.app_name)
            .css_classes(["caption", "dim-label"])
            .xalign(0.0)
            .build();
        row.add_prefix(&app_badge);

        // Add padding so text isn't flush against row borders.
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .hexpand(true)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();

        let title = gtk::Label::builder()
            .label(&notification.summary)
            .xalign(0.0)
            .wrap(true)
            .css_classes(["heading"])
            .build();

        let text = gtk::Label::builder()
            .label(&notification.body)
            .xalign(0.0)
            .wrap(true)
            .css_classes(["dim-label"])
            .build();

        // Create separate actions box for each notification to avoid parent conflicts
        let actions_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .build();

        for a in &notification.actions {
            let b = gtk::Button::builder()
                .label(a)
                .css_classes(["pill", "notif-action"])
                .build();
            actions_box.append(&b);
        }

        content.append(&title);
        content.append(&text);
        content.append(&actions_box);

        // Put vertical content in row itself.
        row.set_child(Some(&content));

        group.add(&row);
    };

    for notification in &notifications {
        add_notif(&notif_group, notification);
    }

    root.append(&notif_group);

    // Notifications controls section
    let notif_controls = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .margin_top(8)
        .build();

    // Do Not Disturb switch
    let dnd_label = gtk::Label::builder()
        .label("Do Not Disturb")
        .hexpand(true)
        .xalign(0.0)
        .build();

    let dnd_switch = gtk::Switch::builder().halign(gtk::Align::End).build();

    // Clear button
    let clear_btn = gtk::Button::builder()
        .label("Clear")
        .css_classes(["destructive-action"])
        .build();

    // Connect DND switch handler
    dnd_switch.connect_active_notify({
        let dnd_label = dnd_label.clone();
        move |switch_| {
            if switch_.is_active() {
                dnd_label.set_css_classes(&["caption", "dim-label"]);
                // In real implementation, you'd enable DND mode here
            } else {
                dnd_label.set_css_classes(&["caption"]);
                // In real implementation, you'd disable DND mode here
            }
        }
    });

    // Connect Clear button handler
    clear_btn.connect_clicked({
        move |_| {
            // In real implementation, you'd clear all notifications here
            println!("Clear all notifications");
        }
    });

    notif_controls.append(&clear_btn);
    notif_controls.append(&dnd_label);
    notif_controls.append(&dnd_switch);

    root.append(&notif_controls);

    root.upcast::<gtk::Widget>()
}
