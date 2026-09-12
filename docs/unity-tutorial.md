# Find your first desync in Unity in 15 minutes

This tutorial walks Pass 1 of the Tickwise workflow inside a Unity project. You will install the package, run a small deterministic game twice, once with a planted bug, and find the exact tick where the two runs diverged, without reading a single log file. The last section shows how to wire the same three calls into your own game.

The Rust version of this tutorial, [Find your first desync in 15 minutes](tutorial.md), goes further and names the field that went wrong. The package can do that too, through state dumps and `TickwiseReplayer`; section 6 says how. Recording and compare, the part that tells you the tick, is what the timed part of this tutorial covers.

Time budget: about 7 minutes for setup, 8 for the workflow.

## 0. Setup, 7 minutes

You need Unity 2022.3 or newer, a stable Rust toolchain from [rustup](https://rustup.rs) for the command line tool, and git.

Install the command line tool. It reads and compares recordings; nothing about it is Unity specific:

```
cargo install tickwise-cli
```

Open a Unity project, an empty one is fine, and add the package by git URL. In the Package Manager window choose the plus button, then Install package from git URL, and paste:

```
https://github.com/cosgunhalil/Tickwise.git#unity/v0.1.0
```

Or add the same line to `Packages/manifest.json` under `dependencies`:

```
"com.cosgunhalil.tickwise": "https://github.com/cosgunhalil/Tickwise.git#unity/v0.1.0"
```

The package brings the native library for Windows, macOS, Linux, Android, and iOS with it, so there is nothing to build. Select Tickwise in the Package Manager window, open the Samples tab, and import Deterministic Mini Game. It lands under `Assets/Samples/Tickwise`.

## 1. Record a clean session, 1 minute

Open the imported scene, `DeterministicMiniGame`, and select the `Tickwise Sample` object. Its inspector shows a Session Name of `clean` and an Inject Chaos toggle that is off. Press Play.

Eight balls bounce in a box for ten seconds, six hundred ticks at sixty ticks per second, then the session finishes on its own and the console prints where the recording went:

```
Tickwise: finished 600 ticks, saved C:/Users/you/AppData/LocalLow/DefaultCompany/YourProject/tickwise/clean.rec
Run again with a different Session Name and Inject Chaos toggled, then compare:
  tickwise compare ".../tickwise/clean.rec" ".../tickwise/chaotic.rec"
```

The path is under `Application.persistentDataPath`. Look inside the file from a terminal:

```
tickwise inspect ".../tickwise/clean.rec"
```

```
  format         version 1
  game           tickwise-mini-game
  build          1.0
  platform       WindowsEditor
  tick rate      60 ticks per second
  rng seed       0x0000000000003039
  created at     unix 1788698198
  full hashes    every 50 ticks
  snapshots      every 300 ticks
  hash algo      id 1
  input format   id 1

  ticks          600
  file size      6.6 KiB

  chunks
    input frames                 14        210 B   repeat suppressed
    light hash batches           10      4.9 KiB   holding 600 hashes
    full hashes                  12        264 B
    snapshots                     2        332 B   at ticks 0, 300
    markers                       1         23 B

  integrity      checksum ok
  next           record a second session, then find the first divergent tick:
                 tickwise compare a.rec b.rec
```

Six hundred ticks of gameplay fit in under seven kilobytes. Inputs are stored only when they change, fourteen times in this session, and every tick carries a hash of the state. The hash algorithm id says the hashes are xxh3, computed by the same native code a Rust client would use.

## 2. Record a sabotaged session, 1 minute

Leave Play mode. In the inspector set Session Name to `chaotic` and turn Inject Chaos on. Press Play again and wait for the console line.

The sample now has a bug. From tick 421 on, it reads the wall clock and adds the value to the first ball's position. Same seed, same scripted inputs, same code, one leaked value, which is what a real desync between two players' machines looks like: the simulations agree until something that is not part of the deterministic state gets into it.

## 3. Find the tick, 1 minute

Copy the compare command from the console and run it:

```
tickwise compare ".../tickwise/clean.rec" ".../tickwise/chaotic.rec"
```

```
comparing clean.rec and chaotic.rec

  first          600 ticks, game tickwise-mini-game, seed 0x3039
  second         600 ticks, game tickwise-mini-game, seed 0x3039

  verdict        first divergence at tick 421, caught by the light hash, confirmed by the full hash at tick 450, last agreement at tick 420

  next           Pass 2: replay each recording in your own loop with
                 dump_at_ticks = [421] to produce two .dump files, then run
                 tickwise diff a.dump b.dump
```

The two runs agree through tick 420 and disagree from tick 421, which is exactly where the bug was planted. The light hash, computed every tick, caught it on the first divergent tick, and the next full hash confirmed it. The exit code is 1, so a build script can branch on the verdict.

Compare needs nothing but the two files. It runs offline, in milliseconds, on any machine, which is the point: the two recordings can come from two players on two continents, and the tool tells you the tick before anyone opens a debugger.

## 4. See the blind spot, 3 minutes

Open `MiniGameSim.cs` in the imported sample and look at `LightHash`. It hashes the score, the random generator state, the tick, and one sum over every ball's position. Delete the position sum, so the light hash covers score, RNG, and tick only, and record both sessions again.

```
  verdict        first divergence at tick 439, caught by the light hash, confirmed by the full hash at tick 450, last agreement at tick 438
```

The bug still struck at tick 421, but the report says 439. Moving a ball does not change the score or the random generator, so the light hash noticed nothing until eighteen ticks later, when the displaced ball hit a wall at a different time and the score changed. The full hash, which covers every ball, caught it at tick 450 either way.

Those eighteen ticks are the light hash blind spot, and the report is the tool telling you a field is missing from the cheap hash. What goes into the light hash is the one design decision Tickwise leaves to you. The [hash coverage checklist](hash-coverage.md) walks through it. Put the position sum back before moving on.

## 5. Point it at your own game

Three integration points, and Tickwise never touches your game loop beyond them.

**A probe.** Implement `IDeterminismProbe` on your simulation, or on a small adapter next to it. Two methods: a cheap `LightHash` called every tick and a `FullHash` covering all gameplay state, called on the ticks the recorder keeps a full hash, every 300 by default. `Xxh3.Hash64` hashes any span of bytes with the same algorithm the Rust side uses:

```csharp
using System;
using Tickwise;

public sealed class MatchState : IDeterminismProbe
{
    public ulong Tick;
    public ulong Score;
    public uint RngState;
    public int[] UnitX = new int[64];
    public int[] UnitY = new int[64];

    private readonly byte[] _light = new byte[24];
    private readonly byte[] _full = new byte[24 + 64 * 8];

    public ulong LightHash()
    {
        // Desync-critical values only. Stay well under one percent of the tick.
        BitConverter.TryWriteBytes(new Span<byte>(_light, 0, 8), Tick);
        BitConverter.TryWriteBytes(new Span<byte>(_light, 8, 8), Score);
        BitConverter.TryWriteBytes(new Span<byte>(_light, 16, 4), RngState);
        return Xxh3.Hash64(_light);
    }

    public ulong FullHash()
    {
        // Everything that is gameplay state. Anything left out is a blind spot.
        Span<byte> buf = _full;
        BitConverter.TryWriteBytes(buf.Slice(0, 8), Tick);
        BitConverter.TryWriteBytes(buf.Slice(8, 8), Score);
        BitConverter.TryWriteBytes(buf.Slice(16, 4), RngState);
        for (int i = 0; i < 64; i++)
        {
            BitConverter.TryWriteBytes(buf.Slice(24 + i * 8, 4), UnitX[i]);
            BitConverter.TryWriteBytes(buf.Slice(28 + i * 8, 4), UnitY[i]);
        }
        return Xxh3.Hash64(_full);
    }
}
```

**A recorder.** Create one when the match starts, call `RecordTick` once per simulation tick with the input bytes your netcode already sends, and dispose it when the match ends. Disposing finishes the file, so a `using` block or an `OnDestroy` is the whole lifecycle:

```csharp
var config = new RecorderConfig
{
    GameId = "my-game",
    BuildHash = Application.version,
    Platform = Application.platform.ToString(),
    TickRate = 60,
    RngSeed = matchSeed,
    HashAlgoId = HashAlgo.Xxh3,
    InputFormatId = 3,   // bump this whenever your input bytes change meaning
}.StampCreatedAt();

_recorder = TickwiseRecorder.Create(path, config);

// inside your fixed tick, after the simulation stepped:
_recorder.RecordTick(tick, inputBytes, matchState);

// when the match ends, or in OnDestroy:
_recorder.Dispose();
```

`RecordTick` asks the probe for `LightHash` every tick and for `FullHash` only on the ticks it will keep, so the expensive hash never runs for nothing. If you compute hashes elsewhere, the overload taking two `ulong` values records them directly, and `WantsFullHash` tells you when the second one matters.

**Comparing** needs no code at all, only the command line tool and two files.

Two things to get right. First, what your light hash covers decides what compare can catch on the first tick; section 4 showed what a gap looks like. Second, your input encoding is yours, so set `InputFormatId` and change it whenever the bytes change meaning. `TickwiseReplayer` with `CheckInputFormat` on refuses a recording made with an older encoding instead of feeding it to the wrong decoder.

A few Unity specifics. Record from `FixedUpdate` or from your own fixed step, never from `Update`, because a recording is only meaningful when every tick is a simulation tick. Keep the simulation in plain C# with no `UnityEngine` types in the hashed state, the way `MiniGameSim` does, because `Time`, `Random`, and physics results differ between machines by design. And write recordings under `Application.persistentDataPath`, which exists and is writable on every platform the package supports.

## 6. What comes next

The compare output ends with a hint about Pass 2: getting from the tick to the field. The package offers two roads. The sample already takes the first: `MiniGameSim` implements `ITickwiseStateWriter`, and the runner records a state dump every 100 ticks and one at the chaos tick, so with the two recordings from this tutorial you can run

```
tickwise diff clean.rec chaotic.rec --at 421
```

and read `balls[0].x` as the field that moved, which is exactly where the bug was planted. No replay was needed, because the dumps were taken during the original runs; that is what makes this road work for a desync that never reproduces. The cost is a full walk of the state every 100 ticks, so keep the interval coarse in a shipping build or leave it at zero.

The second road is `TickwiseReplayer`: open a recording, step your simulation through its inputs, and let `AfterTick` verify the hashes and collect a dump at exactly tick 421. The [package manual](../bridges/unity/com.cosgunhalil.tickwise/Documentation~/com.cosgunhalil.tickwise.md) shows the loop. Either way, the tick is the lead and the field is the answer.

The [package README](../bridges/unity/com.cosgunhalil.tickwise/README.md) lists the supported platforms and the release process, and the [main README](../README.md) lists what Tickwise deliberately does not do.
