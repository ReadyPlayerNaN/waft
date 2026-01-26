//! Minimal library entry point intended for integration tests.
//!
//! This crate is primarily a binary, but integration tests (`tests/`) need a
//! library target to import code. To keep changes small and avoid pulling in the
//! full application/plugin surface (GTK init, DBus server/client wiring, etc.),
//! we only expose the notifications *store* and the minimal types it depends on.

pub mod store;

pub mod ui {
    #[path = "icon.rs"]
    pub mod icon;
}

pub mod features {
    pub mod notifications {
        // NOTE: `#[path = ...]` inside inline modules is resolved relative to the
        // module’s implicit directory (`src/features/notifications/`), not `src/`.
        #[path = "types.rs"]
        pub mod types;

        pub mod dbus {
            #[path = "hints.rs"]
            pub mod hints;

            #[path = "ingress.rs"]
            pub mod ingress;
        }

        #[path = "store/mod.rs"]
        pub mod store;
    }
}
