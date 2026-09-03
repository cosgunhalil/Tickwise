# The light hash budget

`light_hash` runs every tick of your game, forever, on every player's machine. The rule for it is simple: it costs below 1 percent of the tick. This guide explains where the rule comes from, how to measure against it, and what the reference simulation measures at.

## Where the 1 percent comes from

A recording that costs 1 percent of the tick is one you can leave on in production builds and forget about. That is the goal: Pass 1 always on, so the recording of the desync already exists when a player reports it. Above a few percent, teams start turning recording off in release builds, and the first desync in the wild has no recording.

The budget applies to the light hash alone because it is the only per-tick cost that scales with your design choices. The recorder's own overhead is fixed and small, the full hash is amortized over its interval, and the state dump runs only during Pass 2 where cost does not matter.

## Measuring

Run the benchmarks:

```
cargo bench -p tickwise-refsim
cargo bench -p tickwise
```

For your own game, the recipe is the same three measurements: time one simulation tick, time one `light_hash` call, and divide. Criterion, the bench harness used here, is a good fit, but a stopwatch around a million calls works too. Measure `full_hash` as well and divide by your full hash interval to get its amortized per-tick share.

## What the reference simulation measures

Measured on a Windows x86_64 desktop with Tickwise 0.2.0. Absolute numbers vary by machine; the ratios are what matter.

| Configuration | Tick | `light_hash` | Share | `full_hash` | Amortized over 300 ticks |
|---|---|---|---|---|---|
| refsim, 8 balls | 37 ns | 32 ns | 86 percent | 131 ns | 0.4 ns per tick, 1.2 percent |
| refsim, 1000 balls | 3.3 µs | 32 ns | 0.95 percent | 14.4 µs | 48 ns per tick, 1.4 percent |

Two lessons hide in that table.

First, the 8 ball row looks alarming and means nothing. Refsim's default tick is 37 nanoseconds because it is a toy, and no real game simulates a tick in 37 nanoseconds. A percentage rule needs a realistic denominator. At 1000 balls the tick is 3.3 microseconds, still tiny by game standards, and the light hash already sits under 1 percent.

Second, the light hash cost did not move between the rows. Refsim's light hash covers the tick counter, score, entity counts, RNG state, and player positions, none of which grow with the ball count. That is the design the [hash coverage checklist](hash-coverage.md) recommends: a digest of critical state, not a walk over everything. The full hash does walk everything, which is why it grows a hundredfold and why it runs every 300 ticks instead of every tick.

## Putting real numbers to it

Suppose your game simulates at 60 ticks per second and the simulation itself takes 2 milliseconds of each 16.7 millisecond frame. The tick is 2 ms, so the light hash budget is 20 microseconds. That is enough to hash roughly a hundred thousand bytes with xxh3, or to walk several hundred entities and mix a few fields from each. Most games need far less: the RNG state, the entity count, and a dozen positions hash in well under a microsecond.

The recorder's own per-tick cost, measured with a trivial probe so nothing but Tickwise is timed:

| Operation | Cost |
|---|---|
| `record_tick`, inputs unchanged from last tick | 20 ns |
| `record_tick`, inputs change every tick | 96 ns |

Both are far below any plausible budget. Repeat suppression is why the first row is cheaper: when inputs do not change, no input frame is written.

## When the budget is tight

If your light hash approaches the budget, in this order:

1. **Hash less, not slower.** Drop fields that other hashed fields already imply. Positions imply velocities one tick later; a checksum of the entity list implies its count.
2. **Rotate.** Hash one entity per tick, cycling by tick index. Every entity is covered every N ticks at the cost of one entity per tick. Divergences are still caught within N ticks, and the full hash backs it up.
3. **Shorten the full hash interval instead.** If the light hash must stay tiny, a full hash every 60 ticks instead of 300 narrows the window compare reports without touching the per-tick cost.
4. **Move cost to `full_hash`.** Anything you remove from the light hash should still be in the full hash. The blind spot report from `compare` tells you when a light hash has become too thin.

## Offline costs, for completeness

`compare` over two recordings of 100,000 ticks each takes about 45 milliseconds on the same machine, including reading both files. A typical five minute session at 60 ticks per second is 18,000 ticks and compares in a few milliseconds. `state_dump` on the 1000 ball world takes about a millisecond, which is why it only runs in Pass 2 at the ticks you ask for.
