use adw::prelude::*;
use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use relm4::tokio::spawn;
use relm4::tokio::task::JoinHandle;
use relm4::tokio::time::{Duration, interval};
use relm4::{ComponentParts, ComponentSender, SimpleComponent, gtk};

pub struct CountdownBar {
    ttl: u64,
    elapsed: u64, // elapsed time in milliseconds
    progress: f32,
    last_progress: Cell<f32>,
    running: Arc<AtomicBool>,
    timer: Option<JoinHandle<()>>,
}

pub struct CountdownBarInit {
    pub ttl: u64,
}

#[derive(Debug, Clone)]
pub enum CountdownBarInput {
    Continue,
    Pause,
    Start,
    Stop,
    Tick(u64),
}

#[derive(Debug, Clone)]
pub enum CountdownBarOutput {
    Elapsed,
}

impl CountdownBar {
    pub fn set_width_by_progress(
        &self,
        widgets: &<Self as SimpleComponent>::Widgets,
        progress: f32,
    ) {
        // Avoid redundant work.
        if (self.last_progress.get() - progress).abs() < 0.0001 {
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

    fn start_timer(&mut self, sender: ComponentSender<CountdownBar>) {
        self.stop_timer();
        self.timer = Some(spawn(async move {
            let tick: u64 = 60;
            let mut interval = interval(Duration::from_millis(tick as u64));
            loop {
                interval.tick().await;
                sender.input(CountdownBarInput::Tick(tick));
            }
        }));
    }

    fn stop_timer(&mut self) {
        if let Some(timer) = self.timer.take() {
            timer.abort();
            self.timer = None;
        }
    }
}

#[relm4::component(pub)]
impl SimpleComponent for CountdownBar {
    type Init = CountdownBarInit;
    type Input = CountdownBarInput;
    type Output = CountdownBarOutput;

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
        let model = Self {
            elapsed: 0,
            last_progress: Cell::new(1.0),
            running: Arc::new(AtomicBool::new(false)),
            progress: 1.0,
            timer: None,
            ttl: init.ttl,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            Self::Input::Continue => {
                self.running.store(true, Ordering::Relaxed);
                self.start_timer(sender);
            }
            Self::Input::Pause => {
                self.running.store(false, Ordering::Relaxed);
                self.stop_timer();
            }
            Self::Input::Start => {
                self.running.store(true, Ordering::Relaxed);
                self.elapsed = 0;
                self.progress = 1.0;
                self.start_timer(sender);
            }
            Self::Input::Stop => {
                self.running.store(false, Ordering::Relaxed);
                self.stop_timer();
                self.elapsed = 0;
                self.progress = 1.0;
            }
            Self::Input::Tick(time) => {
                if self.running.load(Ordering::Relaxed) {
                    self.elapsed = (self.elapsed + time).clamp(0, self.ttl);
                    self.last_progress.set(self.progress);
                    self.progress = (self.elapsed as f32 / (self.ttl as f32)).clamp(0.0, 1.0);
                    if self.progress >= 1.0 {
                        self.running.store(false, Ordering::Relaxed);
                        self.stop_timer();
                        sender.output(Self::Output::Elapsed);
                    }
                }
            }
        }
    }

    fn post_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        self.set_width_by_progress(widgets, self.progress);
    }
}
