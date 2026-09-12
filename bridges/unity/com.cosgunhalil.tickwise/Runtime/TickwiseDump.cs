using System;
using System.Runtime.InteropServices;
using System.Text;

namespace Tickwise
{
    /// <summary>
    /// A state dump under construction: a flat list of path and value pairs
    /// that <c>tickwise diff</c> walks field by field. Your
    /// <see cref="ITickwiseStateWriter"/> fills one on the ticks a recorder
    /// or replayer asks for it.
    /// </summary>
    /// <remarks>
    /// Paths are dotted, with brackets for indices, the same shape every
    /// probe in the repository produces: <c>players[2].velocity.x</c>. Set a
    /// path twice and the second value wins. Give every collection its
    /// length under the collection's own path with <see cref="SetLength"/>,
    /// so a shorter list never hides behind a matching tail. Reuse one
    /// instance across ticks; <see cref="Clear"/> keeps the native buffer.
    /// </remarks>
    public sealed class TickwiseDump : IDisposable
    {
        private static byte s_empty;

        private readonly DumpHandle _handle;
        private bool _disposed;

        /// <summary>Creates an empty dump builder.</summary>
        public TickwiseDump()
        {
            IntPtr pointer = Native.tickwise_dump_new();
            if (pointer == IntPtr.Zero)
            {
                throw new TickwiseException(TickwiseStatus.Panic, "tickwise_dump_new returned null");
            }
            _handle = new DumpHandle();
            _handle.Adopt(pointer);
        }

        internal DumpHandle Handle
        {
            get
            {
                ThrowIfDisposed();
                return _handle;
            }
        }

        /// <summary>Number of distinct paths set so far.</summary>
        public int Count
        {
            get
            {
                ThrowIfDisposed();
                return (int)(ulong)Native.tickwise_dump_len(_handle);
            }
        }

        /// <summary>Removes every entry, keeping the builder for the next tick.</summary>
        public void Clear()
        {
            ThrowIfDisposed();
            TickwiseException.ThrowIfFailed(Native.tickwise_dump_clear(_handle), "tickwise_dump_clear");
        }

        /// <summary>Records an absent value, for an optional that is unset.</summary>
        public void SetNull(string path)
        {
            byte[] p = PathBytes(path);
            Check(Native.tickwise_dump_set_null(Handle, p, Len(p)), "tickwise_dump_set_null");
        }

        public void SetBool(string path, bool value)
        {
            byte[] p = PathBytes(path);
            Check(Native.tickwise_dump_set_bool(Handle, p, Len(p), value), "tickwise_dump_set_bool");
        }

        public void SetInt64(string path, long value)
        {
            byte[] p = PathBytes(path);
            Check(Native.tickwise_dump_set_i64(Handle, p, Len(p), value), "tickwise_dump_set_i64");
        }

        public void SetUInt64(string path, ulong value)
        {
            byte[] p = PathBytes(path);
            Check(Native.tickwise_dump_set_u64(Handle, p, Len(p), value), "tickwise_dump_set_u64");
        }

        /// <summary>Records a float by bit pattern; the diff classifies sub-epsilon drift on its own.</summary>
        public void SetFloat(string path, float value)
        {
            byte[] p = PathBytes(path);
            Check(Native.tickwise_dump_set_f32(Handle, p, Len(p), value), "tickwise_dump_set_f32");
        }

        public void SetDouble(string path, double value)
        {
            byte[] p = PathBytes(path);
            Check(Native.tickwise_dump_set_f64(Handle, p, Len(p), value), "tickwise_dump_set_f64");
        }

        public void SetString(string path, string value)
        {
            if (value == null)
            {
                throw new ArgumentNullException(nameof(value));
            }
            byte[] p = PathBytes(path);
            byte[] v = Encoding.UTF8.GetBytes(value);
            Check(Native.tickwise_dump_set_str(Handle, p, Len(p), v, Len(v)), "tickwise_dump_set_str");
        }

        public void SetBytes(string path, ReadOnlySpan<byte> value)
        {
            byte[] p = PathBytes(path);
            TickwiseStatus status;
            if (value.IsEmpty)
            {
                status = Native.tickwise_dump_set_bytes(Handle, p, Len(p), ref s_empty, UIntPtr.Zero);
            }
            else
            {
                status = Native.tickwise_dump_set_bytes(
                    Handle, p, Len(p), ref MemoryMarshal.GetReference(value), (UIntPtr)value.Length);
            }
            Check(status, "tickwise_dump_set_bytes");
        }

        /// <summary>
        /// Records a collection's length under the collection's own path, for
        /// example <c>SetLength("players", 4)</c> before <c>players[0]</c>
        /// through <c>players[3]</c>.
        /// </summary>
        public void SetLength(string path, ulong count)
        {
            byte[] p = PathBytes(path);
            Check(Native.tickwise_dump_set_len(Handle, p, Len(p), count), "tickwise_dump_set_len");
        }

        /// <summary>Releases the native builder.</summary>
        public void Dispose()
        {
            if (_disposed)
            {
                return;
            }
            _disposed = true;
            _handle.Dispose();
        }

        private static byte[] PathBytes(string path)
        {
            if (path == null)
            {
                throw new ArgumentNullException(nameof(path));
            }
            if (path.Length == 0)
            {
                throw new ArgumentException("path is empty", nameof(path));
            }
            return Encoding.UTF8.GetBytes(path);
        }

        private static UIntPtr Len(byte[] bytes) => (UIntPtr)bytes.Length;

        private static void Check(TickwiseStatus status, string operation)
        {
            TickwiseException.ThrowIfFailed(status, operation);
        }

        private void ThrowIfDisposed()
        {
            if (_disposed)
            {
                throw new ObjectDisposedException(nameof(TickwiseDump));
            }
        }
    }
}
