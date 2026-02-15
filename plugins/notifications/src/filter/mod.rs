//! Notification filtering system.

pub mod compiler;
pub mod matcher;
pub mod profile_state;
pub mod toml_sync;

pub use compiler::{compile_groups, CompiledGroup};
pub use matcher::{matches_combinator, matches_pattern};
pub use profile_state::{load_active_profile, save_active_profile};
