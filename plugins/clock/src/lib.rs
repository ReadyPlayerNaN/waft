//! Clock plugin — displays current date and time.
//!
//! This is a dynamic plugin (.so) loaded by waft-overview at runtime.

use std::cell::RefCell;
use std::process::Command;
use std::rc::Rc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use gtk::glib::{self, DateTime, GString};
use gtk::prelude::*;
use log::{debug, error, warn};
use serde::Deserialize;

use waft_core::menu_state::MenuStore;
use waft_plugin_api::{OverviewPlugin, PluginId, PluginResources, Slot, Widget, WidgetRegistrar};

// Export plugin entry points.
waft_plugin_api::export_plugin_metadata!("plugin::clock", "Clock", "0.1.0");
waft_plugin_api::export_overview_plugin!(ClockPlugin::new());

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

/// Configuration for the clock plugin.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct ClockConfig {
    /// Command to run when the clock is clicked. Empty means no action.
    #[serde(default)]
    pub on_click: String,
}

pub struct ClockPlugin {
    widget: Rc<RefCell<Option<ClockWidget>>>,
    config: ClockConfig,
}

impl Default for ClockPlugin {
    fn default() -> Self {
        Self {
            widget: Rc::new(RefCell::new(None)),
            config: ClockConfig::default(),
        }
    }
}

impl ClockPlugin {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait(?Send)]
impl OverviewPlugin for ClockPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::clock")
    }

    fn configure(&mut self, settings: &toml::Table) -> Result<()> {
        self.config = settings.clone().try_into()?;
        debug!("Configured clock plugin: {:?}", self.config);
        Ok(())
    }

    async fn init(&mut self, _resources: &PluginResources) -> Result<()> {
        Ok(())
    }

    async fn create_elements(
        &mut self,
        _app: &gtk::Application,
        _menu_store: Rc<MenuStore>,
        registrar: Rc<dyn WidgetRegistrar>,
    ) -> Result<()> {
        let datetime = DateTime::now_local()?;
        let clock = ClockWidget::new(&datetime);

        // Configure click behavior based on on_click setting
        let on_click_cmd = self.config.on_click.clone();
        if on_click_cmd.is_empty() {
            // No command configured — make widget non-interactive
            clock.root.set_can_focus(false);
            clock.root.set_focusable(false);
            clock.root.set_sensitive(false);
        } else {
            // Command configured — add clickable class and connect click handler
            clock.root.add_css_class("clickable");
            clock.root.connect_clicked(move |_| {
                debug!("Clock clicked, running command: {}", on_click_cmd);
                match Command::new("sh").arg("-c").arg(&on_click_cmd).spawn() {
                    Ok(child) => {
                        // Reap the child in a background thread to avoid zombies
                        std::thread::spawn(move || {
                            let mut child = child;
                            if let Err(e) = child.wait() {
                                error!("[clock] on_click child wait error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Failed to run clock on_click command: {}", e);
                    }
                }
            });
        }

        // Register the widget
        registrar.register_widget(Rc::new(Widget {
            id: "clock:main".to_string(),
            slot: Slot::Header,
            el: clock.root.clone().upcast::<gtk::Widget>(),
            weight: 10,
        }));

        // Store the clock widget for tick updates
        *self.widget.borrow_mut() = Some(clock);

        // Schedule tick updates
        let widget_ref = self.widget.clone();
        glib::timeout_add_local(Duration::from_secs(1), move || {
            match DateTime::now_local() {
                Ok(datetime) => {
                    if let Some(ref clock) = *widget_ref.borrow() {
                        clock.tick(&datetime);
                    }
                }
                Err(err) => {
                    error!("[clock] Failed to get current datetime: {:?}", err);
                }
            };
            glib::ControlFlow::Continue
        });

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Widget (self-contained, no dependency on overview's UI modules)
// ---------------------------------------------------------------------------

/// Pure GTK4 clock widget — displays date and time.
struct ClockWidget {
    root: gtk::Button,
    date_label: gtk::Label,
    time_label: gtk::Label,
}

impl ClockWidget {
    /// Create a new clock widget with the given initial datetime.
    fn new(datetime: &DateTime) -> Self {
        let root = gtk::Button::builder().css_classes(["clock-btn"]).build();

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .css_classes(["clock-container"])
            .build();

        let date_label = gtk::Label::builder()
            .label(Self::format_date(datetime))
            .xalign(0.0)
            .css_classes(["title-3", "dim-label", "clock-date"])
            .build();

        let time_label = gtk::Label::builder()
            .label(Self::format_time(datetime))
            .xalign(0.0)
            .css_classes(["title-1", "clock-time"])
            .build();

        content.append(&date_label);
        content.append(&time_label);
        root.set_child(Some(&content));

        Self {
            root,
            date_label,
            time_label,
        }
    }

    /// Update the displayed time.
    fn tick(&self, datetime: &DateTime) {
        self.date_label.set_label(&Self::format_date(datetime));
        self.time_label.set_label(&Self::format_time(datetime));
    }

    fn format_datetime_str(d: &DateTime, format: &str) -> GString {
        match d.format(format) {
            Ok(s) => s,
            Err(_e) => {
                warn!("Failed to format datetime with format: {}", format);
                "".into()
            }
        }
    }

    fn format_date(d: &DateTime) -> String {
        Self::format_datetime_str(d, "%a, %d %b %Y").to_string()
    }

    fn format_time(d: &DateTime) -> String {
        Self::format_datetime_str(d, "%H:%M").to_string()
    }
}
