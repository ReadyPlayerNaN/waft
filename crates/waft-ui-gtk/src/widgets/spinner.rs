//! Spinner widget

use gtk::prelude::*;

/// GTK4 spinner widget.
pub struct SpinnerWidget {
    spinner: gtk::Spinner,
}

impl SpinnerWidget {
    pub fn new(spinning: bool) -> Self {
        let spinner = gtk::Spinner::new();
        if spinning {
            spinner.start();
        }
        Self { spinner }
    }

    pub fn set_spinning(&self, spinning: bool) {
        if spinning {
            self.spinner.start();
        } else {
            self.spinner.stop();
        }
    }
}

impl crate::widget_base::WidgetBase for SpinnerWidget {
    fn widget(&self) -> gtk::Widget {
        self.spinner.clone().upcast()
    }
}
