# Tickwise for Unity

Tickwise records what a deterministic simulation did, one tick at a time, so two machines that were supposed to agree can be compared afterwards and the first tick where they disagreed can be named.

This manual is written alongside the package and grows with it. The `Tickwise.Runtime` classes carry the API reference in their doc comments.

## What a recording holds

Once per tick your code hands Tickwise three things: the input bytes for that tick, a cheap hash of desync-critical state called the light hash, and, every N ticks, a full hash of all gameplay state. Tickwise writes them into a `.rec` file together with session metadata such as the game identifier, build hash, platform, tick rate, and seed. With `RecorderConfig.DumpInterval` set, the file also carries a state dump every N ticks: every field of gameplay state by name, written by your `ITickwiseStateWriter`.

Tickwise never runs your simulation and never calls back into your code. You compute the hashes and write the dumps; the package tells you when the expensive work is due.

## The two-pass workflow

1. **Record on both machines.** Each client records its own `.rec` file during the match.
2. **Compare offline.** `tickwise compare a.rec b.rec` prints the first divergent tick, which hash caught it, and the last tick where both sides agreed.
3. **Diff at field level.** Two ways to get there:
   - **With no replay**, when both recordings carry dumps: `tickwise diff a.rec b.rec --at <tick>` compares the dumps at a shared tick and names the fields that differ. Compare's output names the first shared dump after the divergence. This works for a desync that never reproduces, at the cost of a full state walk every `DumpInterval` ticks inside the game loop.
   - **With a replay**, for the exact tick compare named: open each recording with `TickwiseReplayer`, step your simulation through the recorded inputs, and let `AfterTick` verify the hashes and collect a dump at `DumpAtTicks`. `Finish` writes a `.dump`, and `tickwise diff a.dump b.dump` does the rest. A `HashMismatch` along the way means your simulation is not reproducing its own recording, which is the first thing to fix.

```csharp
public sealed class Match : IDeterminismProbe, ITickwiseStateWriter
{
    public ulong LightHash() { /* cheap, every tick */ }
    public ulong FullHash() { /* everything, on the recorder's interval */ }
    public void WriteState(TickwiseDump dump)
    {
        dump.SetUInt64("score", score);
        dump.SetLength("units", units.Count);
        for (int i = 0; i < units.Count; i++)
        {
            dump.SetInt64($"units[{i}].x", units[i].X);
        }
    }
}

// Pass 2, in your own loop:
using var replayer = TickwiseReplayer.Open("a.rec", new ReplayOptions { DumpAtTicks = new ulong[] { 4021 } });
while (replayer.TryNextStep(out ulong tick, out ReadOnlySpan<byte> inputs))
{
    match.Step(inputs);
    replayer.AfterTick(tick, match);
}
replayer.Finish("a.dump");
```

`TryNearestSnapshotBefore` and `SeekTo` let a replay start from a snapshot you recorded and restore yourself, instead of from tick 0.

## Installing the native library

Released versions of this package carry `tickwise_ffi` for Windows, macOS, Linux, Android, and iOS under `Runtime/Plugins`. When working from a source checkout, build it yourself with `bridges/tickwise-ffi/scripts/build-for-unity.ps1`.

## Getting the command line tool

```
cargo install tickwise-cli
```

Or download a release binary from the Tickwise repository.

## Further reading

- The Unity tutorial, fifteen minutes from install to a caught desync: https://github.com/cosgunhalil/Tickwise/blob/main/docs/unity-tutorial.md
- The Tickwise repository: https://github.com/cosgunhalil/Tickwise
- The Rust tutorial, which walks the same workflow on the reference simulation: https://github.com/cosgunhalil/Tickwise/blob/main/docs/tutorial.md
- The hash coverage checklist, which says what belongs in each hash: https://github.com/cosgunhalil/Tickwise/blob/main/docs/hash-coverage.md
