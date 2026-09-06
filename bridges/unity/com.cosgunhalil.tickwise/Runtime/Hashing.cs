using System;
using System.Runtime.InteropServices;

namespace Tickwise
{
    /// <summary>
    /// Identifiers for <see cref="RecorderConfig.HashAlgoId"/>. They tell the
    /// tools which algorithm produced the hashes in a recording.
    /// </summary>
    public static class HashAlgo
    {
        /// <summary>A hash of your own. Nothing is assumed about it.</summary>
        public const ushort UserDefined = 0;

        /// <summary>xxh3 64 bit, what <see cref="Xxh3.Hash64"/> computes.</summary>
        public const ushort Xxh3 = 1;

        /// <summary>blake3 truncated to 64 bits. Not available from this package yet.</summary>
        public const ushort Blake3 = 2;
    }

    /// <summary>
    /// The xxh3 64 bit hash, computed by the native library so a Unity client
    /// and a Rust client hashing the same bytes get the same value.
    /// </summary>
    public static class Xxh3
    {
        private static byte s_empty;

        /// <summary>Hashes the bytes. An empty span hashes the empty input.</summary>
        public static ulong Hash64(ReadOnlySpan<byte> data)
        {
            if (data.IsEmpty)
            {
                return Native.tickwise_xxh3_64(ref s_empty, UIntPtr.Zero);
            }
            return Native.tickwise_xxh3_64(
                ref MemoryMarshal.GetReference(data),
                (UIntPtr)data.Length);
        }

        /// <summary>Hashes a byte array.</summary>
        public static ulong Hash64(byte[] data)
        {
            if (data == null)
            {
                throw new ArgumentNullException(nameof(data));
            }
            return Hash64(new ReadOnlySpan<byte>(data));
        }
    }
}
