//! Plugin types and traits.
//!
//! This module re-exports types from `waft-plugin-api` for use within the overview crate.
//! The canonical definitions live in `waft-plugin-api`; this module provides
//! backward-compatible access so existing feature plugins don't need import changes.

pub use waft_plugin_api::{
    ExpandCallback, OverviewPlugin as Plugin, PluginId, PluginResources, Slot, Widget,
    WidgetFeatureToggle, WidgetRegistrar,
};
