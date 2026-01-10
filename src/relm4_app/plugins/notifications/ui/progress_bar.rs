use relm4::{ComponentParts, ComponentSender, SimpleComponent, gtk};

use adw::prelude::*;
use std::cell::Cell;

pub struct ProgressBar {
    progress: f32,
    last_progress: Cell<Option<f32>>,
    visible: bool,
}

pub struct ProgressBarInit {
    pub progress: f32,
    pub visible: bool,
}

#[derive(Debug, Clone)]
pub enum ProgressBarInput {
    Progress(f32),
}

#[derive(Debug, Clone)]
pub enum ProgressBarOutput {
    Click,
}

impl ProgressBar {
    /// Update timeout progress by directly providing remaining fraction in [0..=1].
    /// `1.0` means full bar; `0.0` means expired.
    ///
    /// Implementation detail:
    /// We control the fill width inside a `gtk::Fixed` so updates are smooth and don't rely on
    /// widget transform APIs.
    pub fn set_width_by_progress(
        &self,
        widgets: &<Self as SimpleComponent>::Widgets,
        progress: f32,
    ) {
        if !self.visible {
            return;
        }

        // Avoid redundant work.
        if self
            .last_progress
            .get()
            .is_some_and(|prev| (prev - progress).abs() < 0.0001)
        {
            return;
        }

        // Compute target width from the bar's allocated width.
        // Note: if not allocated yet, keep it "full" and let the next tick update precisely.
        let bar_w = widgets.root.allocated_width().max(0);
        if bar_w == 0 {
            widgets.fill.set_size_request(-1, 2);
            widgets.fixer.move_(&widgets.fill, 0.0, 0.0);
            return;
        }

        let target_w = ((bar_w as f32) * progress).round().max(0.0) as i32;

        // Pin to left edge and only change the fill width (height stays 2px).
        widgets.fixer.move_(&widgets.fill, 0.0, 0.0);
        widgets.fill.set_size_request(target_w, 2);
    }
}

#[relm4::component(pub)]
impl SimpleComponent for ProgressBar {
    type Init = ProgressBarInit;
    type Input = ProgressBarInput;
    type Output = ProgressBarOutput;

    view! {
      #[name = "root"]
      gtk::Box {
        set_orientation: gtk::Orientation::Horizontal,
        set_css_classes: &["notification-progress"],
        set_hexpand: true,
        set_vexpand: false,
        set_height_request: 2,

        #[name = "fixer"]
        gtk::Fixed {
          set_hexpand: true,
          set_vexpand: false,
          set_css_classes: &["notification-progress-fix"],

          #[name = "fill"]
          put[0.0, 0.0] = &gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_hexpand: false,
            set_vexpand: false,
            set_height_request: 2,
            set_css_classes: &["notification-progress-fill"]
          }
        }
      }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = ProgressBar {
            last_progress: Cell::new(None),
            progress: init.progress,
            visible: init.visible,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            Self::Input::Progress(progress) => {
                self.last_progress.set(Some(self.progress));
                self.progress = progress.clamp(0.0, 1.0);
            }
        }
    }

    fn post_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        self.set_width_by_progress(widgets, self.progress);
    }
}
