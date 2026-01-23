//! Clock plugin - displays current date and time.

use anyhow::Result;
use async_trait::async_trait;
use log::error;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gtk::glib::DateTime;
use gtk::prelude::*;

use crate::plugin::{Plugin, PluginId, Slot, Widget};
use crate::ui::clock::ClockWidget;

pub struct ClockPlugin {
    widget: Rc<RefCell<Option<ClockWidget>>>,
}

impl ClockPlugin {
    pub fn new() -> Self {
        Self {
            widget: Rc::new(RefCell::new(None)),
        }
    }
}

#[async_trait(?Send)]
impl Plugin for ClockPlugin {
    fn id(&self) -> PluginId {
        PluginId::from_static("plugin::clock")
    }

    async fn init(&mut self) -> Result<()> {
        Ok(())
    }

    async fn create_elements(&mut self) -> Result<()> {
        let datetime = DateTime::now_local()?;
        let clock = ClockWidget::new(&datetime);

        // Store the clock widget
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

    fn get_widgets(&self) -> Vec<Arc<Widget>> {
        match *self.widget.borrow() {
            Some(ref clock) => {
                vec![Arc::new(Widget {
                    slot: Slot::Header,
                    el: clock.root.clone().upcast::<gtk::Widget>(),
                    weight: 10,
                })]
            }
            None => vec![],
        }
    }
}
