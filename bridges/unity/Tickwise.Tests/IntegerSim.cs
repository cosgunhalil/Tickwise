using System;

namespace Tickwise.Tests
{
    /// <summary>
    /// The same four-body integer simulation the C harness uses, so a
    /// recording made from C# and one made from C carry comparable hashes.
    /// Integer math and a private LCG keep it deterministic everywhere.
    /// </summary>
    internal sealed class IntegerSim : IDeterminismProbe, ITickwiseStateWriter
    {
        private readonly ulong[] _pos = new ulong[4];
        private readonly byte[] _full = new byte[44];
        private readonly byte[] _light = new byte[12];
        private ulong _score;
        private uint _rng;

        public IntegerSim(uint seed)
        {
            _rng = seed;
        }

        public ulong Score => _score;

        public void Step(ReadOnlySpan<byte> inputs, bool injectDefect)
        {
            for (int i = 0; i < 4; i++)
            {
                _rng = _rng * 1664525u + 1013904223u;
                _pos[i] += inputs[i % 2] + (_rng & 0xFu);
            }
            _score += _pos[0] % 7u;
            if (injectDefect)
            {
                // A stale value leaking into gameplay state, the kind of bug
                // Tickwise exists to locate.
                _score += 1;
            }
        }

        /// <summary>Score and RNG only: cheap, and blind to positions on purpose.</summary>
        public ulong LightHash()
        {
            BitConverter.TryWriteBytes(new Span<byte>(_light, 0, 8), _score);
            BitConverter.TryWriteBytes(new Span<byte>(_light, 8, 4), _rng);
            return Xxh3.Hash64(_light);
        }

        public ulong FullHash()
        {
            Span<byte> buf = _full;
            for (int i = 0; i < 4; i++)
            {
                BitConverter.TryWriteBytes(buf.Slice(i * 8, 8), _pos[i]);
            }
            BitConverter.TryWriteBytes(buf.Slice(32, 8), _score);
            BitConverter.TryWriteBytes(buf.Slice(40, 4), _rng);
            return Xxh3.Hash64(_full);
        }

        /// <summary>Every field the full hash covers, by name, with the collection's length.</summary>
        public void WriteState(TickwiseDump dump)
        {
            dump.SetUInt64("score", _score);
            dump.SetUInt64("rng", _rng);
            dump.SetLength("pos", 4);
            for (int i = 0; i < 4; i++)
            {
                dump.SetUInt64("pos[" + i + "]", _pos[i]);
            }
        }

        /// <summary>Serializes the state in the same 44-byte layout the full hash uses.</summary>
        public byte[] Snapshot()
        {
            FullHash();
            return (byte[])_full.Clone();
        }

        /// <summary>Restores from a <see cref="Snapshot"/>, the caller's half of a snapshot seek.</summary>
        public void Restore(ReadOnlySpan<byte> snapshot)
        {
            for (int i = 0; i < 4; i++)
            {
                _pos[i] = BitConverter.ToUInt64(snapshot.Slice(i * 8, 8));
            }
            _score = BitConverter.ToUInt64(snapshot.Slice(32, 8));
            _rng = BitConverter.ToUInt32(snapshot.Slice(40, 4));
        }
    }
}
