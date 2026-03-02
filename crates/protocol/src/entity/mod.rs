//! Domain entity types for the waft protocol.
//!
//! Entity modules are organized by **domain**, not by plugin. A module like
//! `display` groups all display-related entities (brightness, dark mode, night
//! light) regardless of which plugin provides them.
//!
//! # Anti-pattern: plugin names in the protocol
//!
//! Entity modules and types must **never** reference a specific plugin
//! implementation. Names like `darkman`, `sunsetr`, `caffeine`, or
//! `brightnessctl` are plugin identifiers — they describe *who provides*
//! the data, not *what the data is*. The protocol describes domain concepts
//! that any conforming plugin can implement.
//!
//! ```text
//! BAD:  entity::darkman::DarkmanMode     — ties the protocol to one plugin
//! GOOD: entity::display::DarkMode        — describes the domain concept
//!
//! BAD:  entity::sunsetr::SunsetrState    — plugin name leaked into protocol
//! GOOD: entity::display::NightLight      — domain-level entity
//!
//! BAD:  entity::caffeine::CaffeineState  — implementation detail
//! GOOD: entity::session::SleepInhibitor  — describes what it is
//! ```
//!
//! When adding new entity types, ask "what domain does this belong to?"
//! not "which plugin produces it?".

pub mod accounts;
pub mod app;
pub mod appearance;
pub mod audio;
pub mod bluetooth;
pub mod calendar;
pub mod clock;
pub mod display;
pub mod keyboard;
pub mod network;
pub mod notification;
pub mod notification_filter;
pub mod notification_sound;
pub mod plugin;
pub mod power;
pub mod registry;
pub mod session;
pub mod storage;
pub mod weather;
pub mod window;
