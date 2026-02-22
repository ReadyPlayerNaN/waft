use std::any::Any;

use gtk::prelude::*;

use super::component::AnyWidget;
use super::vnode::VNode;

struct ReconcilerEntry {
    component:  Box<dyn AnyWidget>,
    last_props: Box<dyn Any>,
    type_id:    std::any::TypeId,
}

/// Maintains a keyed list of live component instances inside a `gtk::Box`.
///
/// Call `reconcile()` with a new list of `VNode`s on every state change.
///
/// The reconciler performs three operations per call:
/// - **Key present, props unchanged** → widget kept as-is.
/// - **Key present, props changed** → widget updated in place.
/// - **Key absent from new list** → widget removed from container.
///
/// # Ordering
/// When all keys in the new list already exist in the current state, the GTK widget
/// order is not updated — widgets retain their prior position in the container.
/// Ordering is only guaranteed for newly appended entries.
pub struct Reconciler {
    // Vec preserves insertion order; linear scan is fine for UI lists.
    children:  Vec<(String, ReconcilerEntry)>,
    container: gtk::Box,
}

impl Reconciler {
    pub fn new(container: gtk::Box) -> Self {
        Self { children: Vec::new(), container }
    }

    pub fn reconcile(&mut self, nodes: impl IntoIterator<Item = VNode>) {
        let nodes: Vec<VNode> = nodes.into_iter().collect();

        // Assign keys: explicit key or positional fallback "$0", "$1", …
        let keyed: Vec<(String, VNode)> = nodes
            .into_iter()
            .enumerate()
            .map(|(i, node)| {
                let key = node.key.clone().unwrap_or_else(|| format!("${i}"));
                (key, node)
            })
            .collect();

        let new_keys: std::collections::HashSet<&str> =
            keyed.iter().map(|(k, _)| k.as_str()).collect();

        // 1. Remove entries absent from the new list.
        let to_remove: Vec<String> = self
            .children
            .iter()
            .filter(|(k, _)| !new_keys.contains(k.as_str()))
            .map(|(k, _)| k.clone())
            .collect();

        for key in &to_remove {
            let pos = self
                .children
                .iter()
                .position(|(k, _)| k == key)
                .expect("key in to_remove must exist in children");
            let (_, entry) = self.children.remove(pos);
            self.container.remove(&entry.component.widget());
        }

        // 2. Update existing entries and insert new ones.
        // TODO: reorder pre-existing widgets to match new order when required.
        for (key, vnode) in keyed {
            match self.children.iter().position(|(k, _)| k == &key) {
                Some(pos) => {
                    let entry = &mut self.children[pos].1;

                    if entry.type_id != vnode.type_id {
                        // Type changed: destroy old widget, build new one.
                        self.container.remove(&entry.component.widget());
                        let component = (vnode.build)();
                        self.container.append(&component.widget());
                        self.children[pos].1 = ReconcilerEntry {
                            last_props: vnode.props,
                            type_id:    vnode.type_id,
                            component,
                        };
                    } else if !(vnode.props_eq)(&entry.last_props) {
                        // Same type, props changed: update in place.
                        (vnode.update)(entry.component.as_ref());
                        entry.last_props = vnode.props;
                    }
                    // else: same type, same props — nothing to do.
                }

                None => {
                    // New key: build and append.
                    let component = (vnode.build)();
                    self.container.append(&component.widget());
                    self.children.push((key, ReconcilerEntry {
                        last_props: vnode.props,
                        type_id:    vnode.type_id,
                        component,
                    }));
                }
            }
        }
    }
}
