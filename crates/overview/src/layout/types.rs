//! Layout widget types used by the feature toggle grid system.

use waft_ui_gtk::widgets::feature_toggle::FeatureToggleWidget;

/// A feature toggle widget with metadata for grid placement.
///
/// Used by dynamic toggle components to provide toggles to the FeatureToggleGrid.
pub struct WidgetFeatureToggle {
    pub id: String,
    pub weight: i32,
    pub toggle: FeatureToggleWidget,
    /// Optional menu widget (for expandable toggles).
    pub menu: Option<gtk::Widget>,
}
