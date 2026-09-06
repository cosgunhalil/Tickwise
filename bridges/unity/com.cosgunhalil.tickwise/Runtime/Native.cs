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
    /// carries an explicit I1 marshalling attribute: the default would read
    /// four bytes and pick up garbage.
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
}
