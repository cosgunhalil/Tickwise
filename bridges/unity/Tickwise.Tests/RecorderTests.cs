using System;
using System.IO;
using Xunit;

namespace Tickwise.Tests
{
    public class RecorderTests : IDisposable
    {
        private const ulong Ticks = 600;
        private const ulong DivergenceAt = 421;

        private readonly string _dir;

        public RecorderTests()
        {
            _dir = Path.Combine(Path.GetTempPath(), "tickwise-unity-tests-" + Guid.NewGuid().ToString("N"));
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
                GameId = "dotnet-tests",
                BuildHash = "test-build",
                Platform = "dotnet",
                TickRate = 60,
                RngSeed = 12345,
                FullHashInterval = 50,
                SnapshotEvery = 100,
                HashAlgoId = HashAlgo.Xxh3,
                InputFormatId = 42,
            }.StampCreatedAt();
        }

        /// <summary>Records a full session through the public API, finishing via Dispose.</summary>
        private void RecordSession(string path, bool diverge)
        {
            var sim = new IntegerSim(12345);
            var inputs = new byte[2];
            using var rec = TickwiseRecorder.Create(path, SessionConfig());
            for (ulong tick = 0; tick < Ticks; tick++)
            {
                inputs[0] = (byte)(tick / 30);
                inputs[1] = (byte)((tick / 45) & 1);
                sim.Step(inputs, diverge && tick >= DivergenceAt);
                rec.RecordTick(tick, inputs, sim);
                if (rec.WantsSnapshot(tick))
                {
                    rec.RecordSnapshot(tick, sim.Snapshot());
                }
                if (tick == 300)
                {
                    rec.RecordMarker(tick, "round start");
                }
            }
        }

        [Fact]
        public void NativeLibraryIsCompatible()
        {
            TickwiseNative.EnsureCompatible();
            Assert.Equal(TickwiseNative.ExpectedAbiVersion, TickwiseNative.AbiVersion);
            Assert.Matches(@"^\d+\.\d+\.\d+", TickwiseNative.Version);
        }

        [Fact]
        public void Xxh3MatchesTheReferenceDigestOfTheEmptyInput()
        {
            const ulong emptyDigest = 0x2D06800538D394C2UL;
            Assert.Equal(emptyDigest, Xxh3.Hash64(ReadOnlySpan<byte>.Empty));
            Assert.Equal(emptyDigest, Xxh3.Hash64(Array.Empty<byte>()));
            Assert.NotEqual(emptyDigest, Xxh3.Hash64(new byte[] { 1 }));
            Assert.Equal(Xxh3.Hash64(new byte[] { 1, 2, 3 }), Xxh3.Hash64(new byte[] { 1, 2, 3 }));
            Assert.Throws<ArgumentNullException>(() => Xxh3.Hash64((byte[])null));
        }

        [Fact]
        public void RecordedSessionIsReadableByTheCli()
        {
            string clean = PathFor("clean.rec");
            RecordSession(clean, diverge: false);

            TickwiseCli.Result inspect = TickwiseCli.Run("inspect", clean);
            Assert.True(inspect.ExitCode == 0, inspect.Stdout + inspect.Stderr);
            Assert.Contains("dotnet-tests", inspect.Stdout);
            Assert.Contains("test-build", inspect.Stdout);
            Assert.Contains("600", inspect.Stdout);
            Assert.Contains("every 50 ticks", inspect.Stdout);
            Assert.Contains("id 42", inspect.Stdout);
            Assert.Contains("checksum ok", inspect.Stdout);
        }

        [Fact]
        public void CompareFindsTheInjectedDivergence()
        {
            string clean = PathFor("clean.rec");
            string chaotic = PathFor("chaotic.rec");
            RecordSession(clean, diverge: false);
            RecordSession(chaotic, diverge: true);

            TickwiseCli.Result compare = TickwiseCli.Run("compare", clean, chaotic);
            Assert.True(compare.ExitCode == 1, "expected exit code 1, got " + compare.ExitCode + "\n" + compare.Stdout + compare.Stderr);
            Assert.Contains("first divergence at tick 421", compare.Stdout);
            Assert.Contains("last agreement at tick 420", compare.Stdout);

            TickwiseCli.Result same = TickwiseCli.Run("compare", clean, clean);
            Assert.True(same.ExitCode == 0, same.Stdout + same.Stderr);
            Assert.Contains("identical", same.Stdout);
        }

        [Fact]
        public void WantsFullHashFollowsTheInterval()
        {
            using var rec = TickwiseRecorder.Create(PathFor("interval.rec"), SessionConfig());
            Assert.True(rec.WantsFullHash(0));
            Assert.False(rec.WantsFullHash(1));
            Assert.True(rec.WantsFullHash(50));
            Assert.True(rec.WantsSnapshot(100));
            Assert.False(rec.WantsSnapshot(101));
        }

        [Fact]
        public void ProbeFullHashIsOnlyAskedForWhenTheRecorderKeepsIt()
        {
            var probe = new CountingProbe();
            using var rec = TickwiseRecorder.Create(PathFor("counting.rec"), SessionConfig());
            for (ulong tick = 0; tick < 150; tick++)
            {
                rec.RecordTick(tick, ReadOnlySpan<byte>.Empty, probe);
            }
            Assert.Equal(150, probe.LightCalls);
            Assert.Equal(3, probe.FullCalls);
        }

        [Fact]
        public void OutOfOrderTicksAreRejected()
        {
            using var rec = TickwiseRecorder.Create(PathFor("order.rec"), SessionConfig());
            rec.RecordTick(0, ReadOnlySpan<byte>.Empty, 1, 0);
            rec.RecordTick(1, ReadOnlySpan<byte>.Empty, 1, 0);
            var ex = Assert.Throws<TickwiseException>(() => rec.RecordTick(5, ReadOnlySpan<byte>.Empty, 1, 0));
            Assert.Equal(TickwiseStatus.NonSequentialTick, ex.Status);
            Assert.Contains("expected 2, got 5", ex.Message);
        }

        [Fact]
        public void OversizedMarkerLabelIsRejected()
        {
            using var rec = TickwiseRecorder.Create(PathFor("marker.rec"), SessionConfig());
            var ex = Assert.Throws<TickwiseException>(() => rec.RecordMarker(0, new string('x', 70_000)));
            Assert.Equal(TickwiseStatus.InvalidArgument, ex.Status);
            rec.RecordMarker(0, "fits");
        }

        [Fact]
        public void FinishIsFinal()
        {
            var rec = TickwiseRecorder.Create(PathFor("finish.rec"), SessionConfig());
            rec.RecordTick(0, ReadOnlySpan<byte>.Empty, 1, 0);
            Assert.False(rec.IsFinished);
            rec.Finish();
            Assert.True(rec.IsFinished);

            var again = Assert.Throws<TickwiseException>(() => rec.Finish());
            Assert.Equal(TickwiseStatus.AlreadyFinished, again.Status);
            var record = Assert.Throws<TickwiseException>(() => rec.RecordTick(1, ReadOnlySpan<byte>.Empty, 1, 0));
            Assert.Equal(TickwiseStatus.AlreadyFinished, record.Status);

            rec.Dispose();
            rec.Dispose();
            Assert.Throws<ObjectDisposedException>(() => rec.WantsFullHash(0));
        }

        [Fact]
        public void DisposeFinishesAnOpenRecording()
        {
            string path = PathFor("disposed.rec");
            using (var rec = TickwiseRecorder.Create(path, SessionConfig()))
            {
                rec.RecordTick(0, new byte[] { 1, 2 }, 1, 0);
            }
            TickwiseCli.Result inspect = TickwiseCli.Run("inspect", path);
            Assert.True(inspect.ExitCode == 0, inspect.Stdout + inspect.Stderr);
            Assert.Contains("checksum ok", inspect.Stdout);
        }

        [Fact]
        public void CreateReportsIoFailures()
        {
            string missing = Path.Combine(_dir, "definitely", "not", "a", "dir", "x.rec");
            var ex = Assert.Throws<TickwiseException>(() => TickwiseRecorder.Create(missing, SessionConfig()));
            Assert.Equal(TickwiseStatus.Io, ex.Status);
        }

        [Fact]
        public void ArgumentsAreValidatedBeforeTheNativeCall()
        {
            Assert.Throws<ArgumentNullException>(() => TickwiseRecorder.Create(null, SessionConfig()));
            Assert.Throws<ArgumentNullException>(() => TickwiseRecorder.Create(PathFor("x.rec"), null));
            Assert.Throws<ArgumentException>(() => TickwiseRecorder.Create("", SessionConfig()));
            using var rec = TickwiseRecorder.Create(PathFor("args.rec"), SessionConfig());
            Assert.Throws<ArgumentNullException>(() => rec.RecordTick(0, ReadOnlySpan<byte>.Empty, null));
            Assert.Throws<ArgumentNullException>(() => rec.RecordMarker(0, null));
        }

        [Fact]
        public void MetadataStringsSurviveTheRoundTrip()
        {
            string path = PathFor("meta.rec");
            // Turkish letters, written as escapes so the expectation does not
            // depend on the source file's code page.
            const string gameId = "Ka\u00e7\u0131\u015f Oyunu";
            var config = SessionConfig();
            config.GameId = gameId;
            config.Platform = "windows-x86_64";
            using (var rec = TickwiseRecorder.Create(path, config))
            {
                rec.RecordTick(0, ReadOnlySpan<byte>.Empty, 1, 0);
            }
            TickwiseCli.Result inspect = TickwiseCli.Run("inspect", path);
            Assert.True(inspect.ExitCode == 0, inspect.Stdout + inspect.Stderr);
            Assert.Contains(gameId, inspect.Stdout);
            Assert.Contains("windows-x86_64", inspect.Stdout);
        }

        private sealed class CountingProbe : IDeterminismProbe
        {
            public int LightCalls;
            public int FullCalls;

            public ulong LightHash()
            {
                LightCalls++;
                return 1;
            }

            public ulong FullHash()
            {
                FullCalls++;
                return 2;
            }
        }
    }
}
