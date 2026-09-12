# Tickwise for Unreal Engine

Record and compare deterministic simulations to find desyncs, from inside an Unreal project.

[Tickwise](https://github.com/cosgunhalil/Tickwise) finds the first tick where two runs of the same simulation stopped agreeing. This plugin is its Unreal bridge: a C++ module over the `tickwise-ffi` C ABI, with a recorder component and a probe interface usable from C++ and Blueprint.

## Status

Under construction, milestone M9 of the Tickwise v2 roadmap. The component records and, with `DumpInterval` set, dumps state on an interval so `tickwise diff` reaches field level with no replay; Pass 2 replay is available from C++ through `tickwise::Replayer` in the shared header. Built against Unreal Engine 4.26; the module has no 4.26-specific code and is expected to build on 5.x, which is unverified until someone tries.

## The shape of it

1. Implement the **Tickwise Probe** interface on the actor that owns your simulation, in C++ or Blueprint. Two functions: `LightHash`, cheap and called every tick, and `FullHash`, covering everything and called every 300 ticks by default.
2. Add a **Tickwise Recorder** component, set `GameId` and `RngSeed`, and call `StartRecording("clean.rec")`. A relative path lands under the project's `Saved` folder.
3. From your fixed simulation step, after the step has run, call `SetInputs` with the bytes your netcode already sends and then `RecordTick`.

```cpp
// The probe, on the actor that owns the simulation.
int64 AMatch::LightHash_Implementation() const
{
    return Hasher.Reset().Add(Score).Add(RngState).FinishSigned();
}

int64 AMatch::FullHash_Implementation() const
{
    Hasher.Reset().Add(Score).Add(RngState);
    for (const FUnit& Unit : Units)
    {
        Hasher.Add(Unit.Cell).Add(Unit.Health);
    }
    return Hasher.FinishSigned();
}

// The fixed step, wherever yours lives.
void AMatch::StepSimulation(const TArray<uint8>& Inputs)
{
    Advance(Inputs);
    Recorder->SetInputs(Inputs);
    Recorder->RecordTick();
}
```

Run the game twice, or once on each of two machines, then compare offline:

```
cargo install tickwise-cli
tickwise compare clean.rec chaotic.rec
```

```
  verdict        first divergence at tick 421, caught by the light hash,
                 confirmed by the full hash at tick 450, last agreement at tick 420
```

To get from the tick to the field, give the recordings state dumps. Implement `ITickwiseStateWriter` beside the probe and set `DumpInterval` on the component; `RecordDump` adds one on demand, for example next to a round start marker. Each dump walks all of gameplay state, which is why the interval is opt-in.

```cpp
void AMatch::WriteState(FTickwiseDump& Dump) const
{
    Dump.Int(TEXT("Score"), Score).Int(TEXT("Rng"), RngState).Length(TEXT("Units"), Units.Num());
    for (int32 i = 0; i < Units.Num(); ++i)
    {
        const FString Unit = FString::Printf(TEXT("Units[%d]"), i);
        Dump.IntPoint(Unit + TEXT(".Cell"), Units[i].Cell).Int(Unit + TEXT(".Health"), Units[i].Health);
    }
}
```

```
tickwise diff clean.rec chaotic.rec --at 450
```

The interface is C++ only: a Blueprint cannot fill a dump. Replaying a recording, Pass 2 proper, is a plain C++ loop over `tickwise::Replayer` from `tickwise/tickwise.hpp`, which every plugin source already sees through `TickwiseNative.h`; the shared layer's README shows the loop.

## Why the component does not record on its own

Unreal's frame is a variable timestep. A recording is only meaningful when every tick is a simulation tick, so the component records when you call `RecordTick` and never guesses. For a game that steps once per component tick at a fixed frame rate, `bRecordEveryComponentTick` makes the call for you; it is off by default, and the component ticks in `TG_PostUpdateWork` so it sees the state the frame produced.

## What is in the module

| Type | Purpose |
|---|---|
| `UTickwiseRecorderComponent` | The session: `StartRecording`, `StopRecording`, `SetInputs`, `RecordTick`, `RecordMarker`, `RecordDump`, status getters, and the session properties including `DumpInterval` as `UPROPERTY` |
| `ITickwiseProbe` | The two-hash interface, `BlueprintNativeEvent` so Blueprint can implement it |
| `ITickwiseStateWriter` | The optional C++ interface that writes state into a dump by field name, for `tickwise diff` |
| `FTickwiseDump` | The dump builder with typed setters and overloads for vectors, rotators, and points, each field under a dotted path |
| `FTickwiseHasher` | C++ hashing over `Add` overloads for the common engine types, in a fixed byte layout, so two machines hash the same fields the same way |
| `UTickwiseHashLibrary` | Blueprint hashing: `HashBytes`, `HashInts`, `HashFloats`, `HashString`, `CombineHashes` |
| `FTickwiseModule` | Loads the native library from the plugin folder at startup and refuses to record when the ABI does not match |

Hashes are xxh3, recorded under `hash_algo_id` 1, so a recording from Unreal is comparable with one from the Rust or Unity sides that hash the same bytes.

## Installing into a project

From a repository checkout, the plugin builds without staging anything: `Tickwise.Build.cs` falls back to the sibling `tickwise-ffi` and `tickwise-cpp` folders. Build the native library once with `cargo build --release` inside `bridges/tickwise-ffi`, then add the plugin folder to your project's `Plugins/`.

For a project outside the repository, stage first and copy the result:

```
pwsh bridges/tickwise-unreal/scripts/stage-ffi.ps1
```

This fills `Tickwise/Source/ThirdParty/TickwiseFfi/` with the headers, the Windows import library, and the DLL, and the folder then stands on its own. Released plugin archives ship staged.

## Building without the editor

Unreal's automation tool compiles a plugin against a temporary host project, which is how this plugin is verified without opening the editor:

```
pwsh bridges/tickwise-unreal/scripts/build-plugin.ps1
```

The script stages the native library, then runs `RunUAT BuildPlugin` for Win64 and leaves the packaged plugin in `bridges/tickwise-unreal/build/Tickwise`, ready to copy into a project's `Plugins` folder. Pass `-EngineRoot` for an engine installed somewhere other than the 4.26 launcher path. Staging comes first because the automation tool copies the plugin out of the repository before building it. Continuous integration has no engine and verifies the layout only: the `.uplugin` parses, the module and its build rules exist, and the C++ layer underneath compiles and passes its harness on Windows, macOS, and Linux.

## Validating by hand

1. Create an empty C++ project in 4.26, copy the staged `Tickwise` folder into `Plugins/`, and open it. The output log should show `Tickwise native library 0.1.0 ready`.
2. Add a Tickwise Recorder component to any actor, implement the probe on it with a couple of counters, and call `StartRecording` from `BeginPlay` and `RecordTick` from `Tick` with `bRecordEveryComponentTick` on, at a fixed frame rate.
3. Play twice. Change one counter's starting value between runs. Compare the two files under `Saved/` and read the verdict.

## License

MIT OR Apache-2.0, at your option, like the rest of Tickwise.
