//! Provider picker dialog for adding an online account.
//!
//! Shows a list of available providers from `online-account-provider` entities.
//! On selection, sends `add-account` action to the selected provider entity.

use adw::prelude::*;
use waft_client::EntityActionCallback;
use waft_protocol::Urn;
use waft_protocol::entity::accounts::OnlineAccountProvider;
use waft_ui_gtk::icons::IconWidget;

use crate::i18n::t;

/// Show a dialog listing available online account providers.
///
/// When the user selects a provider, fires `add-account` action on its entity URN.
/// The plugin handles the rest (spawns GOA helper binary).
pub fn show_add_account_dialog(
    parent: &impl IsA<gtk::Widget>,
    providers: &[(Urn, OnlineAccountProvider)],
    action_callback: &EntityActionCallback,
) {
    let dialog = adw::AlertDialog::builder()
        .heading(t("online-accounts-add-account-title"))
        .close_response("cancel")
        .build();

    dialog.add_response("cancel", &t("online-accounts-add-account-cancel"));

    let list_box = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();

    // Sort providers alphabetically by display name
    let mut sorted: Vec<&(Urn, OnlineAccountProvider)> = providers.iter().collect();
    sorted.sort_by(|(_, a), (_, b)| a.provider_name.cmp(&b.provider_name));

    for (urn, provider) in &sorted {
        let row = adw::ActionRow::builder()
            .title(&provider.provider_name)
            .activatable(true)
            .build();

        // Add provider icon
        let icon_name = provider
            .icon_name
            .as_deref()
            .unwrap_or("contact-new-symbolic");
        let icon = IconWidget::from_name(icon_name, 32);
        row.add_prefix(icon.widget());

        // Add chevron suffix
        let chevron = IconWidget::from_name("go-next-symbolic", 16);
        row.add_suffix(chevron.widget());

        // On row activation: fire add-account action and close dialog
        let cb = action_callback.clone();
        let provider_urn = (*urn).clone();
        let dialog_ref = dialog.clone();
        row.connect_activated(move |_| {
            cb(
                provider_urn.clone(),
                "add-account".to_string(),
                serde_json::Value::Null,
            );
            dialog_ref.force_close();
        });

        list_box.append(&row);
    }

    dialog.set_extra_child(Some(&list_box));
    dialog.present(Some(parent));
}
