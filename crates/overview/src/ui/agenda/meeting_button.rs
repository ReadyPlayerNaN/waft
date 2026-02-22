//! Meeting link button widget — single button or multi-link popover.

use std::rc::Rc;

use gtk::prelude::*;
use log::warn;

use crate::menu_state::{MenuOp, MenuStore};

use super::meeting_links::{MeetingLink, MeetingProvider};

/// Map a MeetingProvider to a short display label.
fn provider_label(provider: &MeetingProvider) -> &'static str {
    match provider {
        MeetingProvider::GoogleMeet => "Meet",
        MeetingProvider::Zoom => "Zoom",
        MeetingProvider::Teams => "Teams",
    }
}

/// Meeting link action widget. Self-manages popover MenuStore tracking.
pub struct MeetingButton {
    pub root: gtk::Widget,
}

impl MeetingButton {
    /// Build a meeting action widget for the given links.
    ///
    /// - 0 links: returns `None`
    /// - 1 link: returns a direct button
    /// - 2+ links: returns a three-dot menu button with a popover
    pub fn new(links: &[MeetingLink], menu_store: &Rc<MenuStore>) -> Option<Self> {
        match links.len() {
            0 => None,
            1 => {
                let link = &links[0];
                let btn = gtk::Button::builder()
                    .label(provider_label(&link.provider))
                    .css_classes(["agenda-meeting-btn"])
                    .build();

                let url = link.url.clone();
                btn.connect_clicked(move |_| {
                    if let Err(e) =
                        gio::AppInfo::launch_default_for_uri(&url, gio::AppLaunchContext::NONE)
                    {
                        warn!("[agenda] failed to open meeting URL: {e}");
                    }
                });

                Some(Self { root: btn.upcast() })
            }
            _ => {
                let popover_box = gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .spacing(2)
                    .css_classes(["agenda-meeting-popover"])
                    .build();

                let popover = gtk::Popover::builder().child(&popover_box).build();

                // Generate unique ID for this popover
                let popover_id = uuid::Uuid::new_v4().to_string();

                // Track popover visibility in menu store
                let menu_store_show = menu_store.clone();
                let popover_id_show = popover_id.clone();
                popover.connect_show(move |_| {
                    menu_store_show.emit(MenuOp::PopoverOpened(popover_id_show.clone()));
                });

                let menu_store_close = menu_store.clone();
                let popover_id_close = popover_id;
                popover.connect_closed(move |_| {
                    menu_store_close.emit(MenuOp::PopoverClosed(popover_id_close.clone()));
                });

                for link in links {
                    let btn = gtk::Button::builder()
                        .label(provider_label(&link.provider))
                        .css_classes(["agenda-meeting-btn"])
                        .build();

                    let url = link.url.clone();
                    let popover_ref = popover.clone();
                    btn.connect_clicked(move |_| {
                        if let Err(e) =
                            gio::AppInfo::launch_default_for_uri(&url, gio::AppLaunchContext::NONE)
                        {
                            warn!("[agenda] failed to open meeting URL: {e}");
                        }
                        popover_ref.popdown();
                    });

                    popover_box.append(&btn);
                }

                let menu_btn = gtk::MenuButton::builder()
                    .icon_name("view-more-symbolic")
                    .popover(&popover)
                    .css_classes(["agenda-more-btn"])
                    .build();

                Some(Self {
                    root: menu_btn.upcast(),
                })
            }
        }
    }

    pub fn widget(&self) -> &gtk::Widget {
        &self.root
    }
}
