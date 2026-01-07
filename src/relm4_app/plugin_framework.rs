//! Relm4-first plugin framework scaffolding (migration step 04).
//!
//! Key goals:
//! - Split plugin lifecycle across the GTK init boundary:
//!   - `init()` MUST be GTK-free (safe to run before `gtk::init()` / `adw::init()`).
//!   - `mount()` runs after GTK/adw init and may construct GTK widgets / Relm4 components.
//! - Keep plugins GTK-friendly: no `Send + Sync` requirements.
//! - Provide placement metadata (slot + weight) without constructing widgets.
//! - Provide compile-time safe routing via *typed plugin handles* (Option 1.5A):
//!   - Plugins keep their own `Input` enums inside plugin modules.
//!   - The registry/router can acquire a `PluginHandle<P>` for a specific plugin spec `P`.
//!   - Sending `P::Input` is compile-time checked once you have a handle.
//!   - Plugin presence remains runtime (config-driven enablement), so `get::<P>()` returns `Option`.
//!
//! This module intentionally does not define concrete plugin UIs yet. Later steps will
//! introduce real Relm4 components per plugin and wire routing in the Relm4 app entrypoint.

use std::any::{Any, TypeId};
use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;

use crate::relm4_app::events::PluginId;

/// Column/slot placement in the overlay UI.
///
/// Semantics preserved from the legacy widget-based system:
/// - `Left`, `Right`, `Top` columns exist
/// - ordering is by `weight` within each slot (heavier goes lower)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slot {
    Left,
    Right,
    Top,
}

/// Placement metadata for a plugin surface mounted in the overlay UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginPlacement {
    pub slot: Slot,
    pub weight: i32,
}

impl PluginPlacement {
    pub const fn new(slot: Slot, weight: i32) -> Self {
        Self { slot, weight }
    }
}

/// Initialization context passed to `RelmPlugin::init()`.
///
/// This must remain GTK-safe and must not encourage GTK construction.
///
/// It currently contains no handles; it exists as an extension point so we can add
/// things like:
/// - an app-level sender for `AppMsg`
/// - a DBus handle factory
/// - a logging facade
///
/// in later steps without breaking the trait.
#[derive(Debug, Default, Clone)]
pub struct PluginInitContext {}

/// Mount context passed to `RelmPlugin::mount()`.
///
/// This is called after GTK/adw init. In later steps this will likely contain
/// Relm4 `AppHandle`/`Sender` references, main context handles, etc.
#[derive(Debug, Default, Clone)]
pub struct PluginMountContext {}

/// Plugin initialization error.
///
/// Keep it lightweight and non-allocating where possible; plugins can still wrap
/// richer errors in a `String` if needed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginInitError {
    Failed(String),
}

impl fmt::Display for PluginInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PluginInitError::Failed(msg) => write!(f, "plugin init failed: {msg}"),
        }
    }
}

impl std::error::Error for PluginInitError {}

/// Plugin mount error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginMountError {
    Failed(String),
}

impl fmt::Display for PluginMountError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PluginMountError::Failed(msg) => write!(f, "plugin mount failed: {msg}"),
        }
    }
}

impl std::error::Error for PluginMountError {}

/// A compile-time spec for a plugin.
///
/// This is the core of Option 1.5A:
/// - plugin-specific message types remain inside plugins (as `type Input`)
/// - call sites can be compile-time typed once they have a `PluginHandle<P>`
/// - plugin availability remains runtime (config-driven), so acquiring a handle is fallible
pub trait PluginSpec: 'static {
    type Input: 'static;

    /// Stable id for this plugin.
    fn id() -> PluginId;

    /// Human-readable plugin name (mainly for debugging/logging).
    fn name() -> &'static str;

    /// Placement metadata for this plugin.
    fn placement() -> PluginPlacement;
}

/// Relm4-oriented plugin contract.
///
/// Notes:
/// - This trait MUST remain GTK-friendly: it does not require `Send` or `Sync`.
/// - `init()` may start background/DBus work as long as it does not touch GTK.
/// - `mount()` is the GTK-safe phase where the plugin instantiates its Relm4 component(s).
pub trait RelmPlugin {
    /// Stable plugin id (used for routing).
    fn id(&self) -> PluginId;

    /// Human-readable plugin name (mainly for debugging).
    fn name(&self) -> &'static str;

    /// Placement metadata for the overlay UI.
    fn placement(&self) -> PluginPlacement;

    /// Initialize plugin domain state / background tasks.
    ///
    /// MUST NOT create GTK widgets here.
    fn init(&mut self, _ctx: PluginInitContext) -> Result<(), PluginInitError> {
        Ok(())
    }

    /// Mount plugin UI components. Called after GTK/adw is initialized.
    fn mount(&mut self, _ctx: PluginMountContext) -> Result<MountedPlugin, PluginMountError>;
}

/// A typed handle to a mounted plugin endpoint.
///
/// Once you have a `PluginHandle<P>`, sending `P::Input` is compile-time checked.
///
/// Acquiring the handle is runtime-fallible (plugin may be disabled by config or failed to mount).
pub struct PluginHandle<'a, P: PluginSpec> {
    plugin_id: PluginId,
    endpoint: &'a dyn PluginEndpoint,
    _phantom: PhantomData<P>,
}

impl<'a, P: PluginSpec> PluginHandle<'a, P> {
    /// Construct a typed handle from a plugin id + endpoint reference.
    ///
    /// This is crate-public so the registry can build handles, but call sites should
    /// prefer `RelmPluginRegistry::get::<P>()`.
    pub(crate) fn new(plugin_id: PluginId, endpoint: &'a dyn PluginEndpoint) -> Self {
        Self {
            plugin_id,
            endpoint,
            _phantom: PhantomData,
        }
    }
}

impl<'a, P: PluginSpec> Clone for PluginHandle<'a, P> {
    fn clone(&self) -> Self {
        Self {
            plugin_id: self.plugin_id.clone(),
            endpoint: self.endpoint,
            _phantom: PhantomData,
        }
    }
}

impl<'a, P: PluginSpec> PluginHandle<'a, P> {
    pub fn plugin_id(&self) -> &PluginId {
        &self.plugin_id
    }

    /// Send a typed input message to the plugin.
    ///
    /// This is compile-time typed at the call site (`P::Input`).
    pub fn send(&self, msg: &P::Input) -> Result<(), PluginRouteError> {
        self.endpoint.send_any(msg)
    }

    /// Inspect expected input type (useful for assertions/diagnostics).
    pub fn expected_input_type_id(&self) -> TypeId {
        self.endpoint.input_type_id()
    }
}

/// A dynamically-dispatched endpoint used by the app/router to send messages to a plugin.
///
/// We intentionally avoid a centralized enum of plugin endpoints. Instead, we use Option 1.5A:
/// - the registry can *acquire* a typed `PluginHandle<P>` using `PluginSpec`
/// - once you have the typed handle, sending `P::Input` is compile-time checked
///
/// Internally, this endpoint trait uses `Any` downcasting to validate the message type.
/// This runtime validation is unavoidable at the type-erased boundary, but it is now localized
/// primarily to handle acquisition and the endpoint boundary, not spread across call sites.
///
/// IMPORTANT: This endpoint must be used only from the GTK/main thread unless the plugin
/// explicitly documents thread-safety for its sender/controller.
pub trait PluginEndpoint {
    /// The plugin id this endpoint belongs to.
    fn plugin_id(&self) -> PluginId;

    /// The expected message type (for diagnostics / tests).
    fn input_type_id(&self) -> TypeId;

    /// Send a plugin-specific message via type-erased `Any`.
    ///
    /// Implementations should return `PluginRouteError::WrongMsgType` when `msg`
    /// is not of the expected type.
    fn send_any(&self, msg: &dyn Any) -> Result<(), PluginRouteError>;
}

/// Common routing/registry errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginRouteError {
    MissingPlugin {
        plugin: PluginId,
    },
    WrongMsgType {
        plugin: PluginId,
        expected: &'static str,
        got: &'static str,
    },
}

impl fmt::Display for PluginRouteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PluginRouteError::MissingPlugin { plugin } => {
                write!(f, "no mounted endpoint for plugin {plugin}")
            }
            PluginRouteError::WrongMsgType {
                plugin,
                expected,
                got,
            } => write!(
                f,
                "wrong message type for plugin {plugin}: expected {expected}, got {got}"
            ),
        }
    }
}

impl std::error::Error for PluginRouteError {}

/// Metadata about a mounted plugin surface (cloneable, UI-composition friendly).
///
/// This is intentionally separated from routing endpoints so the overlay/layout code can
/// sort/group metadata without needing to clone non-cloneable endpoint handles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountedPluginMeta {
    pub id: PluginId,
    pub name: &'static str,
    pub placement: PluginPlacement,
}

/// A mounted plugin surface: metadata + an endpoint for routing.
///
/// Note: `MountedPlugin` is *not* `Clone` because endpoints are generally not cloneable.
pub struct MountedPlugin {
    pub meta: MountedPluginMeta,
    pub endpoint: Box<dyn PluginEndpoint>,
}

impl fmt::Debug for MountedPlugin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MountedPlugin")
            .field("id", &self.meta.id)
            .field("name", &self.meta.name)
            .field("placement", &self.meta.placement)
            .finish_non_exhaustive()
    }
}

/// Small helper endpoint implementation for common "typed sender/controller" use-cases.
///
/// This lets plugins keep their input enum private and still expose a routing endpoint:
///
/// - Plugin defines `enum Input { ... }`
/// - Plugin builds some sender/controller `S` that can accept `Input`
/// - Plugin returns `MountedPlugin` with `TypedEndpoint<S, Input>`
///
/// `S` can be a Relm4 `ComponentSender<Input>` or any custom type.
pub struct TypedEndpoint<S, Input> {
    plugin: PluginId,
    sender: S,
    _phantom: std::marker::PhantomData<Input>,
}

impl<S, Input> TypedEndpoint<S, Input> {
    pub fn new(plugin: PluginId, sender: S) -> Self {
        Self {
            plugin,
            sender,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Borrow the underlying sender/controller.
    pub fn sender(&self) -> &S {
        &self.sender
    }
}

/// A minimal trait abstraction for "send typed input".
///
/// This avoids coupling this framework module to Relm4 sender/controller types.
/// Plugins can implement this for their sender wrappers if needed.
pub trait SendInput<Input> {
    fn send_input(&self, msg: Input);
}

impl<S, Input> PluginEndpoint for TypedEndpoint<S, Input>
where
    S: SendInput<Input> + 'static,
    Input: Clone + 'static,
{
    fn plugin_id(&self) -> PluginId {
        self.plugin.clone()
    }

    fn input_type_id(&self) -> TypeId {
        TypeId::of::<Input>()
    }

    fn send_any(&self, msg: &dyn Any) -> Result<(), PluginRouteError> {
        let Some(typed) = msg.downcast_ref::<Input>() else {
            return Err(PluginRouteError::WrongMsgType {
                plugin: self.plugin.clone(),
                expected: std::any::type_name::<Input>(),
                got: any_type_name(msg),
            });
        };

        // Endpoint API uses `&dyn Any`; clone to send an owned message.
        self.sender.send_input(typed.clone());
        Ok(())
    }
}

/// NOTE: `PluginEndpoint::send_any` takes `&dyn Any`, so `TypedEndpoint` needs to clone input.
///
/// We keep this as a simple `Input: Clone` bound in the `TypedEndpoint` impl above.
/// If you need non-clone semantics, implement `PluginEndpoint` directly for your endpoint type.

fn any_type_name(v: &dyn Any) -> &'static str {
    // We can't get a stable, real type name from `Any` without knowing the concrete type.
    // For diagnostics, report "unknown" unless the value is a common primitive.
    if v.is::<String>() {
        "String"
    } else if v.is::<bool>() {
        "bool"
    } else if v.is::<u32>() {
        "u32"
    } else if v.is::<i64>() {
        "i64"
    } else {
        "unknown"
    }
}

/// Pure sorting helper for placement ordering (unit-test friendly).
///
/// Rule preserved: sort by `Slot` (deterministic) and then by `weight` ascending
/// (lower first, heavier goes lower). Ties are broken by plugin id to ensure
/// deterministic ordering.
pub fn sort_mounted_plugins(mut plugins: Vec<MountedPluginMeta>) -> Vec<MountedPluginMeta> {
    plugins.sort_by(|a, b| {
        (a.placement.slot, a.placement.weight, a.id.as_str()).cmp(&(
            b.placement.slot,
            b.placement.weight,
            b.id.as_str(),
        ))
    });
    plugins
}

/// Group mounted plugins by slot in sorted order.
///
/// Output is deterministic:
/// - slots are in `Slot` order (Left, Right, Top)
/// - within each slot, plugins are sorted by weight then id
pub fn group_by_slot_sorted(
    plugins: Vec<MountedPluginMeta>,
) -> BTreeMap<Slot, Vec<MountedPluginMeta>> {
    let plugins = sort_mounted_plugins(plugins);
    let mut map: BTreeMap<Slot, Vec<MountedPluginMeta>> = BTreeMap::new();
    for p in plugins {
        map.entry(p.placement.slot).or_default().push(p);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum ExampleInput {
        Ping,
        SetEnabled(bool),
    }

    #[derive(Default)]
    struct CaptureSender {
        log: std::cell::RefCell<Vec<ExampleInput>>,
    }

    impl SendInput<ExampleInput> for CaptureSender {
        fn send_input(&self, msg: ExampleInput) {
            self.log.borrow_mut().push(msg);
        }
    }

    struct CaptureEndpoint {
        plugin: PluginId,
        sender: CaptureSender,
    }

    impl PluginEndpoint for CaptureEndpoint {
        fn plugin_id(&self) -> PluginId {
            self.plugin.clone()
        }

        fn input_type_id(&self) -> TypeId {
            TypeId::of::<ExampleInput>()
        }

        fn send_any(&self, msg: &dyn Any) -> Result<(), PluginRouteError> {
            let Some(m) = msg.downcast_ref::<ExampleInput>() else {
                return Err(PluginRouteError::WrongMsgType {
                    plugin: self.plugin.clone(),
                    expected: std::any::type_name::<ExampleInput>(),
                    got: any_type_name(msg),
                });
            };
            self.sender.send_input(m.clone());
            Ok(())
        }
    }

    fn mounted(id: &'static str, slot: Slot, weight: i32) -> MountedPluginMeta {
        MountedPluginMeta {
            id: id.into(),
            name: "example",
            placement: PluginPlacement { slot, weight },
        }
    }

    #[test]
    fn placement_sorting_is_slot_then_weight_then_id() {
        let p1 = mounted("b", Slot::Left, 10);
        let p2 = mounted("a", Slot::Left, 10);
        let p3 = mounted("c", Slot::Left, 5);
        let p4 = mounted("z", Slot::Right, 0);

        let out = sort_mounted_plugins(vec![p1, p2, p3, p4]);
        let ids: Vec<String> = out.into_iter().map(|p| p.id.to_string()).collect();

        assert_eq!(ids, vec!["c", "a", "b", "z"]);
    }

    #[test]
    fn endpoint_rejects_wrong_message_type() {
        let pid: PluginId = "plugin.example".into();
        let p = MountedPlugin {
            meta: MountedPluginMeta {
                id: pid.clone(),
                name: "example",
                placement: PluginPlacement {
                    slot: Slot::Left,
                    weight: 0,
                },
            },
            endpoint: Box::new(CaptureEndpoint {
                plugin: pid,
                sender: CaptureSender::default(),
            }),
        };
        let err = p
            .endpoint
            .send_any(&"not the right type".to_string())
            .unwrap_err();

        assert_eq!(
            err,
            PluginRouteError::WrongMsgType {
                plugin: "plugin.example".into(),
                expected: std::any::type_name::<ExampleInput>(),
                got: "String",
            }
        );
    }
}
