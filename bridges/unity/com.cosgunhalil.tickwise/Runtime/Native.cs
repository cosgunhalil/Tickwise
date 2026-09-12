using System;
using System.Runtime.InteropServices;

namespace Tickwise
{
    /// <summary>
    /// The raw C surface of tickwise_ffi, one declaration per exported
    /// function. Everything here mirrors include/tickwise.h and nothing
    /// else in the package talks to the native library directly.
    /// </summary>
    /// <remarks>
    /// Strings and byte buffers cross the boundary as a pointer plus a
    /// length. Booleans cross as one byte, which is why every bool return
    /// and parameter carries an explicit I1 marshalling attribute: the
    /// default would read four bytes and pick up garbage.
    /// </remarks>
    internal static class Native
    {
#if UNITY_IOS && !UNITY_EDITOR
        // iOS links the static library into the app binary.
        private const string Library = "__Internal";
#else
        private const string Library = "tickwise_ffi";
#endif

        /// <summary>Recorder configuration with the exact layout of the C struct.</summary>
        [StructLayout(LayoutKind.Sequential)]
        internal struct RecorderConfigNative
        {
            public IntPtr GameId;
            public UIntPtr GameIdLen;
            public IntPtr BuildHash;
            public UIntPtr BuildHashLen;
            public IntPtr Platform;
            public UIntPtr PlatformLen;
            public uint TickRate;
            public ulong RngSeed;
            public ulong CreatedAt;
            public uint FullHashInterval;
            public uint SnapshotEvery;
            public ushort HashAlgoId;
            public ulong InputFormatId;
            public uint DumpInterval;
        }

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern uint tickwise_ffi_abi_version();

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr tickwise_ffi_version();

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr tickwise_last_error_message();

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr tickwise_status_name(TickwiseStatus status);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern ulong tickwise_xxh3_64(ref byte data, UIntPtr len);

        // The recorder, Pass 1.

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern TickwiseStatus tickwise_recorder_config_default(
            out RecorderConfigNative config);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern TickwiseStatus tickwise_recorder_create(
            byte[] path,
            UIntPtr pathLen,
            ref RecorderConfigNative config,
            out IntPtr recorder);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern TickwiseStatus tickwise_recorder_record_tick(
            RecorderHandle recorder,
            ulong tick,
            ref byte inputs,
            UIntPtr inputsLen,
            ulong lightHash,
            ulong fullHash);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool tickwise_recorder_wants_full_hash(RecorderHandle recorder, ulong tick);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool tickwise_recorder_wants_snapshot(RecorderHandle recorder, ulong tick);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool tickwise_recorder_wants_dump(RecorderHandle recorder, ulong tick);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern TickwiseStatus tickwise_recorder_record_dump(
            RecorderHandle recorder,
            ulong tick,
            DumpHandle dump);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern TickwiseStatus tickwise_recorder_record_snapshot(
            RecorderHandle recorder,
            ulong tick,
            ref byte data,
            UIntPtr dataLen);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern TickwiseStatus tickwise_recorder_record_marker(
            RecorderHandle recorder,
            ulong tick,
            byte[] label,
            UIntPtr labelLen);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern TickwiseStatus tickwise_recorder_finish(RecorderHandle recorder);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void tickwise_recorder_destroy(IntPtr recorder);

        // The dump builder.

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr tickwise_dump_new();

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void tickwise_dump_destroy(IntPtr dump);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern TickwiseStatus tickwise_dump_clear(DumpHandle dump);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr tickwise_dump_len(DumpHandle dump);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern TickwiseStatus tickwise_dump_set_null(
            DumpHandle dump, byte[] path, UIntPtr pathLen);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern TickwiseStatus tickwise_dump_set_bool(
            DumpHandle dump, byte[] path, UIntPtr pathLen, [MarshalAs(UnmanagedType.I1)] bool value);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern TickwiseStatus tickwise_dump_set_i64(
            DumpHandle dump, byte[] path, UIntPtr pathLen, long value);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern TickwiseStatus tickwise_dump_set_u64(
            DumpHandle dump, byte[] path, UIntPtr pathLen, ulong value);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern TickwiseStatus tickwise_dump_set_f32(
            DumpHandle dump, byte[] path, UIntPtr pathLen, float value);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern TickwiseStatus tickwise_dump_set_f64(
            DumpHandle dump, byte[] path, UIntPtr pathLen, double value);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern TickwiseStatus tickwise_dump_set_str(
            DumpHandle dump, byte[] path, UIntPtr pathLen, byte[] value, UIntPtr valueLen);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern TickwiseStatus tickwise_dump_set_bytes(
            DumpHandle dump, byte[] path, UIntPtr pathLen, ref byte value, UIntPtr valueLen);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern TickwiseStatus tickwise_dump_set_len(
            DumpHandle dump, byte[] path, UIntPtr pathLen, ulong count);

        // The replayer, Pass 2.

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern TickwiseStatus tickwise_replayer_open(
            byte[] path,
            UIntPtr pathLen,
            ulong[] dumpAtTicks,
            UIntPtr dumpCount,
            [MarshalAs(UnmanagedType.I1)] bool verifyHashes,
            [MarshalAs(UnmanagedType.I1)] bool checkInputFormat,
            ulong expectedInputFormatId,
            out IntPtr replayer);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern TickwiseStatus tickwise_replayer_tick_range(
            ReplayerHandle replayer, out ulong first, out ulong last);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool tickwise_replayer_next_step(
            ReplayerHandle replayer, out ulong tick, out IntPtr inputs, out UIntPtr inputsLen);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool tickwise_replayer_wants_dump(ReplayerHandle replayer, ulong tick);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool tickwise_replayer_wants_full_hash(ReplayerHandle replayer, ulong tick);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern TickwiseStatus tickwise_replayer_after_tick(
            ReplayerHandle replayer, ulong lightHash, ulong fullHash, DumpHandle dump);

        /// <summary>The same entry point with no dump, since a SafeHandle argument cannot be null.</summary>
        [DllImport(Library, CallingConvention = CallingConvention.Cdecl, EntryPoint = "tickwise_replayer_after_tick")]
        internal static extern TickwiseStatus tickwise_replayer_after_tick_without_dump(
            ReplayerHandle replayer, ulong lightHash, ulong fullHash, IntPtr dump);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool tickwise_replayer_nearest_snapshot_before(
            ReplayerHandle replayer, ulong tick, out ulong snapshotTick, out IntPtr data, out UIntPtr dataLen);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern TickwiseStatus tickwise_replayer_seek_to(ReplayerHandle replayer, ulong tick);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern TickwiseStatus tickwise_replayer_finish(
            ReplayerHandle replayer, byte[] path, UIntPtr pathLen);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void tickwise_replayer_destroy(IntPtr replayer);

        /// <summary>
        /// Reads a NUL-terminated UTF-8 string owned by the native library.
        /// Written by hand rather than through Marshal.PtrToStringUTF8 so it
        /// behaves the same under Mono, IL2CPP, and .NET.
        /// </summary>
        internal static string ReadUtf8(IntPtr ptr)
        {
            if (ptr == IntPtr.Zero)
            {
                return string.Empty;
            }
            int length = 0;
            while (Marshal.ReadByte(ptr, length) != 0)
            {
                length++;
            }
            if (length == 0)
            {
                return string.Empty;
            }
            var bytes = new byte[length];
            Marshal.Copy(ptr, bytes, 0, length);
            return System.Text.Encoding.UTF8.GetString(bytes);
        }

        /// <summary>The message of the last failed native call on this thread.</summary>
        internal static string LastErrorMessage()
        {
            return ReadUtf8(tickwise_last_error_message());
        }
    }

    /// <summary>
    /// Owns a native recorder pointer and destroys it exactly once, whether
    /// through Dispose or the finalizer.
    /// </summary>
    internal sealed class RecorderHandle : SafeHandle
    {
        public RecorderHandle() : base(IntPtr.Zero, ownsHandle: true)
        {
        }

        public override bool IsInvalid => handle == IntPtr.Zero;

        internal void Adopt(IntPtr pointer)
        {
            SetHandle(pointer);
        }

        protected override bool ReleaseHandle()
        {
            Native.tickwise_recorder_destroy(handle);
            return true;
        }
    }

    /// <summary>Owns a native dump builder and destroys it exactly once.</summary>
    internal sealed class DumpHandle : SafeHandle
    {
        public DumpHandle() : base(IntPtr.Zero, ownsHandle: true)
        {
        }

        public override bool IsInvalid => handle == IntPtr.Zero;

        internal void Adopt(IntPtr pointer)
        {
            SetHandle(pointer);
        }

        protected override bool ReleaseHandle()
        {
            Native.tickwise_dump_destroy(handle);
            return true;
        }
    }

    /// <summary>Owns a native replayer and destroys it exactly once.</summary>
    internal sealed class ReplayerHandle : SafeHandle
    {
        public ReplayerHandle() : base(IntPtr.Zero, ownsHandle: true)
        {
        }

        public override bool IsInvalid => handle == IntPtr.Zero;

        internal void Adopt(IntPtr pointer)
        {
            SetHandle(pointer);
        }

        protected override bool ReleaseHandle()
        {
            Native.tickwise_replayer_destroy(handle);
            return true;
        }
    }
}
