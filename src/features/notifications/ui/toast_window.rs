use gtk::prelude::{BoxExt, GtkWindowExt, WidgetExt};
use gtk4_layer_shell::LayerShell;
use relm4::gtk;
use relm4::prelude::*;

use super::toast_list::{ToastList, ToastListOutput};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HPos {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VPos {
    Top,
    Center,
    Bottom,
}

pub struct ToastWindow {
    /// Root window (stored so we don't rely on sender APIs to access the widget).
    window: gtk::Window,
    toast_list: Controller<ToastList>,
}

pub struct ToastWindowInit {
    pub hpos: HPos,
    pub vpos: VPos,
}

#[derive(Debug, Clone)]
pub enum ToastWindowInput {
    /// Internal wiring from the `ToastList` child.
    ToastList(ToastListOutput),
}

#[derive(Debug, Clone)]
pub enum ToastWindowOutput {
    ActionClick(u64, String),
    CardClick(u64),
    TimedOut(u64),
}

fn transform_toast_list_output(msg: ToastListOutput) -> ToastWindowInput {
    ToastWindowInput::ToastList(msg)
}

impl ToastWindow {
    fn set_visible_if_needed(&self) {
        let should_be_visible = true;
        if self.window.is_visible() != should_be_visible {
            self.window.set_visible(should_be_visible);
        }
    }

    fn configure_layer_shell(window: &gtk::Window, hpos: HPos, vpos: VPos) {
        // Match the overlay host behavior (`AppModel`):
        // - layer-shell surface
        // - overlay layer so it stays above other windows
        // - focusable (keyboard mode on-demand)
        //
        // NOTE: for toasts we might want to change this later to avoid taking focus.
        window.init_layer_shell();
        window.set_layer(gtk4_layer_shell::Layer::Overlay);
        window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);

        // Reset all anchors to a known baseline.
        window.set_anchor(gtk4_layer_shell::Edge::Left, false);
        window.set_anchor(gtk4_layer_shell::Edge::Right, false);
        window.set_anchor(gtk4_layer_shell::Edge::Top, false);
        window.set_anchor(gtk4_layer_shell::Edge::Bottom, false);

        match hpos {
            HPos::Left => window.set_anchor(gtk4_layer_shell::Edge::Left, true),
            HPos::Right => window.set_anchor(gtk4_layer_shell::Edge::Right, true),
            HPos::Center => {
                // With neither left nor right anchored, the compositor is free to center.
                // This mirrors the approach used by the main overlay.
            }
        }

        match vpos {
            VPos::Top => window.set_anchor(gtk4_layer_shell::Edge::Top, true),
            VPos::Bottom => window.set_anchor(gtk4_layer_shell::Edge::Bottom, true),
            VPos::Center => {
                // With neither top nor bottom anchored, the compositor is free to center.
            }
        }

        // Explicitly set 0 margins per requirement.
        window.set_margin(gtk4_layer_shell::Edge::Left, 0);
        window.set_margin(gtk4_layer_shell::Edge::Right, 0);
        window.set_margin(gtk4_layer_shell::Edge::Top, 0);
        window.set_margin(gtk4_layer_shell::Edge::Bottom, 0);
    }
}

#[relm4::component(pub)]
impl SimpleComponent for ToastWindow {
    type Init = ToastWindowInit;
    type Input = ToastWindowInput;
    type Output = ToastWindowOutput;
    type Widgets = ToastWindowWidgets;

    view! {
        root = gtk::Window {
            set_title: Some("sacrebleui toast window"),
            set_decorated: false,
            set_hide_on_close: true,
            set_modal: false,

            set_default_width: 480,
            set_resizable: false,
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        Self::configure_layer_shell(&root, init.hpos, init.vpos);
        root.set_application(Some(&relm4::main_application()));

        let toast_list = ToastList::builder()
            .launch(())
            .forward(sender.input_sender(), transform_toast_list_output);

        // Window content.
        let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
        content.append(toast_list.widget());

        root.set_child(Some(&content));

        let model = Self {
            window: root.clone(),
            toast_list,
        };

        // Hidden when empty, visible otherwise.
        model.set_visible_if_needed();

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            ToastWindowInput::ToastList(out) => match out {
                ToastListOutput::ActionClick(id, action) => {
                    let _ = sender.output(ToastWindowOutput::ActionClick(id, action));
                }
                ToastListOutput::CardClick(id) => {
                    let _ = sender.output(ToastWindowOutput::CardClick(id));
                }
                ToastListOutput::CardTimedOut(id) => {
                    // Plugin is the source of truth: treat timeout as a signal/intent.
                    // The plugin will decide whether/when to emit `ToastWindowInput::Remove(id)`.
                    let _ = sender.output(ToastWindowOutput::TimedOut(id));
                }
            },
        }
    }
}
