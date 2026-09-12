using System;

namespace Tickwise.Samples.DeterministicMiniGame
{
    /// <summary>
    /// Eight balls bouncing in a box, simulated with integers only. There is
    /// no engine code in here: the class is plain C#, steps on demand, and
    /// hashes its own state. That separation is the whole point of the
    /// sample. Your gameplay simulation should look like this from
    /// Tickwise's side, whatever renders it.
    /// </summary>
    public sealed class MiniGameSim : IDeterminismProbe, ITickwiseStateWriter
    {
        public const int BallCount = 8;

        /// <summary>Arena size in fixed-point units. 16 units per pixel at 1x scale.</summary>
        public const int ArenaSize = 16000;

        private const int Radius = 400;

        private readonly int[] _x = new int[BallCount];
        private readonly int[] _y = new int[BallCount];
        private readonly int[] _vx = new int[BallCount];
        private readonly int[] _vy = new int[BallCount];
        private readonly byte[] _lightBuffer = new byte[32];
        private readonly byte[] _fullBuffer = new byte[24 + BallCount * 16];
        private uint _rng;
        private ulong _score;
        private ulong _tick;

        public MiniGameSim(uint seed)
        {
            _rng = seed;
            for (int i = 0; i < BallCount; i++)
            {
                _x[i] = Radius + (int)(NextRandom() % (uint)(ArenaSize - 2 * Radius));
                _y[i] = Radius + (int)(NextRandom() % (uint)(ArenaSize - 2 * Radius));
                _vx[i] = 40 + (int)(NextRandom() % 80u);
                _vy[i] = 40 + (int)(NextRandom() % 80u);
                if ((NextRandom() & 1u) == 0) _vx[i] = -_vx[i];
                if ((NextRandom() & 1u) == 0) _vy[i] = -_vy[i];
            }
        }

        public ulong Tick => _tick;
        public ulong Score => _score;

        public int X(int ball) => _x[ball];
        public int Y(int ball) => _y[ball];

        /// <summary>
        /// Advances one tick. The input byte's low four bits nudge every ball
        /// up, down, left, or right. <paramref name="chaosNoise"/> is the
        /// sample's injected bug: anything nonzero leaks into the first
        /// ball's position, and because the caller derives it from the wall
        /// clock, two runs stop agreeing at that tick.
        /// </summary>
        public void Step(byte input, int chaosNoise)
        {
            int nudgeX = ((input & 1) != 0 ? 8 : 0) - ((input & 2) != 0 ? 8 : 0);
            int nudgeY = ((input & 4) != 0 ? 8 : 0) - ((input & 8) != 0 ? 8 : 0);

            // The planted bug. Positions are only ever bounced, never clamped,
            // so any nonzero noise is visible in the state on this very tick.
            _x[0] += chaosNoise;

            for (int i = 0; i < BallCount; i++)
            {
                _vx[i] += nudgeX;
                _vy[i] += nudgeY;
                _vx[i] = Clamp(_vx[i], -300, 300);
                _vy[i] = Clamp(_vy[i], -300, 300);
                _x[i] += _vx[i];
                _y[i] += _vy[i];

                if (_x[i] < Radius) { _x[i] = 2 * Radius - _x[i]; _vx[i] = -_vx[i]; _score += 1; }
                if (_x[i] > ArenaSize - Radius) { _x[i] = 2 * (ArenaSize - Radius) - _x[i]; _vx[i] = -_vx[i]; _score += 1; }
                if (_y[i] < Radius) { _y[i] = 2 * Radius - _y[i]; _vy[i] = -_vy[i]; _score += 1; }
                if (_y[i] > ArenaSize - Radius) { _y[i] = 2 * (ArenaSize - Radius) - _y[i]; _vy[i] = -_vy[i]; _score += 1; }
            }

            // A little randomness in gameplay, from the simulation's own
            // generator, so the recording has an RNG state worth hashing.
            int lucky = (int)(NextRandom() % (uint)BallCount);
            _vy[lucky] += (int)(NextRandom() % 3u) - 1;

            _tick++;
        }

        /// <summary>
        /// Score, RNG state, tick, and one sum over every position: a digest
        /// cheap enough for every tick that still notices a ball being moved.
        /// Drop the position sum and compare still catches the planted bug,
        /// but later, when a shifted bounce changes the score. That gap is
        /// the light hash blind spot, and the full hash exists to close it.
        /// </summary>
        public ulong LightHash()
        {
            long positionSum = 0;
            for (int i = 0; i < BallCount; i++)
            {
                positionSum += _x[i] + _y[i];
            }
            WriteU64(_lightBuffer, 0, _score);
            WriteU64(_lightBuffer, 8, _rng);
            WriteU64(_lightBuffer, 16, _tick);
            WriteU64(_lightBuffer, 24, (ulong)positionSum);
            return Xxh3.Hash64(_lightBuffer);
        }

        /// <summary>Everything, including every ball's position and velocity.</summary>
        public ulong FullHash()
        {
            return Xxh3.Hash64(Serialize());
        }

        /// <summary>
        /// The same fields the full hash covers, by name, so <c>tickwise diff</c>
        /// can say which one moved. With the planted bug it points at
        /// <c>balls[0].x</c>, which is exactly where the bug lives.
        /// </summary>
        public void WriteState(TickwiseDump dump)
        {
            dump.SetUInt64("score", _score);
            dump.SetUInt64("rng", _rng);
            dump.SetUInt64("tick", _tick);
            dump.SetLength("balls", BallCount);
            for (int i = 0; i < BallCount; i++)
            {
                string ball = "balls[" + i + "]";
                dump.SetInt64(ball + ".x", _x[i]);
                dump.SetInt64(ball + ".y", _y[i]);
                dump.SetInt64(ball + ".vx", _vx[i]);
                dump.SetInt64(ball + ".vy", _vy[i]);
            }
        }

        /// <summary>The full state in a fixed little-endian layout, for snapshots.</summary>
        public byte[] Serialize()
        {
            WriteU64(_fullBuffer, 0, _score);
            WriteU64(_fullBuffer, 8, _rng);
            WriteU64(_fullBuffer, 16, _tick);
            int offset = 24;
            for (int i = 0; i < BallCount; i++)
            {
                WriteI32(_fullBuffer, offset, _x[i]);
                WriteI32(_fullBuffer, offset + 4, _y[i]);
                WriteI32(_fullBuffer, offset + 8, _vx[i]);
                WriteI32(_fullBuffer, offset + 12, _vy[i]);
                offset += 16;
            }
            return _fullBuffer;
        }

        private uint NextRandom()
        {
            _rng = _rng * 1664525u + 1013904223u;
            return _rng;
        }

        private static int Clamp(int value, int min, int max)
        {
            return value < min ? min : value > max ? max : value;
        }

        private static void WriteU64(byte[] buffer, int offset, ulong value)
        {
            for (int b = 0; b < 8; b++)
            {
                buffer[offset + b] = (byte)(value >> (8 * b));
            }
        }

        private static void WriteI32(byte[] buffer, int offset, int value)
        {
            uint bits = (uint)value;
            for (int b = 0; b < 4; b++)
            {
                buffer[offset + b] = (byte)(bits >> (8 * b));
            }
        }
    }
}
