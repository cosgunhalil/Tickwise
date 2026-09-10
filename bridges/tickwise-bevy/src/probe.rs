//! The probe: a Tickwise view over a Bevy `World`, built from the
//! component and resource types you declare as gameplay state.

use crate::walk::{Hasher, Sink, push_child, walk};
use bevy_ecs::component::Component;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use bevy_reflect::{Reflect, TypePath};
use tickwise::{DeterminismProbe, StateDump, Value};

/// One registered type and the walk that reads it out of a world.
struct Entry {
    /// The path prefix, the short type path of the registered type.
    label: &'static str,
    /// Whether the type also takes part in the light hash.
    in_light_hash: bool,
    /// Reads the type out of the world and emits its leaves. This is a
    /// plain function pointer, monomorphized at registration, so the
    /// probe needs no type registry and no downcasting.
    read: fn(&World, &mut String, &mut dyn Sink),
}

/// The set of component and resource types Tickwise treats as gameplay
/// state.
///
/// Nothing is included by default. A Bevy world holds timers, window
/// handles, and asset ids that have no business in a determinism hash, so
/// coverage is declared rather than guessed. This is the same rule the
/// [hash coverage checklist] describes: you decide what is gameplay state.
///
/// Register through the [`TickwiseAppExt`](crate::TickwiseAppExt) methods
/// on `App` rather than building this by hand.
///
/// [hash coverage checklist]: https://github.com/cosgunhalil/Tickwise/blob/main/docs/hash-coverage.md
#[derive(Resource, Default)]
pub struct TickwiseScope {
    entries: Vec<Entry>,
}

impl TickwiseScope {
    /// An empty scope covering nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of registered types.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when nothing is registered, in which case every hash is the
    /// hash of an empty walk and no desync can ever be found.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The short type paths of every registered type, in walk order.
    pub fn labels(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.entries.iter().map(|entry| entry.label)
    }

    /// Registers a component type for the full hash and the dump.
    ///
    /// Every entity carrying the component is walked, ordered by entity
    /// index, under paths like `Position[3].x`. The count of carrying
    /// entities is emitted as `Position.len`, so a spawn or a despawn on
    /// one machine shows up as a structural difference.
    pub fn component<C: Component + Reflect + TypePath>(&mut self) -> &mut Self {
        self.push::<C>(false, read_component::<C>)
    }

    /// Registers a component type for the light hash as well as the full
    /// hash. The light hash runs every tick, so keep this list short.
    pub fn light_component<C: Component + Reflect + TypePath>(&mut self) -> &mut Self {
        self.push::<C>(true, read_component::<C>)
    }

    /// Registers a resource type for the full hash and the dump, under
    /// paths like `Score.value`.
    pub fn resource<R: Resource + Reflect + TypePath>(&mut self) -> &mut Self {
        self.push::<R>(false, read_resource::<R>)
    }

    /// Registers a resource type for the light hash as well as the full
    /// hash. A random generator's state and a score belong here; a list
    /// of every entity does not.
    pub fn light_resource<R: Resource + Reflect + TypePath>(&mut self) -> &mut Self {
        self.push::<R>(true, read_resource::<R>)
    }

    fn push<T: TypePath>(
        &mut self,
        in_light_hash: bool,
        read: fn(&World, &mut String, &mut dyn Sink),
    ) -> &mut Self {
        let label = T::short_type_path();
        if let Some(existing) = self.entries.iter_mut().find(|entry| entry.label == label) {
            // Registering the same type twice, once plain and once for the
            // light hash, promotes it rather than walking it twice.
            existing.in_light_hash |= in_light_hash;
            return self;
        }
        // Kept sorted by label so the hash does not depend on the order
        // the plugins happened to register their types in.
        let at = self.entries.partition_point(|entry| entry.label < label);
        self.entries.insert(
            at,
            Entry {
                label,
                in_light_hash,
                read,
            },
        );
        self
    }

    fn walk_into(&self, world: &World, sink: &mut dyn Sink, light_only: bool) {
        let mut path = String::with_capacity(128);
        for entry in &self.entries {
            if light_only && !entry.in_light_hash {
                continue;
            }
            path.clear();
            path.push_str(entry.label);
            (entry.read)(world, &mut path, sink);
        }
    }
}

fn read_resource<R: Resource + Reflect + TypePath>(
    world: &World,
    path: &mut String,
    sink: &mut dyn Sink,
) {
    match world.get_resource::<R>() {
        Some(resource) => walk(resource.as_partial_reflect(), path, sink),
        // Present on one machine and absent on the other is exactly the
        // kind of difference worth catching, so absence is recorded.
        None => sink.leaf(path, Value::Null),
    }
}

fn read_component<C: Component + Reflect + TypePath>(
    world: &World,
    path: &mut String,
    sink: &mut dyn Sink,
) {
    // Entity iteration follows archetype order, which is an allocation
    // detail rather than part of the simulation, so the walk sorts by
    // entity index before emitting anything.
    let mut carriers: Vec<(u32, &C)> = world
        .iter_entities()
        .filter_map(|entity| {
            entity
                .get::<C>()
                .map(|component| (entity.id().index_u32(), component))
        })
        .collect();
    carriers.sort_by_key(|(index, _)| *index);

    let base = path.len();
    push_child(path, "len");
    sink.leaf(path, Value::Len(carriers.len() as u64));
    path.truncate(base);

    for (index, component) in carriers {
        let entity_base = path.len();
        path.push('[');
        path.push_str(&index.to_string());
        path.push(']');
        walk(component.as_partial_reflect(), path, sink);
        path.truncate(entity_base);
    }
}

/// A Tickwise probe over a Bevy world, covering the types in a
/// [`TickwiseScope`].
///
/// Borrow both and hand the probe to a recorder. The plugin does this for
/// you every fixed tick; construct one by hand only when driving Tickwise
/// yourself.
///
/// # Examples
///
/// ```
/// use bevy_ecs::prelude::*;
/// use bevy_reflect::Reflect;
/// use tickwise::DeterminismProbe;
/// use tickwise_bevy::{BevyProbe, TickwiseScope};
///
/// #[derive(Resource, Reflect, Default)]
/// struct Score(u32);
///
/// let mut world = World::new();
/// world.insert_resource(Score(7));
///
/// let mut scope = TickwiseScope::new();
/// scope.light_resource::<Score>();
///
/// let probe = BevyProbe::new(&world, &scope);
/// let before = probe.light_hash();
///
/// world.resource_mut::<Score>().0 = 8;
/// let probe = BevyProbe::new(&world, &scope);
/// assert_ne!(before, probe.light_hash());
/// ```
pub struct BevyProbe<'w> {
    world: &'w World,
    scope: &'w TickwiseScope,
}

impl<'w> BevyProbe<'w> {
    /// Builds a probe over a world and a scope.
    pub fn new(world: &'w World, scope: &'w TickwiseScope) -> Self {
        Self { world, scope }
    }
}

impl DeterminismProbe for BevyProbe<'_> {
    fn light_hash(&self) -> u64 {
        let mut hasher = Hasher::new();
        self.scope.walk_into(self.world, &mut hasher, true);
        hasher.finish()
    }

    fn full_hash(&self) -> u64 {
        let mut hasher = Hasher::new();
        self.scope.walk_into(self.world, &mut hasher, false);
        hasher.finish()
    }

    fn state_dump(&self) -> StateDump {
        let mut dump = StateDump::empty();
        self.scope.walk_into(self.world, &mut dump, false);
        dump
    }
}
