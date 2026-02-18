//! Unified Events component combining Calendar and Agenda.
//!
//! Wraps a collapsible calendar grid and a headerless agenda in a single
//! container with shared header controls. Calendar is hidden by default
//! and revealed by a toggle button.

use std::rc::Rc;

use gtk::prelude::*;

use crate::calendar_selection::{CalendarSelectionOp, CalendarSelectionStore};
use crate::components::agenda::AgendaComponent;
use crate::components::calendar_grid::CalendarComponent;
use crate::menu_state::MenuStore;
use crate::ui::main_window::trigger_window_resize;
use waft_client::EntityStore;
use waft_protocol::entity;
use waft_ui_gtk::widget_base::WidgetBase as _;
use waft_ui_gtk::widgets::spinner::SpinnerWidget;

pub struct EventsComponent {
    container: gtk::Box,
    _calendar: CalendarComponent,
    _agenda: AgendaComponent,
}

impl EventsComponent {
    pub fn new(
        store: &Rc<EntityStore>,
        menu_store: &Rc<MenuStore>,
        selection_store: &Rc<CalendarSelectionStore>,
    ) -> Self {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .build();

        // Create child components
        let calendar = CalendarComponent::new(store, selection_store);
        let agenda = AgendaComponent::new(store, menu_store, selection_store, false);

        // Header row: title + controls
        let header = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .build();

        let title_label = gtk::Label::builder()
            .label(crate::i18n::t("events-title"))
            .xalign(0.0)
            .hexpand(true)
            .css_classes(["title-3", "agenda-header"])
            .build();

        let calendar_toggle = gtk::ToggleButton::builder()
            .icon_name("x-office-calendar-symbolic")
            .tooltip_text(crate::i18n::t("events-show-calendar-tooltip"))
            .css_classes(["flat"])
            .active(false)
            .build();

        let past_btn = agenda.past_events_button().clone();

        // Spinner shown while the EDS plugin is actively refreshing calendar backends.
        let sync_spinner = Rc::new(SpinnerWidget::new(false));
        let spinner_widget = sync_spinner.widget();
        spinner_widget.set_visible(false);

        let controls = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .build();
        controls.append(&spinner_widget); // spinner left of toggle buttons
        controls.append(&calendar_toggle);
        controls.append(&past_btn);

        header.append(&title_label);
        header.append(&controls);

        // Calendar in a revealer (hidden by default)
        let calendar_revealer = gtk::Revealer::builder()
            .transition_type(gtk::RevealerTransitionType::SlideDown)
            .transition_duration(200)
            .reveal_child(false)
            .build();
        calendar_revealer.set_child(Some(calendar.widget()));

        // Calendar toggle logic
        let revealer_ref = calendar_revealer.clone();
        let selection_store_ref = selection_store.clone();
        calendar_toggle.connect_toggled(move |btn| {
            let show = btn.is_active();
            revealer_ref.set_reveal_child(show);

            if show {
                btn.set_tooltip_text(Some(&crate::i18n::t("events-hide-calendar-tooltip")));
            } else {
                btn.set_tooltip_text(Some(&crate::i18n::t("events-show-calendar-tooltip")));
                selection_store_ref.emit(CalendarSelectionOp::ClearSelection);
            }
        });

        // Hide past events button when a non-today date is selected (the
        // past/future split only applies in the default today+tomorrow view;
        // selecting today is treated as no-selection).
        let past_btn_ref = past_btn.clone();
        let selection_store_ref = selection_store.clone();
        selection_store.subscribe(move || {
            let selected = selection_store_ref.get_state().selected_date;
            let today = chrono::Local::now().date_naive();
            let use_selected_mode = selected.is_some_and(|d| d != today);
            past_btn_ref.set_visible(!use_selected_mode);
        });

        // Trigger window resize after revealer animation completes
        calendar_revealer.connect_child_revealed_notify(move |_| {
            trigger_window_resize();
        });

        // Reflect calendar sync state in the spinner.
        {
            let update_spinner = {
                let sync_spinner_rc = Rc::clone(&sync_spinner);
                let store_ref = store.clone();
                move || {
                    let entities = store_ref.get_entities_typed::<entity::calendar::CalendarSync>(
                        entity::calendar::CALENDAR_SYNC_ENTITY_TYPE,
                    );
                    let syncing = entities.first().map(|(_, s)| s.syncing).unwrap_or(false);
                    log::debug!("[events] spinner syncing={syncing}");
                    sync_spinner_rc.set_spinning(syncing);
                    sync_spinner_rc.set_visible(syncing);
                }
            };
            store.subscribe_type(
                entity::calendar::CALENDAR_SYNC_ENTITY_TYPE,
                update_spinner.clone(),
            );
            // Initial reconciliation: entity may already be cached (CLAUDE.md EntityStore pattern).
            gtk::glib::idle_add_local_once(update_spinner);
        }

        container.append(&header);
        container.append(&calendar_revealer);
        container.append(agenda.widget());

        Self {
            container,
            _calendar: calendar,
            _agenda: agenda,
        }
    }

    pub fn widget(&self) -> &gtk::Widget {
        self.container.upcast_ref()
    }
}
