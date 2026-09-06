using System;
using System.Runtime.InteropServices;
using System.Text;

namespace Tickwise
{
    /// <summary>
    /// Records inputs and per-tick hashes from your game loop into a .rec file
    /// that <c>tickwise compare</c> understands.
    /// </summary>
    /// <remarks>
    /// The recorder has no dependency on the engine and no opinion about your
    /// loop. Call <see cref="RecordTick(ulong, ReadOnlySpan{byte}, IDeterminismProbe)"/>
    /// once per simulation tick, in order, then <see cref="Finish"/> when the
    /// session ends. Disposing an unfinished recorder finishes it, so a
    /// <c>using</c> block is enough for the common case.
    /// </remarks>
    public sealed class TickwiseRecorder : IDisposable
    {
        private static bool s_compatibilityChecked;
        private static byte s_empty;

        private readonly RecorderHandle _handle;
        private bool _finished;
        private bool _disposed;

        private TickwiseRecorder(RecorderHandle handle)
        {
            _handle = handle;
        }

        /// <summary>True once <see cref="Finish"/> has run.</summary>
        public bool IsFinished => _finished;

        /// <summary>
        /// Creates a recorder writing to a new file at <paramref name="path"/>.
        /// </summary>
        /// <exception cref="TickwiseException">The file could not be created, or the native library is incompatible.</exception>
        public static TickwiseRecorder Create(string path, RecorderConfig config)
        {
            if (path == null)
            {
                throw new ArgumentNullException(nameof(path));
            }
            if (config == null)
            {
                throw new ArgumentNullException(nameof(config));
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
            var strings = new NativeStrings(config.GameId, config.BuildHash, config.Platform);
            try
            {
                var native = new Native.RecorderConfigNative
                {
                    GameId = strings.Pointer(0),
                    GameIdLen = strings.Length(0),
                    BuildHash = strings.Pointer(1),
                    BuildHashLen = strings.Length(1),
                    Platform = strings.Pointer(2),
                    PlatformLen = strings.Length(2),
                    TickRate = config.TickRate,
                    RngSeed = config.RngSeed,
                    CreatedAt = config.CreatedAt,
                    FullHashInterval = config.FullHashInterval,
                    SnapshotEvery = config.SnapshotEvery,
                    HashAlgoId = config.HashAlgoId,
                    InputFormatId = config.InputFormatId,
                };
                TickwiseStatus status = Native.tickwise_recorder_create(
                    pathBytes, (UIntPtr)pathBytes.Length, ref native, out IntPtr pointer);
                TickwiseException.ThrowIfFailed(status, "tickwise_recorder_create");

                var handle = new RecorderHandle();
                handle.Adopt(pointer);
                return new TickwiseRecorder(handle);
            }
            finally
            {
                strings.Dispose();
            }
        }

        /// <summary>
        /// Records one tick: the input bytes and the probe's light hash, plus
        /// its full hash on the ticks where the recorder keeps one. Call
        /// exactly once per tick, in order. The first call may use any tick;
        /// every later call must advance by exactly one.
        /// </summary>
        /// <exception cref="TickwiseException">The tick was out of order, the recorder is finished, or the write failed.</exception>
        public void RecordTick(ulong tick, ReadOnlySpan<byte> inputs, IDeterminismProbe probe)
        {
            if (probe == null)
            {
                throw new ArgumentNullException(nameof(probe));
            }
            ulong light = probe.LightHash();
            ulong full = WantsFullHash(tick) ? probe.FullHash() : 0;
            RecordTick(tick, inputs, light, full);
        }

        /// <summary>
        /// Records one tick with hashes you computed yourself. The full hash
        /// is ignored on ticks where <see cref="WantsFullHash"/> is false, so
        /// pass zero there rather than paying for it.
        /// </summary>
        public void RecordTick(ulong tick, ReadOnlySpan<byte> inputs, ulong lightHash, ulong fullHash)
        {
            ThrowIfUnusable();
            TickwiseStatus status;
            if (inputs.IsEmpty)
            {
                status = Native.tickwise_recorder_record_tick(
                    _handle, tick, ref s_empty, UIntPtr.Zero, lightHash, fullHash);
            }
            else
            {
                status = Native.tickwise_recorder_record_tick(
                    _handle, tick, ref MemoryMarshal.GetReference(inputs),
                    (UIntPtr)inputs.Length, lightHash, fullHash);
            }
            TickwiseException.ThrowIfFailed(status, "tickwise_recorder_record_tick");
        }

        /// <summary>
        /// True when the recorder will keep a full hash at this tick. Use it
        /// to skip computing the expensive hash everywhere else.
        /// </summary>
        public bool WantsFullHash(ulong tick)
        {
            ThrowIfUnusable();
            return Native.tickwise_recorder_wants_full_hash(_handle, tick);
        }

        /// <summary>
        /// True when the snapshot interval asks for a snapshot at this tick.
        /// Tickwise cannot serialize your state, so check this and call
        /// <see cref="RecordSnapshot"/> with your own bytes.
        /// </summary>
        public bool WantsSnapshot(ulong tick)
        {
            ThrowIfUnusable();
            return Native.tickwise_recorder_wants_snapshot(_handle, tick);
        }

        /// <summary>Records a serialized state snapshot at the given tick.</summary>
        public void RecordSnapshot(ulong tick, ReadOnlySpan<byte> data)
        {
            ThrowIfUnusable();
            TickwiseStatus status;
            if (data.IsEmpty)
            {
                status = Native.tickwise_recorder_record_snapshot(_handle, tick, ref s_empty, UIntPtr.Zero);
            }
            else
            {
                status = Native.tickwise_recorder_record_snapshot(
                    _handle, tick, ref MemoryMarshal.GetReference(data), (UIntPtr)data.Length);
            }
            TickwiseException.ThrowIfFailed(status, "tickwise_recorder_record_snapshot");
        }

        /// <summary>
        /// Records a marker you place yourself, for example round start. The
        /// label may be up to 65535 bytes of UTF-8.
        /// </summary>
        public void RecordMarker(ulong tick, string label)
        {
            if (label == null)
            {
                throw new ArgumentNullException(nameof(label));
            }
            ThrowIfUnusable();
            byte[] bytes = Encoding.UTF8.GetBytes(label);
            TickwiseStatus status = Native.tickwise_recorder_record_marker(
                _handle, tick, bytes, (UIntPtr)bytes.Length);
            TickwiseException.ThrowIfFailed(status, "tickwise_recorder_record_marker");
        }

        /// <summary>
        /// Flushes the last hashes, writes the index and trailer, and closes the
        /// file. The recorder accepts no further calls except <see cref="Dispose"/>.
        /// </summary>
        /// <exception cref="TickwiseException">Writing the trailer failed, or the recorder was already finished.</exception>
        public void Finish()
        {
            ThrowIfUnusable();
            TickwiseStatus status = Native.tickwise_recorder_finish(_handle);
            TickwiseException.ThrowIfFailed(status, "tickwise_recorder_finish");
            _finished = true;
        }

        /// <summary>
        /// Finishes the recording if <see cref="Finish"/> has not run, then
        /// releases the native recorder. A failure to finish propagates, so a
        /// recording that could not be closed is never silently lost.
        /// </summary>
        public void Dispose()
        {
            if (_disposed)
            {
                return;
            }
            try
            {
                if (!_finished && !_handle.IsInvalid)
                {
                    Finish();
                }
            }
            finally
            {
                // Marked disposed only after the finish attempt, because
                // Finish refuses to run on a disposed recorder.
                _disposed = true;
                _handle.Dispose();
            }
        }

        private void ThrowIfUnusable()
        {
            if (_disposed)
            {
                throw new ObjectDisposedException(nameof(TickwiseRecorder));
            }
            if (_finished)
            {
                throw new TickwiseException(
                    TickwiseStatus.AlreadyFinished,
                    "the recorder is finished; create a new one for another session");
            }
        }

        /// <summary>
        /// UTF-8 copies of the metadata strings in unmanaged memory for the
        /// duration of the create call. Config structs cannot point into
        /// managed arrays, and this path runs once per session.
        /// </summary>
        private sealed class NativeStrings : IDisposable
        {
            private readonly IntPtr[] _pointers;
            private readonly int[] _lengths;

            public NativeStrings(params string[] values)
            {
                _pointers = new IntPtr[values.Length];
                _lengths = new int[values.Length];
                for (int i = 0; i < values.Length; i++)
                {
                    byte[] bytes = Encoding.UTF8.GetBytes(values[i] ?? string.Empty);
                    _lengths[i] = bytes.Length;
                    if (bytes.Length == 0)
                    {
                        continue;
                    }
                    _pointers[i] = Marshal.AllocHGlobal(bytes.Length);
                    Marshal.Copy(bytes, 0, _pointers[i], bytes.Length);
                }
            }

            public IntPtr Pointer(int index) => _pointers[index];

            public UIntPtr Length(int index) => (UIntPtr)_lengths[index];

            public void Dispose()
            {
                for (int i = 0; i < _pointers.Length; i++)
                {
                    if (_pointers[i] != IntPtr.Zero)
                    {
                        Marshal.FreeHGlobal(_pointers[i]);
                        _pointers[i] = IntPtr.Zero;
                    }
                }
            }
        }
    }
}
