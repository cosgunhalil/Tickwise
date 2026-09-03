# Hash coverage checklist

Tickwise only sees what your probe shows it. A desync in a field your hashes never touch is invisible to `compare`, and a field your dump never lists is invisible to `diff`. This checklist is about closing those gaps before they cost you a debugging session.

## The three layers, and what each one is for

**`light_hash`** runs every tick, so it has to be cheap: the budget is below 1 percent of the tick. It does not need to cover everything. Its job is to catch the divergence within a tick or two of when it happens, which means it should cover the state that changes on nearly every tick and that nearly every bug ends up touching.

**`full_hash`** runs every N ticks, 300 by default. It should cover every field that influences future simulation. It confirms what the light hash found, and it catches what the light hash missed: when the full hash disagrees while the light hash agreed, `compare` reports a blind spot and you know a field is missing from the light layer.

**`state_dump`** runs only during Pass 2, at the ticks you ask for, so cost barely matters. It should list everything the full hash covers, field by field, because it is what `diff` walks. A field in the full hash but not in the dump gives you a divergence you can confirm but never name.

The rule that ties them together: the dump and the full hash cover the same set, and the light hash covers a well-chosen subset of it.

## Checklist: what belongs in the full hash and the dump

Everything that can influence a future tick. Walk your state type and ask of each field: if this were different on two machines, could the simulations ever behave differently? If yes, it belongs.

- [ ] Every entity, and the number of entities. A missing count hides spawn and despawn bugs behind a length change that nothing hashes.
- [ ] Positions, velocities, rotations, and every other continuous quantity that feeds physics.
- [ ] The random generator state. Not the seed, the current state. This single field catches an enormous share of desyncs, because almost every gameplay system consumes randomness.
- [ ] Timers, cooldowns, and counters that gate behavior.
- [ ] Health, scores, resources, and every gameplay number.
- [ ] Pending events, command queues, and anything scheduled for a future tick, in order.
- [ ] Identifier counters and allocation sequences, for example the next entity id. Two machines that agree on state but disagree on the next id will diverge the moment something spawns.
- [ ] Cached or derived values that the simulation reads. If the cache is derived purely from hashed state it is redundant, but a stale cache is one of the classic desync causes, so hashing it turns that class of bug from invisible to caught.
- [ ] Per-player state including the players' own inputs as applied, if input handling has any state such as buffering.

What does not belong: rendering state, audio, interpolation for display, UI, debug overlays, anything derived from wall clock time, and profiling counters. These differ between machines by design and hashing them produces false divergences.

## Checklist: what belongs in the light hash

The light hash is a digest, not a copy. Aim for a handful of values that change often and sit downstream of most systems.

- [ ] The random generator state. Cheapest and most effective single field.
- [ ] The tick counter.
- [ ] Entity count, or counts per entity kind.
- [ ] Player positions, or the positions of the few most important entities.
- [ ] Score or an equivalent game-level aggregate.
- [ ] A rotating sample: hash one full entity per tick, cycling through them by tick index. Over a few hundred ticks every entity gets covered, at the cost of hashing one entity per tick. This is the best trick available for large worlds.

Do not try to make the light hash complete. That is what the full hash is for, and a complete light hash blows the budget in exactly the games that need Tickwise most.

## Reading a blind spot report

When `compare` says the divergence was caught by the full hash while the light hash saw nothing, three things follow.

1. The real divergence happened somewhere between the last agreeing full hash and the one that fired. Replay both recordings and dump a few ticks after the last agreement, then again near the full hash tick, to see the difference while it is still small. The tutorial's float-drift section shows this.
2. The field that `diff` names is missing from your light hash. Add it, or add the system it belongs to, to your light digest. Rerun the recording and `compare` will catch the same bug on the tick it happens.
3. Consider a shorter full hash interval while you are hunting. Every 30 ticks costs more than every 300, but it narrows the window.

## Determinism rules for the hashing itself

A probe can lie in ways that look like desyncs or hide real ones. These rules keep it honest.

- **Iterate in a fixed order.** Never hash by walking a `HashMap` or `HashSet`. Use `BTreeMap`, a `Vec`, or sort first. A hash map iterates differently in every process, so hashing one reports a desync that does not exist.
- **Hash float bits, not float values.** Use `to_bits` so that `0.0` and `-0.0` differ and NaN payloads are visible. Formatting or rounding floats before hashing hides the sub-epsilon drift you are looking for.
- **Hash fields, not memory.** Never hash the raw bytes of a struct. Padding bytes are uninitialized and differ between runs, which is a false desync, and two logically equal values with different padding are the uninit-read bug in disguise.
- **Keep wall clock time out.** Timestamps belong in the session metadata the recorder stores, never in a hash or a dump.
- **Mind platform-dependent math.** `sin`, `cos`, `exp`, and friends may round differently across operating systems and CPUs. That is a real cross-platform desync source, not a hashing bug, and Tickwise will report it as sub-epsilon float drift. Whether to fix it with fixed-point math or accept it is your call; the diff classifies, it does not judge.

## With the serde layer

`SerdeProbe::new(&state)` hashes the postcard encoding of the whole state for both hashes and dumps every field. It is the fastest way to get complete coverage, and for small states it is enough.

- Fields that should not be covered take `#[serde(skip)]`. This is how rendering state stays out.
- Hash maps in the state hash in iteration order and are not deterministic. Replace them with `BTreeMap`, or leave them out of the hashed view. Dumps are immune, since they sort by path.
- For the light hash budget, build a small view struct of the critical fields and pass it to `SerdeProbe::with_light(&state, &view)`. The view is your light digest checklist in code.
- Declare an input format id through `format_id("MyInput v1")` and bump the label when the input type changes. The replayer refuses recordings made with an older encoding, which prevents a whole category of confusing false desyncs.

## Quick self-test

Record a session. Replay it and record the replay. Run `compare` on the two. If the verdict is identical, your probe is deterministic on one machine. If it is not, fix that before comparing across machines, because a non-deterministic probe makes every cross-machine result meaningless.
