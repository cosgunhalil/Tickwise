# Find your first desync in 15 minutes

This tutorial walks the whole Tickwise workflow on the reference simulation that ships in this repository. You will record two sessions, one of them sabotaged, find the exact tick where they diverge, and name the field that went wrong. No game of your own is needed yet. The last section shows how to point the same tools at your simulation.

Time budget: about 5 minutes for setup, 10 for the workflow.

## 0. Setup, 5 minutes

You need a stable Rust toolchain from [rustup](https://rustup.rs) and git.

Install the command line tool:

```
cargo install tickwise-cli
```

Clone the repository for the reference simulation, called refsim. It is the small deterministic 2D world every example here uses, and it can sabotage itself on request:

```
git clone https://github.com/cosgunhalil/Tickwise.git
cd Tickwise
cargo build --release -p tickwise-refsim --examples
```

Everything below runs from the `Tickwise` directory. The refsim examples are invoked through cargo, so the first run of each compiles it.

## 1. Record a clean session, 1 minute

```
cargo run -q -p tickwise-refsim --example record_demo -- clean.rec
```

Output:

```
recorded 6000 clean ticks
```

Refsim just played 6000 ticks, about 100 seconds of game time at 60 ticks per second, with scripted inputs, and Tickwise recorded the inputs plus a light hash every tick and a full hash every 300 ticks. Look inside:

```
tickwise inspect clean.rec
```

```
clean.rec
  format         version 1
  game           tickwise-refsim
  build          m2-dev
  platform       windows
  tick rate      60 ticks per second
  rng seed       0x0000000000ddba11
  created at     unix 1756400000
  full hashes    every 300 ticks
  snapshots      every 1800 ticks
  hash algo      id 0
  input format   id 1

  ticks          6000
  file size      283.5 KiB

  chunks
    input frames               5932    104.3 KiB   repeat suppressed
    light hash batches           94     48.5 KiB   holding 6000 hashes
    full hashes                  20        440 B
    snapshots                     4        132 B   at ticks 0, 1800, 3600, 5400
    markers                      1         29 B

  integrity      checksum ok
  next           record a second session, then find the first divergent tick:
                 tickwise compare a.rec b.rec
```

The recording is small, the checksum is intact, and the tool already tells you the next step.

## 2. Record a sabotaged session, 1 minute

Refsim has chaos flags that inject a known class of non-determinism starting at a chosen tick. This one reads a stale value that should have been reinitialized, the classic cached-value bug, striking at tick 4021:

```
cargo run -q -p tickwise-refsim --example record_demo -- chaotic.rec --chaos uninit-read 4021
```

```
recorded 6000 ticks with uninit-read chaos from tick 4021
```

Same seed, same inputs, same code, one bug. This stands in for two players whose clients drifted apart mid-match.

## 3. Pass 1: find the tick, 1 minute

```
tickwise compare clean.rec chaotic.rec
```

```
comparing clean.rec and chaotic.rec

  first          6000 ticks, game tickwise-refsim, seed 0xddba11
  second         6000 ticks, game tickwise-refsim, seed 0xddba11

  verdict        first divergence at tick 4021, caught by the light hash, confirmed by the full hash at tick 4200, last agreement at tick 4020

  next           Pass 2: replay each recording in your own loop with
                 dump_at_ticks = [4021] to produce two .dump files, then run
                 tickwise diff a.dump b.dump
```

That is the answer most studios spend days reproducing: the sessions agree through tick 4020 and disagree from 4021. The light hash caught it on the first divergent tick and the next full hash confirmed it. The exit code is 1, so a script can branch on the verdict.

Pass 1 needs nothing but the two recordings. It runs offline, in milliseconds, on any machine.

## 4. Pass 2: rebuild the state at that tick, 3 minutes

Tickwise never runs your simulation. To see the state at tick 4021, you replay each recording through the simulation and ask Tickwise to dump the state when it gets there. The `replay_demo` example does exactly that for refsim:

```
cargo run -q -p tickwise-refsim --example replay_demo -- clean.rec clean.dump --dump-at 4021
```

```
wrote clean.dump with the state at tick 4021
replayed ticks 0 to 5999, every hash matched the recording
```

The second line matters. During replay, Tickwise compared every live hash against the recording, so the replay provably reproduced the original session. Now the sabotaged one, with the same chaos flags so the bug reproduces too:

```
cargo run -q -p tickwise-refsim --example replay_demo -- chaotic.rec chaotic.dump --dump-at 4021 --chaos uninit-read 4021
```

```
wrote chaotic.dump with the state at tick 4021
replayed ticks 0 to 5999, every hash matched the recording
```

## 5. Name the field, 1 minute

```
tickwise diff clean.dump chaotic.dump
```

```
diffing clean.dump and chaotic.dump

  first          game tickwise-refsim, seed 0xddba11, dumps at ticks 4021
  second         game tickwise-refsim, seed 0xddba11, dumps at ticks 4021
  float policy   f32 epsilon 1e-5, f64 epsilon 1e-12

tick 4021       1 difference over 41 fields: 0 structural, 1 exact, 0 sub-epsilon float drift
  exact          score: 3317 versus 4811663725493808200

  verdict        1 difference across 1 compared tick
  next           an exact difference at the first divergent tick is your lead. Trace that field's last write backwards through the tick
```

Forty fields agree, one does not. The score jumped from 3317 to a garbage number, because a stale scratch value was added into it. That is the stale-value bug, caught at the first tick it struck, named down to the field. You found your first desync.

## 6. Try the other chaos classes, 3 minutes

Each chaos class leaves a different fingerprint. Repeat steps 2 to 5 with a different flag and watch the reports change.

**float-drift** nudges one ball's velocity by a single bit each tick:

```
cargo run -q -p tickwise-refsim --example record_demo -- drift.rec --chaos float-drift 4021
tickwise compare clean.rec drift.rec
```

```
  verdict        divergence caught by the full hash at tick 4200, while the light hash saw nothing: the light hash has a blind spot, and the real divergence happened at or before this tick, last agreement at tick 3900
```

The verdict is different this time. The light hash never noticed, because refsim's light hash deliberately covers only a critical digest of the state and ball velocities are not in it. The full hash at tick 4200 caught it, and the report says plainly that the light hash has a blind spot and the real divergence is somewhere between 3900 and 4200. Dump both recordings at 4200 and diff:

```
tick 4200       1 difference over 41 fields: 0 structural, 1 exact, 0 sub-epsilon float drift
  exact          balls[0].velocity.x: -1.0577879 versus -1.0578094, delta 2.1457672119140625e-5
```

One field, `balls[0].velocity.x`, and by now the difference is past the default epsilon, so it counts as exact. The drift began much smaller. Dump at tick 4022 instead, one tick after the strike:

```
tick 4022       1 difference over 41 fields: 0 structural, 0 exact, 1 sub-epsilon float drift
  drift          balls[0].velocity.x: 1.0577879 versus 1.0577881, delta 2.384185791015625e-7
```

A single bit of difference, classified as sub-epsilon float drift, that compounded a hundredfold in 178 ticks. This is exactly what cross-platform float deviation looks like, and the two dumps together tell the story: where it started, how small it was, and how fast it grew. When a compare verdict names a blind spot, dumping a few ticks after the last agreement is the way to catch the drift while it is still small.

**hashmap-iter** folds state through a real hash map, whose iteration order is random per process. Compare finds tick 4021 as before. Now replay the sabotaged recording with its own chaos flags:

```
cargo run -q -p tickwise-refsim --example replay_demo -- hm.rec hm.dump --dump-at 4021 --chaos hashmap-iter 4021
```

```
replay_demo: replay diverged from the recording at tick 4021: light hash recorded 0e728cffcfbc7b43, replay produced 2a778fbcc6cc0bf8. Your simulation is not reproducing the session, run the self-check before hunting cross-client desyncs
```

The replay cannot reproduce the recording even with the same code and the same flags, because the new process iterates the map in a different order. That is the whole problem with unordered collections in a deterministic simulation, demonstrated live. The dump is still written, and diffing it against the clean one shows `score` differing, exact, since that is where the order-dependent fold ends up.

**time-dependent** leaks the wall clock into the random generator. Compare finds tick 4021 as before, and the replay fails verification at tick 4021 for the same reason the hash map did: wall clock time is different in every run. The dump written anyway shows a single exact difference on `rng.state`. Wall clock time must never reach a deterministic simulation, and here is what it looks like when it does.

## 7. The self-check, 1 minute

Before hunting a desync between two machines, make sure one machine agrees with itself. Replay a recording and record the replay, then compare the two. Refsim's replay verification does this implicitly, but the standalone version is simple:

```
tickwise compare original.rec replayed.rec
```

If the verdict is anything but identical, your simulation is not deterministic on a single machine, and you found out before your players did.

## 8. Point it at your own game

The workflow above is three integration points, and Tickwise never touches your game loop beyond them.

**Recording.** Implement the `DeterminismProbe` trait for your game state, three methods: a cheap `light_hash` called every tick, a `full_hash` covering all gameplay state, and a `state_dump` producing the field list the diff walks. Then call `record_tick` once per tick with your input bytes. With the `serde` feature, a `#[derive(Serialize)]` state gets all three methods for free through `SerdeProbe`, and `record_tick_typed` takes your input type directly:

```rust
use serde::Serialize;
use tickwise::serde_probe::SerdeProbe;
use tickwise::{Recorder, RecorderConfig};

#[derive(Serialize)]
struct Game { tick: u64, score: u64, positions: Vec<(f32, f32)> }

let mut rec = Recorder::create("session.rec", RecorderConfig::default())?;
// inside your loop, every tick:
rec.record_tick_typed(tick, &input, &SerdeProbe::new(&game))?;
// at the end:
rec.finish()?;
```

**Replaying.** Open the recording with `Replayer`, loop over `next_step`, apply each step's inputs to your simulation, advance it, and call `after_tick` with the probe. Dumps land at the ticks you asked for.

**Comparing and diffing** need no code at all, only the CLI.

Two things to get right. First, what your light hash covers decides what compare can catch on the first tick; the blind spot report in step 6 is the tool telling you a field is missing from it. The [hash coverage checklist](hash-coverage.md) walks through what belongs in each layer. Second, your input encoding is yours, so declare an input format id in the recorder config and the replayer will refuse recordings made with an older encoding instead of feeding them to the wrong decoder.

From here, the API reference on docs.rs covers every type, and the README lists what Tickwise deliberately does not do.
