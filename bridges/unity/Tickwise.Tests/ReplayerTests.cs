using System;
using System.IO;
using Xunit;

namespace Tickwise.Tests
{
    /// <summary>
    /// Pass 2 through the wrapper: dumps recorded on an interval, the
    /// replayer verifying a recording and collecting dumps, and the CLI
    /// reading both back.
    /// </summary>
    public class ReplayerTests : IDisposable
    {
        private const ulong Ticks = 600;
        private const ulong DivergenceAt = 421;

        private readonly string _dir;

        public ReplayerTests()
        {
            _dir = Path.Combine(Path.GetTempPath(), "tickwise-unity-replay-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(_dir);
        }

        public void Dispose()
        {
            try
            {
                Directory.Delete(_dir, recursive: true);
            }
            catch (IOException)
            {
                // A leftover temp folder is not worth failing a test over.
            }
        }

        private string PathFor(string name) => Path.Combine(_dir, name);

        private static RecorderConfig SessionConfig()
        {
            return new RecorderConfig
            {
                GameId = "dotnet-replay-tests",
                Platform = "dotnet",
                TickRate = 60,
                RngSeed = 12345,
                FullHashInterval = 50,
                SnapshotEvery = 100,
                DumpInterval = 100,
                HashAlgoId = HashAlgo.Xxh3,
                InputFormatId = 42,
            }.StampCreatedAt();
        }

        private static void Inputs(ulong tick, byte[] inputs)
        {
            inputs[0] = (byte)(tick / 30);
            inputs[1] = (byte)((tick / 45) & 1);
        }

        /// <summary>Pass 1 with a dump every 100 ticks and one on demand at the planted tick.</summary>
        private void RecordSession(string path, bool diverge)
        {
            var sim = new IntegerSim(12345);
            var inputs = new byte[2];
            using var rec = TickwiseRecorder.Create(path, SessionConfig());
            for (ulong tick = 0; tick < Ticks; tick++)
            {
                Inputs(tick, inputs);
                sim.Step(inputs, diverge && tick >= DivergenceAt);
                rec.RecordTick(tick, inputs, sim);
                if (rec.WantsSnapshot(tick))
                {
                    rec.RecordSnapshot(tick, sim.Snapshot());
                }
                if (tick == DivergenceAt)
                {
                    rec.RecordDump(tick, sim);
                }
            }
        }

        /// <summary>Pass 2: replays a recording with the same simulation, a defect from <paramref name="strikeAt"/> when set.</summary>
        private TickwiseException Replay(string recPath, string dumpPath, ulong[] dumpAt, ulong? strikeAt)
        {
            var sim = new IntegerSim(12345);
            var options = new ReplayOptions
            {
                DumpAtTicks = dumpAt,
                CheckInputFormat = true,
                ExpectedInputFormatId = 42,
            };
            using var replayer = TickwiseReplayer.Open(recPath, options);
            replayer.TickRange(out ulong first, out ulong last);
            Assert.Equal(0UL, first);
            Assert.Equal(Ticks - 1, last);

            var expected = new byte[2];
            TickwiseException failure = null;
            ulong steps = 0;
            while (replayer.TryNextStep(out ulong tick, out ReadOnlySpan<byte> inputs))
            {
                Inputs(tick, expected);
                Assert.True(inputs.SequenceEqual(expected), "the recorded inputs come back per tick");
                sim.Step(inputs, strikeAt.HasValue && tick >= strikeAt.Value);
                try
                {
                    replayer.AfterTick(tick, sim);
                }
                catch (TickwiseException ex)
                {
                    failure = ex;
                    break;
                }
                steps++;
            }
            if (failure == null)
            {
                Assert.Equal(Ticks, steps);
            }
            // A mismatch ends the loop, not the session: the dumps taken so
            // far still reach the file.
            replayer.Finish(dumpPath);
            Assert.True(replayer.IsFinished);
            return failure;
        }

        [Fact]
        public void DumpsRecordedOnTheIntervalReachFieldLevelWithNoReplay()
        {
            string clean = PathFor("clean.rec");
            string chaotic = PathFor("chaotic.rec");
            RecordSession(clean, diverge: false);
            RecordSession(chaotic, diverge: true);

            TickwiseCli.Result inspect = TickwiseCli.Run("inspect", clean);
            Assert.True(inspect.ExitCode == 0, inspect.Stdout + inspect.Stderr);
            Assert.Contains("state dumps    every 100 ticks", inspect.Stdout);
            Assert.Contains("421", inspect.Stdout);

            TickwiseCli.Result compare = TickwiseCli.Run("compare", clean, chaotic);
            Assert.True(compare.ExitCode == 1, compare.Stdout + compare.Stderr);
            Assert.Contains("first divergence at tick 421", compare.Stdout);
            Assert.Contains("--at 421", compare.Stdout);

            TickwiseCli.Result diff = TickwiseCli.Run("diff", clean, chaotic, "--at", "421");
            Assert.True(diff.ExitCode == 1, diff.Stdout + diff.Stderr);
            // The defect adds one to the score and touches nothing else, so
            // the diff names exactly that field.
            Assert.Contains("1 difference over 7 fields", diff.Stdout);
            Assert.Contains("score", diff.Stdout);

            // Before the defect the dumps agree, whichever side is read.
            TickwiseCli.Result before = TickwiseCli.Run("diff", clean, chaotic, "--at", "400");
            Assert.True(before.ExitCode == 0, before.Stdout + before.Stderr);
        }

        [Fact]
        public void AReplayThatReproducesVerifiesEveryHashAndWritesTheDumpsAsked()
        {
            string clean = PathFor("clean.rec");
            string dump = PathFor("clean.dump");
            RecordSession(clean, diverge: false);

            TickwiseException failure = Replay(clean, dump, new ulong[] { 250, DivergenceAt }, strikeAt: null);
            Assert.True(failure == null, failure?.Message);

            TickwiseCli.Result inspect = TickwiseCli.Run("inspect", dump);
            Assert.True(inspect.ExitCode == 0, inspect.Stdout + inspect.Stderr);
            Assert.Contains("250, 421", inspect.Stdout);

            // The replayed dump at 421 equals the one Pass 1 recorded there.
            TickwiseCli.Result same = TickwiseCli.Run("diff", dump, clean, "--at", "421");
            Assert.True(same.ExitCode == 0, same.Stdout + same.Stderr);
        }

        [Fact]
        public void AReplayThatDoesNotReproduceIsCaughtAtItsTickAndKeepsItsDumps()
        {
            string clean = PathFor("clean.rec");
            string dump = PathFor("strike.dump");
            RecordSession(clean, diverge: false);

            TickwiseException failure = Replay(clean, dump, new ulong[] { 100, 150 }, strikeAt: 150);
            Assert.True(failure != null, "expected a hash mismatch");
            Assert.Equal(TickwiseStatus.HashMismatch, failure.Status);
            Assert.Contains("tick 150", failure.Message);

            // The dump at the divergent tick was taken before verification.
            TickwiseCli.Result inspect = TickwiseCli.Run("inspect", dump);
            Assert.True(inspect.ExitCode == 0, inspect.Stdout + inspect.Stderr);
            Assert.Contains("100, 150", inspect.Stdout);
        }

        [Fact]
        public void OpenRefusesWhatItCannotReplay()
        {
            string clean = PathFor("clean.rec");
            RecordSession(clean, diverge: false);

            var wrongFormat = Assert.Throws<TickwiseException>(() => TickwiseReplayer.Open(
                clean, new ReplayOptions { CheckInputFormat = true, ExpectedInputFormatId = 41 }));
            Assert.Equal(TickwiseStatus.InputFormatMismatch, wrongFormat.Status);

            var farTick = Assert.Throws<TickwiseException>(() => TickwiseReplayer.Open(
                clean, new ReplayOptions { DumpAtTicks = new ulong[] { Ticks + 5 } }));
            Assert.Equal(TickwiseStatus.TickOutOfRange, farTick.Status);

            var missing = Assert.Throws<TickwiseException>(() => TickwiseReplayer.Open(
                PathFor("missing.rec"), new ReplayOptions()));
            Assert.Equal(TickwiseStatus.Io, missing.Status);

            Assert.Throws<ArgumentNullException>(() => TickwiseReplayer.Open(null, new ReplayOptions()));
            Assert.Throws<ArgumentNullException>(() => TickwiseReplayer.Open(clean, null));
            Assert.Throws<ArgumentException>(() => TickwiseReplayer.Open("", new ReplayOptions()));
        }

        [Fact]
        public void ProtocolSlipsAndMissingDumpsAreExceptionsNotCrashes()
        {
            string clean = PathFor("clean.rec");
            RecordSession(clean, diverge: false);

            var sim = new IntegerSim(12345);
            using var dump = new TickwiseDump();
            using var replayer = TickwiseReplayer.Open(clean, new ReplayOptions { DumpAtTicks = new ulong[] { 2 } });

            // AfterTick before any step.
            var early = Assert.Throws<TickwiseException>(() => replayer.AfterTick(0, 0, null));
            Assert.Equal(TickwiseStatus.ProtocolMisuse, early.Status);

            // Two honest steps, one through the probe and one by hand.
            Assert.True(replayer.TryNextStep(out ulong tick, out ReadOnlySpan<byte> inputs));
            sim.Step(inputs, false);
            replayer.AfterTick(tick, sim);
            Assert.True(replayer.TryNextStep(out tick, out inputs));
            sim.Step(inputs, false);
            replayer.AfterTick(sim.LightHash(), 0, null);

            // Tick 2 owes a dump: refused without one, the step stays pending,
            // and the retry with a dump goes through.
            Assert.True(replayer.TryNextStep(out tick, out inputs));
            Assert.Equal(2UL, tick);
            Assert.True(replayer.WantsDump(2));
            sim.Step(inputs, false);
            var owed = Assert.Throws<TickwiseException>(() => replayer.AfterTick(sim.LightHash(), 0, null));
            Assert.Equal(TickwiseStatus.MissingDump, owed.Status);
            sim.WriteState(dump);
            Assert.Equal(7, dump.Count);
            replayer.AfterTick(sim.LightHash(), 0, dump);

            // A wrong hash is a mismatch at its tick.
            Assert.True(replayer.TryNextStep(out tick, out inputs));
            sim.Step(inputs, false);
            var wrong = Assert.Throws<TickwiseException>(() => replayer.AfterTick(sim.LightHash() ^ 1, 0, null));
            Assert.Equal(TickwiseStatus.HashMismatch, wrong.Status);
            Assert.Contains("tick 3", wrong.Message);

            // A snapshot restores the simulation and the replay seeks past it.
            Assert.True(replayer.TryNearestSnapshotBefore(250, out ulong snapshotTick, out byte[] snapshot));
            Assert.Equal(200UL, snapshotTick);
            Assert.Equal(44, snapshot.Length);
            sim.Restore(snapshot);
            replayer.SeekTo(201);
            Assert.True(replayer.TryNextStep(out tick, out inputs));
            Assert.Equal(201UL, tick);
            sim.Step(inputs, false);
            // This step is left without its AfterTick on purpose.
            Assert.True(replayer.TryNextStep(out tick, out inputs));
            Assert.Equal(202UL, tick);
            sim.Step(inputs, false);
            replayer.AfterTick(tick, sim);

            // The skipped step is caught at finish, and the session survives.
            var skipped = Assert.Throws<TickwiseException>(() => replayer.Finish(PathFor("misuse.dump")));
            Assert.Equal(TickwiseStatus.ProtocolMisuse, skipped.Status);
            Assert.False(replayer.IsFinished);
            var far = Assert.Throws<TickwiseException>(() => replayer.SeekTo(Ticks + 10));
            Assert.Equal(TickwiseStatus.TickOutOfRange, far.Status);
            Assert.Throws<ArgumentException>(() => replayer.Finish(""));
        }

        [Fact]
        public void ADumpIntervalNeedsAStateWriter()
        {
            var probe = new HashOnlyProbe();
            using var rec = TickwiseRecorder.Create(PathFor("no-writer.rec"), SessionConfig());
            Assert.True(rec.WantsDump(0));
            var ex = Assert.Throws<TickwiseException>(() => rec.RecordTick(0, ReadOnlySpan<byte>.Empty, probe));
            Assert.Equal(TickwiseStatus.InvalidArgument, ex.Status);
            Assert.Contains("ITickwiseStateWriter", ex.Message);

            // The hash form never schedules dumps, so it works with any probe,
            // and a dump built by hand can still be recorded.
            rec.RecordTick(1, ReadOnlySpan<byte>.Empty, probe.LightHash(), 0);
            using var dump = new TickwiseDump();
            dump.SetInt64("x", -1);
            dump.SetInt64("x", -2);
            dump.SetBool("b", true);
            dump.SetFloat("f", 1.5f);
            dump.SetDouble("d", 2.5);
            dump.SetString("s", "text");
            dump.SetBytes("raw", new byte[] { 1, 2, 3 });
            dump.SetNull("none");
            dump.SetLength("list", 0);
            Assert.Equal(8, dump.Count);
            rec.RecordDump(1, dump);
            dump.Clear();
            Assert.Equal(0, dump.Count);
            Assert.Throws<ArgumentException>(() => dump.SetInt64("", 1));
            Assert.Throws<ArgumentNullException>(() => dump.SetString("s", null));
        }

        private sealed class HashOnlyProbe : IDeterminismProbe
        {
            public ulong LightHash() => 1;
            public ulong FullHash() => 2;
        }
    }
}
