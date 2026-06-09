mod component;
pub mod container;
pub mod primitives;
mod reconciler;
mod render_component;
mod vnode;

pub use component::{Component, RenderCallback, RenderFn};
pub use container::{
    ActionRowPrefixContainer, ActionRowSuffixContainer, ButtonChildContainer,
    ToggleButtonChildContainer, VdomContainer,
};
pub use primitives::{
    VActionRow, VBox, VButton, VCustomButton, VEntryRow, VIcon, VLabel, VPreferencesGroup,
    VProgressBar, VRevealer, VScale, VSpinner, VSwitch, VSwitchRow, VToggleButton,
};
pub use reconciler::{Reconciler, SingleChildReconciler};
pub use render_component::RenderComponent;
pub use vnode::VNode;

#[cfg(test)]
mod tests;
