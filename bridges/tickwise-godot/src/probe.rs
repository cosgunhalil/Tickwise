//! The probe: a Tickwise view over the nodes you declared as gameplay
//! state.

use crate::walk::{Hasher, Sink, script_variable_names, walk};
use godot::builtin::StringName;
use godot::classes::{Node, SceneTree};
use godot::obj::Gd;
use tickwise::{DeterminismProbe, StateDump};

/// One node in scope, with the path its values are recorded under.
struct Member {
    /// The node's absolute path, for example `/root/Game/Ball1`.
    path: String,
    node: Gd<Node>,
    /// Whether the node also takes part in the light hash.
    in_light_hash: bool,
}

/// A Tickwise probe over the nodes of a scene tree.
///
/// Membership is declared with groups rather than guessed. A node in the
/// full group has its script variables covered by the full hash and the
/// dump; a node in the light group is covered by the light hash as well,
/// which runs every tick.
///
/// Only script variables count. A node's `position`, `visible`, and
/// `name` are engine properties and stay out, which is what keeps
/// rendering and editor state from reaching a determinism hash.
pub(crate) struct GodotProbe {
    members: Vec<Member>,
}

impl GodotProbe {
    /// Collects the nodes of both groups from the tree.
    ///
    /// Nodes are sorted by absolute path, because a group's membership
    /// order follows the order nodes entered the tree, which is an
    /// engine detail rather than part of the simulation.
    pub(crate) fn collect(
        tree: &Gd<SceneTree>,
        group: &StringName,
        light_group: &StringName,
    ) -> Self {
        let mut members: Vec<Member> = Vec::new();

        for node in tree.get_nodes_in_group(group).iter_shared() {
            members.push(Member {
                path: node.get_path().to_string(),
                node,
                in_light_hash: false,
            });
        }
        for node in tree.get_nodes_in_group(light_group).iter_shared() {
            let path = node.get_path().to_string();
            match members.iter_mut().find(|member| member.path == path) {
                // In both groups: promote rather than walk it twice.
                Some(existing) => existing.in_light_hash = true,
                None => members.push(Member {
                    path,
                    node,
                    in_light_hash: true,
                }),
            }
        }

        members.sort_by(|a, b| a.path.cmp(&b.path));
        Self { members }
    }

    /// Number of nodes in scope.
    pub(crate) fn len(&self) -> usize {
        self.members.len()
    }

    fn walk_into(&self, sink: &mut dyn Sink, light_only: bool) {
        let mut path = String::with_capacity(128);
        for member in &self.members {
            if light_only && !member.in_light_hash {
                continue;
            }
            let object = member.node.clone().upcast::<godot::classes::Object>();
            for name in script_variable_names(&object) {
                path.clear();
                path.push_str(&member.path);
                path.push('.');
                path.push_str(&name.to_string());
                walk(&object.get(&name), &mut path, sink);
            }
        }
    }

    /// Hashes only the nodes in the light group.
    pub(crate) fn light(&self) -> u64 {
        let mut hasher = Hasher::new();
        self.walk_into(&mut hasher, true);
        hasher.finish()
    }

    /// Hashes every node in scope.
    pub(crate) fn full(&self) -> u64 {
        let mut hasher = Hasher::new();
        self.walk_into(&mut hasher, false);
        hasher.finish()
    }

    /// The full state of every node in scope, as paths and values.
    pub(crate) fn dump(&self) -> StateDump {
        let mut dump = StateDump::empty();
        self.walk_into(&mut dump, false);
        dump
    }
}

impl DeterminismProbe for GodotProbe {
    fn light_hash(&self) -> u64 {
        self.light()
    }

    fn full_hash(&self) -> u64 {
        self.full()
    }

    fn state_dump(&self) -> StateDump {
        self.dump()
    }
}
