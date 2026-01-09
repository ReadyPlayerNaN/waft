use crate::relm4_app::plugin::WidgetFeatureToggle;
use std::sync::Arc;

use gtk::prelude::*;
use relm4::prelude::*;

use relm4::{ComponentParts, ComponentSender, SimpleComponent};

#[derive(Debug, Clone)]
pub struct FeatureGridInit {
    pub items: Vec<Arc<WidgetFeatureToggle>>,
}

pub struct FeatureGrid {
    items: Vec<gtk::Widget>,
}

#[relm4::component(pub)]
impl SimpleComponent for FeatureGrid {
    type Init = FeatureGridInit;
    type Input = ();
    type Output = ();

    view! {
      gtk::Box {
        #[local_ref]
        grid -> gtk::Grid {
          set_column_spacing: 12,
          set_row_spacing: 0,
          set_css_classes: &["feature-grid"],
        }
      }
    }

    fn init(
        init: FeatureGridInit,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let items = init.items.into_iter().map(|item| item.el.clone()).collect();
        let cols = 2;

        let model = Self { items };
        let grid = gtk::Grid::new();
        let widgets = view_output!();

        for (i, widget) in model.items.iter().enumerate() {
            let col = (i as i32) % cols;
            let row = (i as i32) / cols;
            grid.attach(widget, col, row, 1, 1);
        }

        ComponentParts { model, widgets }
    }
}
