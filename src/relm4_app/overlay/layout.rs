//! Pure overlay layout helper (migration step 05).
//!
//! This module is GTK/Relm4-free and exists purely to map mounted plugin metadata
//! (slot + weight + id) into the overlay host's layout buckets:
//! - Top (horizontal container)
//! - Left (vertical column)
//! - Right (vertical column)
//!
//! Ordering semantics must match the legacy overlay composition:
//! - within each slot, lighter weight goes higher, heavier goes lower
//! - ties are broken deterministically by plugin id (lexicographic)

use crate::relm4_app::events::PluginId;
use crate::relm4_app::plugin_framework::{MountedPluginMeta, Slot};

/// Grouped view of overlay placements for the three layout buckets.
///
/// This is intentionally small and easy to assert against in unit tests.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OverlayBuckets {
    pub top: Vec<PluginId>,
    pub left: Vec<PluginId>,
    pub right: Vec<PluginId>,
}

impl OverlayBuckets {
    /// Convenience for tests and simple callers.
    pub fn ids(&self) -> (Vec<PluginId>, Vec<PluginId>, Vec<PluginId>) {
        (self.top.clone(), self.left.clone(), self.right.clone())
    }
}

/// Compute overlay layout buckets from a list of mounted plugin metadata.
///
/// Requirements:
/// - stable and deterministic ordering
/// - "heavier goes lower" => sort by `weight` ascending within each slot
/// - ignore unknown slots (should not happen because `Slot` is exhaustive)
///
/// NOTE: This function does not consult the registry directly; callers can pass
/// either `registry.mounted_sorted()` or `registry.mounted_by_slot()` contents.
/// The function will re-assert deterministic ordering regardless.
pub fn bucketize_mounted_plugins(mounted: &[MountedPluginMeta]) -> OverlayBuckets {
    // Defensive: sort again here so the mapping stays correct even if callers
    // pass an unsorted slice.
    let mut v: Vec<MountedPluginMeta> = mounted.to_vec();
    v.sort_by(|a, b| {
        a.placement
            .slot
            .cmp(&b.placement.slot)
            .then_with(|| a.placement.weight.cmp(&b.placement.weight))
            .then_with(|| a.id.cmp(&b.id))
    });

    let mut buckets = OverlayBuckets::default();
    for m in v {
        match m.placement.slot {
            Slot::Top => buckets.top.push(m.id),
            Slot::Left => buckets.left.push(m.id),
            Slot::Right => buckets.right.push(m.id),
        }
    }
    buckets
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relm4_app::plugin_framework::PluginPlacement;

    fn m(id: &'static str, slot: Slot, weight: i32) -> MountedPluginMeta {
        MountedPluginMeta {
            id: PluginId::from_static(id),
            name: id,
            placement: PluginPlacement::new(slot, weight),
        }
    }

    #[test]
    fn bucketize_preserves_weight_ordering_heavier_goes_lower() {
        // Intentionally shuffled input.
        let mounted = vec![
            m("p.left.heavy", Slot::Left, 50),
            m("p.top.light", Slot::Top, 1),
            m("p.left.light", Slot::Left, 5),
            m("p.right.mid", Slot::Right, 10),
            m("p.top.heavy", Slot::Top, 99),
        ];

        let b = bucketize_mounted_plugins(&mounted);

        assert_eq!(
            b.top.iter().map(|x| x.as_str()).collect::<Vec<_>>(),
            vec!["p.top.light", "p.top.heavy"]
        );

        assert_eq!(
            b.left.iter().map(|x| x.as_str()).collect::<Vec<_>>(),
            vec!["p.left.light", "p.left.heavy"]
        );

        assert_eq!(
            b.right.iter().map(|x| x.as_str()).collect::<Vec<_>>(),
            vec!["p.right.mid"]
        );
    }

    #[test]
    fn bucketize_breaks_ties_by_id_deterministically() {
        let mounted = vec![
            m("p.left.b", Slot::Left, 10),
            m("p.left.a", Slot::Left, 10),
            m("p.left.c", Slot::Left, 10),
        ];

        let b = bucketize_mounted_plugins(&mounted);
        assert_eq!(
            b.left.iter().map(|x| x.as_str()).collect::<Vec<_>>(),
            vec!["p.left.a", "p.left.b", "p.left.c"]
        );
    }

    #[test]
    fn bucketize_handles_empty_input() {
        let b = bucketize_mounted_plugins(&[]);
        assert_eq!(b, OverlayBuckets::default());
    }
}
