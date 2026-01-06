use crate::ui::FeatureSpec;

pub enum Slot {
    Left,
    Right,
    Top,
}

pub struct Widget {
    pub el: gtk::Box,
    pub weight: i32,
    pub column: Slot,
}

/// A lightweight handle describing how a plugin exposes a single feature.
///
/// This struct owns a `FeatureSpec`, which keeps the API simple and
/// lifetime-free at the cost of a cheap clone when constructing it.
pub struct FeatureToggle {
    /// UI description of this feature.
    pub el: FeatureSpec,

    /// Ordering weight for sorting feature toggles in the UI.
    pub weight: i32,
}
