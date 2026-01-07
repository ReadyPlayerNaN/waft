//! Relm4 plugin registry (migration step 04).
//!
//! Responsibilities:
//! - Register plugins statically at startup (no unload/reload).
//! - Run `init()` for all plugins before GTK/adw init (must be GTK-free).
//! - Run `mount()` for all plugins after GTK/adw init (may construct UI/components).
//! - Store mounted endpoints for routing plugin-specific typed messages.
//! - Provide deterministic placement ordering (slot then weight; heavier goes lower).
//!
//! Notes:
//! - This module is intentionally GTK-free and unit-test friendly.
//! - We store cloneable mounted *metadata* separately from non-cloneable routing endpoints.
//! - Mounting actual Relm4 components is deferred to later steps; for now plugins may
//!   mount stubs, but the framework + routing must exist and be testable.

use std::any::{Any, TypeId};
use std::collections::BTreeMap;

use crate::relm4_app::events::PluginId;
use crate::relm4_app::plugin_framework::{
    MountedPlugin, MountedPluginMeta, PluginEndpoint, PluginHandle, PluginInitContext,
    PluginInitError, PluginMountContext, PluginMountError, PluginPlacement, PluginRouteError,
    PluginSpec, RelmPlugin, Slot, group_by_slot_sorted, sort_mounted_plugins,
};

/// Registry lifecycle state (helps catch incorrect call ordering).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegistryState {
    New,
    Inited,
    Mounted,
}

/// Relm4 plugin registry holding plugin instances and their mounted endpoints.
///
/// Plugins are stored as `Box<dyn RelmPlugin>` to preserve GTK-friendliness:
/// - no `Send + Sync` bounds
/// - initialization/mounting is expected to happen on the main thread.
pub struct RelmPluginRegistry {
    state: RegistryState,
    plugins: Vec<Box<dyn RelmPlugin>>,

    // Cloneable, UI-composition friendly metadata:
    mounted: Vec<MountedPluginMeta>,
    mounted_by_slot: BTreeMap<Slot, Vec<MountedPluginMeta>>,

    // Non-cloneable routing endpoints:
    endpoints: BTreeMap<PluginId, Box<dyn PluginEndpoint>>,

    // Captured at init-time for debugging/layout queries before mount:
    placements: BTreeMap<PluginId, PluginPlacement>,
}

impl std::fmt::Debug for RelmPluginRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RelmPluginRegistry")
            .field("state", &self.state)
            .field("plugins_len", &self.plugins.len())
            .field("mounted", &self.mounted)
            .field("mounted_by_slot", &self.mounted_by_slot)
            .field("endpoints_len", &self.endpoints.len())
            .field("placements", &self.placements)
            .finish()
    }
}

impl Default for RelmPluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl RelmPluginRegistry {
    pub fn new() -> Self {
        Self {
            state: RegistryState::New,
            plugins: Vec::new(),
            mounted: Vec::new(),
            mounted_by_slot: BTreeMap::new(),
            endpoints: BTreeMap::new(),
            placements: BTreeMap::new(),
        }
    }

    /// Register a plugin. Intended to be called during app startup, before `init_all()`.
    pub fn register(&mut self, plugin: Box<dyn RelmPlugin>) {
        self.plugins.push(plugin);
    }

    /// Initialize all plugins (GTK-free phase). Must be called before GTK/adw init.
    pub fn init_all(&mut self, ctx: PluginInitContext) -> Result<(), PluginInitError> {
        match self.state {
            RegistryState::New => {}
            RegistryState::Inited | RegistryState::Mounted => {
                // Idempotency is not guaranteed; keep it strict to avoid subtle bugs.
                return Err(PluginInitError::Failed(
                    "init_all() called more than once".to_string(),
                ));
            }
        }

        // Keep metadata captured even if init later fails (useful for debugging).
        for p in self.plugins.iter_mut() {
            let id = p.id();
            let placement = p.placement();
            self.placements.insert(id, placement);
        }

        for p in self.plugins.iter_mut() {
            p.init(ctx.clone())?;
        }

        self.state = RegistryState::Inited;
        Ok(())
    }

    /// Mount all plugins (GTK-safe phase). Must be called after GTK/adw init.
    pub fn mount_all(&mut self, ctx: PluginMountContext) -> Result<(), PluginMountError> {
        match self.state {
            RegistryState::Inited => {}
            RegistryState::New => {
                return Err(PluginMountError::Failed(
                    "mount_all() called before init_all()".to_string(),
                ));
            }
            RegistryState::Mounted => {
                return Err(PluginMountError::Failed(
                    "mount_all() called more than once".to_string(),
                ));
            }
        }

        let mut mounted: Vec<MountedPluginMeta> = Vec::with_capacity(self.plugins.len());
        let mut endpoints: BTreeMap<PluginId, Box<dyn PluginEndpoint>> = BTreeMap::new();

        for p in self.plugins.iter_mut() {
            let plugin_id = p.id();
            let name = p.name();
            let placement = p.placement();

            let mp = p.mount(ctx.clone())?;

            // Store routing endpoint.
            endpoints.insert(plugin_id.clone(), mp.endpoint);

            // Store cloneable metadata for sorting/grouping/layout.
            mounted.push(MountedPluginMeta {
                id: plugin_id,
                name,
                placement,
            });
        }

        // Store deterministic ordering for UI composition.
        self.mounted = sort_mounted_plugins(mounted);
        self.mounted_by_slot = group_by_slot_sorted(self.mounted.clone());
        self.endpoints = endpoints;
        self.state = RegistryState::Mounted;

        Ok(())
    }

    /// Acquire a typed handle to a plugin endpoint (Option 1.5A).
    ///
    /// This is runtime-fallible because plugin enablement is config-driven.
    /// Once acquired, sending `P::Input` is compile-time checked.
    pub fn get<P: PluginSpec>(&self) -> Option<PluginHandle<'_, P>> {
        let id = P::id();
        let ep = self.endpoints.get(&id)?;
        let ep: &dyn PluginEndpoint = ep.as_ref();

        // Optional sanity check: ensure the endpoint's declared input type matches `P::Input`.
        if ep.input_type_id() != TypeId::of::<P::Input>() {
            return None;
        }

        Some(PluginHandle::new(id, ep))
    }

    /// Return mounted plugin metadata sorted deterministically (slot then weight then id).
    ///
    /// This is intended for the overlay layout to mount plugin components into columns.
    pub fn mounted_sorted(&self) -> &[MountedPluginMeta] {
        &self.mounted
    }

    /// Return mounted plugin metadata grouped by slot, each slot list sorted by weight then id.
    pub fn mounted_by_slot(&self) -> &BTreeMap<Slot, Vec<MountedPluginMeta>> {
        &self.mounted_by_slot
    }

    /// Get a plugin's placement metadata (available after registration; populated at init).
    pub fn placement(&self, plugin: &PluginId) -> Option<PluginPlacement> {
        self.placements.get(plugin).copied()
    }

    /// Route a typed message to a plugin endpoint (legacy helper; prefer `get::<P>()` + `handle.send(...)`).
    pub fn route_any(&self, plugin: &PluginId, msg: &dyn Any) -> Result<(), PluginRouteError> {
        let ep = self
            .endpoints
            .get(plugin)
            .ok_or_else(|| PluginRouteError::MissingPlugin {
                plugin: plugin.clone(),
            })?;
        ep.send_any(msg)
    }

    /// Convenience typed routing helper (legacy; prefer `get::<P>()` + `handle.send(...)`).
    pub fn route_typed<T: 'static>(
        &self,
        plugin: &PluginId,
        msg: &T,
    ) -> Result<(), PluginRouteError> {
        self.route_any(plugin, msg)
    }

    /// Inspect the expected input `TypeId` for a mounted plugin (useful for diagnostics/tests).
    pub fn expected_input_type(&self, plugin: &PluginId) -> Option<TypeId> {
        self.endpoints.get(plugin).map(|e| e.input_type_id())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relm4_app::plugin_framework::{PluginPlacement, PluginRouteError};

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum P1Input {
        Ping,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum P2Input {
        Set(u32),
    }

    struct CaptureEndpoint<T: 'static> {
        plugin: PluginId,
        _phantom: std::marker::PhantomData<T>,
    }

    impl<T: 'static> PluginEndpoint for CaptureEndpoint<T> {
        fn plugin_id(&self) -> PluginId {
            self.plugin.clone()
        }

        fn input_type_id(&self) -> TypeId {
            TypeId::of::<T>()
        }

        fn send_any(&self, msg: &dyn Any) -> Result<(), PluginRouteError> {
            if msg.is::<T>() {
                Ok(())
            } else {
                Err(PluginRouteError::WrongMsgType {
                    plugin: self.plugin.clone(),
                    expected: std::any::type_name::<T>(),
                    got: "unknown",
                })
            }
        }
    }

    struct Plugin1;

    impl RelmPlugin for Plugin1 {
        fn id(&self) -> PluginId {
            "plugin.p1".into()
        }
        fn name(&self) -> &'static str {
            "p1"
        }
        fn placement(&self) -> PluginPlacement {
            PluginPlacement::new(Slot::Left, 10)
        }
        fn init(&mut self, _ctx: PluginInitContext) -> Result<(), PluginInitError> {
            Ok(())
        }
        fn mount(&mut self, _ctx: PluginMountContext) -> Result<MountedPlugin, PluginMountError> {
            let id = self.id();
            Ok(MountedPlugin {
                meta: MountedPluginMeta {
                    id: id.clone(),
                    name: self.name(),
                    placement: self.placement(),
                },
                endpoint: Box::new(CaptureEndpoint::<P1Input> {
                    plugin: id,
                    _phantom: std::marker::PhantomData,
                }),
            })
        }
    }

    struct Plugin2;

    impl RelmPlugin for Plugin2 {
        fn id(&self) -> PluginId {
            "plugin.p2".into()
        }
        fn name(&self) -> &'static str {
            "p2"
        }
        fn placement(&self) -> PluginPlacement {
            PluginPlacement::new(Slot::Left, 5)
        }
        fn init(&mut self, _ctx: PluginInitContext) -> Result<(), PluginInitError> {
            Ok(())
        }
        fn mount(&mut self, _ctx: PluginMountContext) -> Result<MountedPlugin, PluginMountError> {
            let id = self.id();
            Ok(MountedPlugin {
                meta: MountedPluginMeta {
                    id: id.clone(),
                    name: self.name(),
                    placement: self.placement(),
                },
                endpoint: Box::new(CaptureEndpoint::<P2Input> {
                    plugin: id,
                    _phantom: std::marker::PhantomData,
                }),
            })
        }
    }

    struct P1Spec;
    impl PluginSpec for P1Spec {
        type Input = P1Input;
        fn id() -> PluginId {
            "plugin.p1".into()
        }
        fn name() -> &'static str {
            "p1"
        }
        fn placement() -> PluginPlacement {
            PluginPlacement::new(Slot::Left, 10)
        }
    }

    struct P2Spec;
    impl PluginSpec for P2Spec {
        type Input = P2Input;
        fn id() -> PluginId {
            "plugin.p2".into()
        }
        fn name() -> &'static str {
            "p2"
        }
        fn placement() -> PluginPlacement {
            PluginPlacement::new(Slot::Left, 5)
        }
    }

    #[test]
    fn init_can_run_without_gtk() {
        let mut reg = RelmPluginRegistry::new();
        reg.register(Box::new(Plugin1));
        reg.register(Box::new(Plugin2));

        reg.init_all(PluginInitContext::default()).unwrap();
    }

    #[test]
    fn mounted_sorted_orders_by_weight_within_slot() {
        let mut reg = RelmPluginRegistry::new();
        reg.register(Box::new(Plugin1));
        reg.register(Box::new(Plugin2));

        reg.init_all(PluginInitContext::default()).unwrap();
        reg.mount_all(PluginMountContext::default()).unwrap();

        let ids: Vec<String> = reg
            .mounted_sorted()
            .iter()
            .map(|m| m.id.to_string())
            .collect();
        // weight 5 first, weight 10 later (heavier goes lower)
        assert_eq!(ids, vec!["plugin.p2", "plugin.p1"]);
    }

    #[test]
    fn typed_handle_acquisition_is_runtime_fallible_but_send_is_compile_time_typed() {
        let mut reg = RelmPluginRegistry::new();
        reg.register(Box::new(Plugin1));
        reg.register(Box::new(Plugin2));

        reg.init_all(PluginInitContext::default()).unwrap();
        reg.mount_all(PluginMountContext::default()).unwrap();

        let p1 = reg.get::<P1Spec>().expect("p1 should be mounted");
        p1.send(&P1Input::Ping).unwrap();

        // Missing plugin => None
        struct MissingSpec;
        impl PluginSpec for MissingSpec {
            type Input = P1Input;
            fn id() -> PluginId {
                "plugin.missing".into()
            }
            fn name() -> &'static str {
                "missing"
            }
            fn placement() -> PluginPlacement {
                PluginPlacement::new(Slot::Left, 0)
            }
        }
        assert!(reg.get::<MissingSpec>().is_none());
    }

    #[test]
    fn routing_finds_correct_endpoint_and_errors_on_missing() {
        let mut reg = RelmPluginRegistry::new();
        reg.register(Box::new(Plugin1));
        reg.register(Box::new(Plugin2));

        reg.init_all(PluginInitContext::default()).unwrap();
        reg.mount_all(PluginMountContext::default()).unwrap();

        // Prefer typed handles:
        reg.get::<P1Spec>().unwrap().send(&P1Input::Ping).unwrap();

        // Legacy routing path still works:
        let err = reg
            .route_typed(&"plugin.missing".into(), &P1Input::Ping)
            .unwrap_err();
        assert_eq!(
            err,
            PluginRouteError::MissingPlugin {
                plugin: "plugin.missing".into()
            }
        );
    }

    #[test]
    fn routing_errors_on_wrong_message_type() {
        let mut reg = RelmPluginRegistry::new();
        reg.register(Box::new(Plugin1));
        reg.register(Box::new(Plugin2));

        reg.init_all(PluginInitContext::default()).unwrap();
        reg.mount_all(PluginMountContext::default()).unwrap();

        let err = reg
            .route_typed(&"plugin.p1".into(), &P2Input::Set(1))
            .unwrap_err();
        assert!(matches!(err, PluginRouteError::WrongMsgType { .. }));

        // Typed handles prevent this at compile time (cannot send `P2Input` via `P1Spec` handle).
        let _p1 = reg.get::<P1Spec>().unwrap();
    }
}
