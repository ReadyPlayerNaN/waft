use std::collections::HashSet;
use std::sync::Arc;

use gtk::prelude::{BoxExt, GtkWindowExt, WidgetExt};
use gtk4_layer_shell::LayerShell;
use relm4::gtk;
use relm4::prelude::*;

use super::super::types::NotificationDisplay;
use super::toast_list::{ToastList, ToastListInit, ToastListInput, ToastListOutput};

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

    /// Track ids we believe are currently present (source of truth is the plugin).
    ///
    /// We use this to decide whether the window should be visible.
    ///
    /// NOTE:
    /// `ToastList` currently removes on `TimedOut` internally but does not emit an output for it.
    /// In the "plugin is SoT" model, expiry should be decided by the plugin and propagated via
    /// `ToastWindowInput::Remove`, so this remains correct.
    present_ids: HashSet<u64>,

    toast_list: Controller<ToastList>,
}

pub struct ToastWindowInit {
    pub hpos: HPos,
    pub vpos: VPos,
    pub notifications: Vec<Arc<NotificationDisplay>>,
}

#[derive(Debug, Clone)]
pub enum ToastWindowInput {
    /// Ingest a new/updated notification (plugin-driven).
    Ingest(Arc<NotificationDisplay>),

    /// Remove a notification by id (plugin-driven).
    Remove(u64),

    /// Internal wiring from the `ToastList` child.
    ToastList(ToastListOutput),
}

#[derive(Debug, Clone)]
pub enum ToastWindowOutput {
    ActionClick(u64, String),
    CardClick(u64),
    CardClose(u64),
    TimedOut(u64),

    Collapse(Arc<str>),
    Expand(Arc<str>),
}

fn transform_toast_list_output(msg: ToastListOutput) -> ToastWindowInput {
    ToastWindowInput::ToastList(msg)
}

impl ToastWindow {
    fn set_visible_if_needed(&self) {
        let should_be_visible = !self.present_ids.is_empty();
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

        // Associate this window with the main GTK application.
        //
        // IMPORTANT:
        // The plugin/host is responsible for calling `add_window(...)` so it controls lifecycle.
        root.set_application(Some(&relm4::main_application()));

        let mut present_ids: HashSet<u64> = HashSet::with_capacity(init.notifications.len());
        for n in init.notifications.iter() {
            present_ids.insert(n.id);
        }

        let toast_list = ToastList::builder()
            .launch(ToastListInit {
                notifications: Some(init.notifications),
                // Required by `ToastList` but not part of ToastWindow API.
                id: Arc::from("toast-window"),
                title: Arc::from("Toasts"),
            })
            .forward(sender.input_sender(), transform_toast_list_output);

        // Window content.
        let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
        content.append(toast_list.widget());

        root.set_child(Some(&content));

        let model = Self {
            window: root.clone(),
            present_ids,
            toast_list,
        };

        // Hidden when empty, visible otherwise.
        model.set_visible_if_needed();

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            ToastWindowInput::Ingest(notification) => {
                // Plugin is the source of truth; we keep a light local index for visibility decisions.
                self.present_ids.insert(notification.id);

                self.toast_list
                    .sender()
                    .emit(ToastListInput::Ingest(notification));

                self.set_visible_if_needed();
            }

            ToastWindowInput::Remove(id) => {
                self.present_ids.remove(&id);

                self.toast_list.sender().emit(ToastListInput::Remove(id));

                self.set_visible_if_needed();
            }

            ToastWindowInput::ToastList(out) => match out {
                ToastListOutput::ActionClick(id, action) => {
                    let _ = sender.output(ToastWindowOutput::ActionClick(id, action));
                }
                ToastListOutput::CardClick(id) => {
                    let _ = sender.output(ToastWindowOutput::CardClick(id));
                }
                ToastListOutput::CardClose(id) => {
                    // Plugin is the source of truth: do not mutate local presence state here.
                    // The plugin will decide whether/when to emit `ToastWindowInput::Remove(id)`.
                    let _ = sender.output(ToastWindowOutput::CardClose(id));
                }
                ToastListOutput::TimedOut(id) => {
                    // Plugin is the source of truth: treat timeout as a signal/intent.
                    // The plugin will decide whether/when to emit `ToastWindowInput::Remove(id)`.
                    let _ = sender.output(ToastWindowOutput::TimedOut(id));
                }
                ToastListOutput::Collapse(group_id) => {
                    let _ = sender.output(ToastWindowOutput::Collapse(group_id));
                }
                ToastListOutput::Expand(group_id) => {
                    let _ = sender.output(ToastWindowOutput::Expand(group_id));
                }
            },
        }
    }
}
