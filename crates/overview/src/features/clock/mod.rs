//! Clock plugin - displays current date and time.
use crate::menu_state::MenuStore;

use anyhow::Result;
use async_trait::async_trait;
use log::{debug, error};
use serde::Deserialize;
use std::cell::RefCell;
use std::process::Command;
use std::rc::Rc;
use std::time::Duration;

use gtk::glib::DateTime;
use gtk::prelude::*;

use crate::plugin::{Plugin, PluginId, Slot, Widget, WidgetRegistrar};
use crate::ui::clock::{ClockOutput, ClockWidget};

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

impl ClockPlugin {
    pub fn new() -> Self {
        Self {
            widget: Rc::new(RefCell::new(None)),
            config: ClockConfig::default(),
        }
    }
}

#[async_trait(?Send)]
impl Plugin for ClockPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::clock")
    }

    fn configure(&mut self, settings: &toml::Table) -> Result<()> {
        self.config = settings.clone().try_into()?;
        debug!("Configured clock plugin: {:?}", self.config);
        Ok(())
    }

    async fn init(&mut self) -> Result<()> {
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
            // No command configured - make widget non-interactive
            clock.root.set_can_focus(false);
            clock.root.set_focusable(false);
            clock.root.set_sensitive(false);
        } else {
            // Command configured - add clickable class and connect click handler
            clock.root.add_css_class("clickable");
            clock.connect_output(move |output| {
                if matches!(output, ClockOutput::Click) {
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
