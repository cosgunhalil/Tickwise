//! The reflection walk: what ends up in a dump, and what must never
//! reach a hash.

use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use std::collections::HashMap;
use tickwise::{DeterminismProbe, Value};
use tickwise_bevy::{BevyProbe, TickwiseScope};

#[derive(Reflect, Clone, Debug, Default)]
struct Vec2 {
    x: f32,
    y: f32,
}

#[derive(Reflect, Clone, Debug, Default)]
enum Mode {
    #[default]
    Idle,
    Carrying(u32),
    Aiming {
        target: u32,
    },
}

#[derive(Component, Reflect, Clone, Debug, Default)]
struct Unit {
    position: Vec2,
    hitpoints: i32,
    alive: bool,
    mode: Mode,
    inventory: Vec<u32>,
    label: String,
}

#[derive(Resource, Reflect, Default)]
struct Inventory {
    counts: HashMap<String, i32>,
}

#[derive(Resource, Reflect, Default)]
struct Weather {
    wind: f64,
}

#[derive(Component, Reflect, Default)]
struct Unregistered {
    noise: u64,
}

fn dump_of(world: &World, scope: &TickwiseScope) -> Vec<(String, Value)> {
    BevyProbe::new(world, scope)
        .state_dump()
        .iter()
        .map(|(path, value)| (path.to_string(), value.clone()))
        .collect()
}

fn paths(world: &World, scope: &TickwiseScope) -> Vec<String> {
    dump_of(world, scope)
        .into_iter()
        .map(|(path, _)| path)
        .collect()
}

#[test]
fn a_component_walks_into_named_paths_under_its_entity_index() {
    let mut world = World::new();
    world.spawn(Unit {
        position: Vec2 { x: 1.5, y: -2.0 },
        hitpoints: 42,
        alive: true,
        mode: Mode::Idle,
        inventory: vec![7, 8],
        label: "scout".to_string(),
    });

    let mut scope = TickwiseScope::new();
    scope.component::<Unit>();
    let dump: HashMap<String, Value> = dump_of(&world, &scope).into_iter().collect();

    let index = world
        .iter_entities()
        .find(|entity| entity.contains::<Unit>())
        .unwrap()
        .id()
        .index_u32();
    let at = |suffix: &str| dump.get(&format!("Unit[{index}]{suffix}")).unwrap().clone();

    assert_eq!(dump.get("Unit.len"), Some(&Value::Len(1)));
    assert_eq!(at(".position.x"), Value::F32(1.5));
    assert_eq!(at(".position.y"), Value::F32(-2.0));
    assert_eq!(at(".hitpoints"), Value::I64(42));
    assert_eq!(at(".alive"), Value::Bool(true));
    assert_eq!(at(".label"), Value::Str("scout".to_string()));
    // A collection records its length, so a shorter list can never hide
    // behind a tail that happens to match.
    assert_eq!(at(".inventory"), Value::Len(2));
    assert_eq!(at(".inventory[0]"), Value::U64(7));
    assert_eq!(at(".inventory[1]"), Value::U64(8));
}

#[test]
fn enum_variants_are_recorded_by_name_and_by_field() {
    let mut world = World::new();
    world.spawn(Unit {
        mode: Mode::Idle,
        ..Default::default()
    });
    let mut scope = TickwiseScope::new();
    scope.component::<Unit>();
    let idle = paths(&world, &scope);
    assert!(
        idle.iter().any(|path| path.ends_with("].mode")),
        "unit variant is one leaf: {idle:?}"
    );
    let dump: HashMap<String, Value> = dump_of(&world, &scope).into_iter().collect();
    assert!(
        dump.values()
            .any(|value| *value == Value::Str("Idle".to_string())),
        "the variant name is the value"
    );

    let mut world = World::new();
    world.spawn(Unit {
        mode: Mode::Aiming { target: 9 },
        ..Default::default()
    });
    let aiming = paths(&world, &scope);
    assert!(
        aiming
            .iter()
            .any(|path| path.ends_with(".mode.Aiming.target")),
        "a struct variant nests under its name: {aiming:?}"
    );

    let mut world = World::new();
    world.spawn(Unit {
        mode: Mode::Carrying(4),
        ..Default::default()
    });
    let carrying = paths(&world, &scope);
    assert!(
        carrying
            .iter()
            .any(|path| path.ends_with(".mode.Carrying.0")),
        "a tuple variant numbers its fields: {carrying:?}"
    );
}

#[test]
fn map_iteration_order_never_reaches_the_hash() {
    // The same entries inserted in opposite orders. Two hash maps in one
    // process do not agree on iteration order, so a walk that trusted it
    // would report two identical worlds as different.
    let keys: Vec<String> = (0..24).map(|index| format!("item-{index:02}")).collect();

    let mut forward = World::new();
    let mut counts = HashMap::new();
    for (index, key) in keys.iter().enumerate() {
        counts.insert(key.clone(), index as i32);
    }
    forward.insert_resource(Inventory { counts });

    let mut backward = World::new();
    let mut counts = HashMap::new();
    for (index, key) in keys.iter().enumerate().rev() {
        counts.insert(key.clone(), index as i32);
    }
    backward.insert_resource(Inventory { counts });

    let mut scope = TickwiseScope::new();
    scope.resource::<Inventory>();

    assert_eq!(
        BevyProbe::new(&forward, &scope).full_hash(),
        BevyProbe::new(&backward, &scope).full_hash(),
        "the hash depends on the entries, not on their iteration order"
    );
    assert_eq!(dump_of(&forward, &scope), dump_of(&backward, &scope));

    let dump: HashMap<String, Value> = dump_of(&forward, &scope).into_iter().collect();
    assert_eq!(dump.get("Inventory.counts"), Some(&Value::Len(24)));
    assert_eq!(
        dump.get("Inventory.counts[item-07]"),
        Some(&Value::I64(7)),
        "map entries are keyed by their rendered key"
    );
}

#[test]
fn entities_are_walked_in_index_order_not_archetype_order() {
    // Adding a second component moves an entity to another archetype,
    // which changes iteration order without changing the simulation. The
    // walk sorts by entity index, so the hash does not notice.
    let mut plain = World::new();
    let first = plain
        .spawn(Unit {
            hitpoints: 1,
            ..Default::default()
        })
        .id();
    plain.spawn(Unit {
        hitpoints: 2,
        ..Default::default()
    });
    plain.spawn(Unit {
        hitpoints: 3,
        ..Default::default()
    });

    let mut scope = TickwiseScope::new();
    scope.component::<Unit>();
    let before = BevyProbe::new(&plain, &scope).full_hash();
    let before_dump = dump_of(&plain, &scope);

    // An unregistered component changes the archetype and nothing else.
    plain.entity_mut(first).insert(Unregistered { noise: 5 });
    assert_eq!(
        before,
        BevyProbe::new(&plain, &scope).full_hash(),
        "moving an entity between archetypes is not a state change"
    );
    assert_eq!(before_dump, dump_of(&plain, &scope));
}

#[test]
fn an_unregistered_type_is_invisible() {
    // Coverage is a decision, and this is the cost of it: what you leave
    // out cannot be caught. The blind spot is real and worth a test.
    let mut world = World::new();
    let entity = world
        .spawn((Unit::default(), Unregistered { noise: 1 }))
        .id();
    world.insert_resource(Weather { wind: 3.5 });

    let mut scope = TickwiseScope::new();
    scope.component::<Unit>();
    let before = BevyProbe::new(&world, &scope).full_hash();

    world
        .entity_mut(entity)
        .get_mut::<Unregistered>()
        .unwrap()
        .noise = 99;
    world.resource_mut::<Weather>().wind = 100.0;
    assert_eq!(before, BevyProbe::new(&world, &scope).full_hash());

    // Register it and the same change is caught.
    scope.component::<Unregistered>();
    let with_coverage = BevyProbe::new(&world, &scope).full_hash();
    world
        .entity_mut(entity)
        .get_mut::<Unregistered>()
        .unwrap()
        .noise = 100;
    assert_ne!(with_coverage, BevyProbe::new(&world, &scope).full_hash());
}

#[test]
fn the_light_hash_covers_only_what_was_promoted() {
    let mut world = World::new();
    let entity = world.spawn(Unit::default()).id();
    world.insert_resource(Weather { wind: 1.0 });

    let mut scope = TickwiseScope::new();
    scope.component::<Unit>();
    scope.light_resource::<Weather>();

    let probe = BevyProbe::new(&world, &scope);
    let light = probe.light_hash();
    let full = probe.full_hash();
    assert_ne!(light, full, "the two hashes cover different ground");

    // A change to a full-hash-only type moves the full hash alone.
    world
        .entity_mut(entity)
        .get_mut::<Unit>()
        .unwrap()
        .hitpoints = 7;
    let probe = BevyProbe::new(&world, &scope);
    assert_eq!(light, probe.light_hash());
    assert_ne!(full, probe.full_hash());

    // A change to a promoted type moves both.
    let full = probe.full_hash();
    world.resource_mut::<Weather>().wind = 2.0;
    let probe = BevyProbe::new(&world, &scope);
    assert_ne!(light, probe.light_hash());
    assert_ne!(full, probe.full_hash());
}

#[test]
fn a_missing_resource_is_recorded_rather_than_skipped() {
    let world = World::new();
    let mut scope = TickwiseScope::new();
    scope.resource::<Weather>();

    let dump: HashMap<String, Value> = dump_of(&world, &scope).into_iter().collect();
    assert_eq!(
        dump.get("Weather"),
        Some(&Value::Null),
        "present on one machine and absent on the other is a difference"
    );

    let mut with_it = World::new();
    with_it.insert_resource(Weather::default());
    assert_ne!(
        BevyProbe::new(&world, &scope).full_hash(),
        BevyProbe::new(&with_it, &scope).full_hash()
    );
}

#[test]
fn registration_is_idempotent_and_order_independent() {
    let mut one = TickwiseScope::new();
    one.component::<Unit>();
    one.resource::<Weather>();
    one.light_resource::<Weather>();
    // The same type twice promotes it instead of walking it twice.
    one.component::<Unit>();
    assert_eq!(one.len(), 2);

    let mut two = TickwiseScope::new();
    two.light_resource::<Weather>();
    two.component::<Unit>();
    assert_eq!(
        two.labels().collect::<Vec<_>>(),
        one.labels().collect::<Vec<_>>()
    );

    let mut world = World::new();
    world.spawn(Unit {
        hitpoints: 3,
        ..Default::default()
    });
    world.insert_resource(Weather { wind: 0.25 });
    assert_eq!(
        BevyProbe::new(&world, &one).full_hash(),
        BevyProbe::new(&world, &two).full_hash(),
        "registration order must not change the hash"
    );
    assert_eq!(
        BevyProbe::new(&world, &one).light_hash(),
        BevyProbe::new(&world, &two).light_hash()
    );
}

#[test]
fn an_empty_scope_hashes_nothing_and_says_so() {
    let scope = TickwiseScope::new();
    assert!(scope.is_empty());

    let mut world = World::new();
    world.spawn(Unit {
        hitpoints: 1,
        ..Default::default()
    });
    let before = BevyProbe::new(&world, &scope).full_hash();
    world.insert_resource(Weather { wind: 9.0 });
    assert_eq!(
        before,
        BevyProbe::new(&world, &scope).full_hash(),
        "with nothing registered, two different worlds hash the same"
    );
}
