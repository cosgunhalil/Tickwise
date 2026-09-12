using System;
using System.Runtime.InteropServices;
using System.Text;

namespace Tickwise
{
    /// <summary>
    /// Pass 2: steps through a recording's inputs while your simulation
    /// replays them, verifies the live hashes against the recorded ones, and
    /// collects state dumps at the ticks you name into a .dump file for
    /// <c>tickwise diff</c>.
    /// </summary>
    /// <remarks>
    /// The replayer never runs your simulation. The loop is yours:
    /// <code>
    /// using var replayer = TickwiseReplayer.Open(path, new ReplayOptions { DumpAtTicks = new ulong[] { 421 } });
    /// while (replayer.TryNextStep(out ulong tick, out ReadOnlySpan&lt;byte&gt; inputs))
    /// {
    ///     sim.Step(inputs);
    ///     replayer.AfterTick(tick, sim);
    /// }
    /// replayer.Finish("a.dump");
    /// </code>
    /// A <see cref="TickwiseStatus.HashMismatch"/> from <c>AfterTick</c> means
    /// the replay is not reproducing the recording. Disposing without
    /// <see cref="Finish"/> discards the collected dumps.
    /// </remarks>
    public sealed class TickwiseReplayer : IDisposable
    {
        private static bool s_compatibilityChecked;

        private readonly ReplayerHandle _handle;
        private byte[] _inputs = new byte[64];
        private TickwiseDump _scratch;
        private bool _finished;
        private bool _disposed;

        private TickwiseReplayer(ReplayerHandle handle)
        {
            _handle = handle;
        }

        /// <summary>True once <see cref="Finish"/> has run.</summary>
        public bool IsFinished => _finished;

        /// <summary>
        /// Opens a recording for replay.
        /// </summary>
        /// <exception cref="TickwiseException">
        /// <see cref="TickwiseStatus.Io"/> when the file cannot be read,
        /// <see cref="TickwiseStatus.InputFormatMismatch"/> when the format check fails,
        /// <see cref="TickwiseStatus.TickOutOfRange"/> when a dump tick lies outside the recording,
        /// <see cref="TickwiseStatus.EmptyRecording"/> when it holds no ticks.
        /// </exception>
        public static TickwiseReplayer Open(string path, ReplayOptions options)
        {
            if (path == null)
            {
                throw new ArgumentNullException(nameof(path));
            }
            if (options == null)
            {
                throw new ArgumentNullException(nameof(options));
            }
            if (path.Length == 0)
            {
                throw new ArgumentException("path is empty", nameof(path));
            }
            if (!s_compatibilityChecked)
            {
                TickwiseNative.EnsureCompatible();
                s_compatibilityChecked = true;
            }

            byte[] pathBytes = Encoding.UTF8.GetBytes(path);
            ulong[] dumpAt = options.DumpAtTicks ?? Array.Empty<ulong>();
            TickwiseStatus status = Native.tickwise_replayer_open(
                pathBytes,
                (UIntPtr)pathBytes.Length,
                dumpAt.Length == 0 ? null : dumpAt,
                (UIntPtr)dumpAt.Length,
                options.VerifyHashes,
                options.CheckInputFormat,
                options.ExpectedInputFormatId,
                out IntPtr pointer);
            TickwiseException.ThrowIfFailed(status, "tickwise_replayer_open");

            var handle = new ReplayerHandle();
            handle.Adopt(pointer);
            return new TickwiseReplayer(handle);
        }

        /// <summary>The first and last tick the recording covers.</summary>
        public void TickRange(out ulong first, out ulong last)
        {
            ThrowIfUnusable();
            TickwiseStatus status = Native.tickwise_replayer_tick_range(_handle, out first, out last);
            TickwiseException.ThrowIfFailed(status, "tickwise_replayer_tick_range");
        }

        /// <summary>
        /// Yields the next tick and its recorded inputs, or returns false when
        /// the recording is exhausted. The inputs are valid until the next
        /// call. Call <c>AfterTick</c> exactly once per step.
        /// </summary>
        public bool TryNextStep(out ulong tick, out ReadOnlySpan<byte> inputs)
        {
            ThrowIfUnusable();
            if (!Native.tickwise_replayer_next_step(_handle, out tick, out IntPtr data, out UIntPtr len))
            {
                inputs = ReadOnlySpan<byte>.Empty;
                return false;
            }
            int count = (int)(ulong)len;
            if (count > _inputs.Length)
            {
                _inputs = new byte[Math.Max(count, _inputs.Length * 2)];
            }
            if (count > 0)
            {
                Marshal.Copy(data, _inputs, 0, count);
            }
            inputs = new ReadOnlySpan<byte>(_inputs, 0, count);
            return true;
        }

        /// <summary>True when the replay wants a state dump at this tick.</summary>
        public bool WantsDump(ulong tick)
        {
            ThrowIfUnusable();
            return Native.tickwise_replayer_wants_dump(_handle, tick);
        }

        /// <summary>True when the recording holds a full hash at this tick and hashes are being verified.</summary>
        public bool WantsFullHash(ulong tick)
        {
            ThrowIfUnusable();
            return Native.tickwise_replayer_wants_full_hash(_handle, tick);
        }

        /// <summary>
        /// Completes the current step from the probe, asking it only for what
        /// this tick needs: the light hash, the full hash where the recording
        /// has one, and the state dump on the ticks named in the options.
        /// </summary>
        /// <exception cref="TickwiseException">
        /// <see cref="TickwiseStatus.HashMismatch"/> when the replay diverged from the recording at this tick;
        /// <see cref="TickwiseStatus.InvalidArgument"/> when a dump is due and the probe does not implement <see cref="ITickwiseStateWriter"/>.
        /// </exception>
        public void AfterTick(ulong tick, IDeterminismProbe probe)
        {
            if (probe == null)
            {
                throw new ArgumentNullException(nameof(probe));
            }
            ThrowIfUnusable();
            ulong light = probe.LightHash();
            ulong full = WantsFullHash(tick) ? probe.FullHash() : 0;
            TickwiseDump dump = null;
            if (WantsDump(tick))
            {
                ITickwiseStateWriter writer = TickwiseRecorder.StateWriterOf(probe);
                if (_scratch == null)
                {
                    _scratch = new TickwiseDump();
                }
                _scratch.Clear();
                writer.WriteState(_scratch);
                dump = _scratch;
            }
            AfterTick(light, full, dump);
        }

        /// <summary>
        /// Completes the current step with hashes you computed yourself.
        /// <paramref name="dump"/> may be null except on ticks where
        /// <see cref="WantsDump"/> is true; there, a missing dump fails with
        /// <see cref="TickwiseStatus.MissingDump"/> and leaves the step
        /// pending, so the call can be repeated with one.
        /// </summary>
        public void AfterTick(ulong lightHash, ulong fullHash, TickwiseDump dump)
        {
            ThrowIfUnusable();
            TickwiseStatus status = dump == null
                ? Native.tickwise_replayer_after_tick_without_dump(_handle, lightHash, fullHash, IntPtr.Zero)
                : Native.tickwise_replayer_after_tick(_handle, lightHash, fullHash, dump.Handle);
            TickwiseException.ThrowIfFailed(status, "tickwise_replayer_after_tick");
        }

        /// <summary>
        /// The latest snapshot at or before <paramref name="tick"/>, as a copy
        /// of the bytes your recorder stored. Restore your state from it, then
        /// <see cref="SeekTo"/> the tick after it. Returns false when the
        /// recording holds no snapshot that early.
        /// </summary>
        public bool TryNearestSnapshotBefore(ulong tick, out ulong snapshotTick, out byte[] data)
        {
            ThrowIfUnusable();
            if (!Native.tickwise_replayer_nearest_snapshot_before(
                    _handle, tick, out snapshotTick, out IntPtr pointer, out UIntPtr len))
            {
                data = null;
                return false;
            }
            data = new byte[(int)(ulong)len];
            if (data.Length > 0)
            {
                Marshal.Copy(pointer, data, 0, data.Length);
            }
            return true;
        }

        /// <summary>
        /// Positions the replay so the next step is <paramref name="tick"/>,
        /// after you restored state from a snapshot taken at the tick before.
        /// </summary>
        public void SeekTo(ulong tick)
        {
            ThrowIfUnusable();
            TickwiseException.ThrowIfFailed(Native.tickwise_replayer_seek_to(_handle, tick), "tickwise_replayer_seek_to");
        }

        /// <summary>
        /// Writes the collected dumps as a .dump file at <paramref name="path"/>
        /// and ends the session. Fails with <see cref="TickwiseStatus.ProtocolMisuse"/>
        /// when a step was left without its <c>AfterTick</c>, and the session
        /// stays open so the step can be completed.
        /// </summary>
        public void Finish(string path)
        {
            if (path == null)
            {
                throw new ArgumentNullException(nameof(path));
            }
            if (path.Length == 0)
            {
                throw new ArgumentException("path is empty", nameof(path));
            }
            ThrowIfUnusable();
            byte[] bytes = Encoding.UTF8.GetBytes(path);
            TickwiseStatus status = Native.tickwise_replayer_finish(_handle, bytes, (UIntPtr)bytes.Length);
            TickwiseException.ThrowIfFailed(status, "tickwise_replayer_finish");
            _finished = true;
        }

        /// <summary>Releases the native replayer. Dumps not written by <see cref="Finish"/> are discarded.</summary>
        public void Dispose()
        {
            if (_disposed)
            {
                return;
            }
            _disposed = true;
            _scratch?.Dispose();
            _scratch = null;
            _handle.Dispose();
        }

        private void ThrowIfUnusable()
        {
            if (_disposed)
            {
                throw new ObjectDisposedException(nameof(TickwiseReplayer));
            }
            if (_finished)
            {
                throw new TickwiseException(
                    TickwiseStatus.AlreadyFinished,
                    "the replayer is finished; open the recording again for another pass");
            }
        }
    }
}
