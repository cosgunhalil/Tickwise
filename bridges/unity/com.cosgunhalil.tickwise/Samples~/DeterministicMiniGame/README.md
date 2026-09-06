# Deterministic Mini Game

Eight balls bounce in a box, simulated with integer math and recorded by Tickwise every tick. The sample exists to show the whole Pass 1 loop inside Unity in one Play session, and to let you catch a planted bug with the command line tool.

## Run it

1. Open `DeterministicMiniGame.unity` and select the `Tickwise Sample` object.
2. Press Play with **Session Name** set to `clean` and **Inject Chaos** off. The session records 600 ticks, ten seconds, and finishes on its own.
3. Set **Session Name** to `chaotic`, turn **Inject Chaos** on, and press Play again.
4. Copy the `tickwise compare` command from the console and run it.

Expected output: the first divergence at tick 421, caught by the light hash, confirmed by the full hash at tick 450. That is the tick where the sample starts leaking a wall clock value into the first ball's position.

The recordings are written under `Application.persistentDataPath/tickwise/`. The console prints the full path. Each Play session overwrites the file with the same session name, so change the name between runs.

## What to look at

- `MiniGameSim.cs` has no engine code. It steps on demand and implements `IDeterminismProbe` with a cheap light hash over score, RNG state, tick, and a sum of positions, and a full hash over every ball. That separation is what makes a simulation recordable and, later, replayable.
- **An experiment.** Remove the position sum from `LightHash` and run both sessions again. Compare still finds the bug, but later, on the first tick where the moved ball bounces at a different time and changes the score, and the report says the full hash at tick 450 confirmed it. The ticks in between are the light hash blind spot. Choosing what goes into the light hash is the one design decision Tickwise leaves to you, and the [hash coverage checklist](https://github.com/cosgunhalil/Tickwise/blob/main/docs/hash-coverage.md) is the guide.
- `TickwiseSampleRunner.cs` is the glue: it creates a `TickwiseRecorder` in `Awake`, records from `FixedUpdate`, and disposes it when the session ends or Play mode stops. Disposing finishes the file, so leaving Play early still leaves a readable recording.
- Inputs are scripted, not read from the keyboard, so two runs differ only by the planted bug. In a real game the input bytes are whatever your netcode already sends.

## Requirements

The `tickwise` command line tool: `cargo install tickwise-cli`. The native library must be present under the package's `Runtime/Plugins` folder; released package versions include it.
