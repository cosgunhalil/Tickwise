using System;
using System.IO;
using UnityEngine;

namespace Tickwise.Samples.DeterministicMiniGame
{
    /// <summary>
    /// Drives <see cref="MiniGameSim"/> from FixedUpdate and records every
    /// tick with Tickwise. Press Play once with Inject Chaos off and once with
    /// it on, then compare the two recordings with the command line tool.
    /// The console prints the exact command when a session finishes.
    /// </summary>
    public sealed class TickwiseSampleRunner : MonoBehaviour
    {
        [Tooltip("File name of the recording, without extension. Change it between runs so the second run does not overwrite the first.")]
        public string sessionName = "clean";

        [Tooltip("Ticks to record before the session finishes on its own. 600 ticks is ten seconds at 60 ticks per second.")]
        public int ticksToRecord = 600;

        [Tooltip("Seed for the simulation. Both runs must use the same seed for the comparison to mean anything.")]
        public uint seed = 12345;

        [Tooltip("The injected bug: from Chaos At Tick on, a wall clock value leaks into the first ball's position. Two runs stop agreeing at exactly that tick.")]
        public bool injectChaos;

        [Tooltip("First tick where the injected bug takes effect.")]
        public int chaosAtTick = 421;

        private MiniGameSim _sim;
        private TickwiseRecorder _recorder;
        private readonly byte[] _input = new byte[1];
        private ulong _tick;
        private string _path;
        private string _status = "";
        private bool _done;

        private void Awake()
        {
            Time.fixedDeltaTime = 1f / 60f;
            _sim = new MiniGameSim(seed);

            string dir = Path.Combine(Application.persistentDataPath, "tickwise");
            Directory.CreateDirectory(dir);
            _path = Path.Combine(dir, sessionName + ".rec");

            var config = new RecorderConfig
            {
                GameId = "tickwise-mini-game",
                BuildHash = Application.version,
                Platform = Application.platform.ToString(),
                TickRate = 60,
                RngSeed = seed,
                FullHashInterval = 50,
                SnapshotEvery = 300,
                HashAlgoId = HashAlgo.Xxh3,
                InputFormatId = 1,
            }.StampCreatedAt();

            try
            {
                _recorder = TickwiseRecorder.Create(_path, config);
                _status = $"recording to {_path}";
                Debug.Log($"Tickwise: recording to {_path} with native library {TickwiseNative.Version}");
            }
            catch (TickwiseException ex)
            {
                _status = "could not start recording: " + ex.Message;
                Debug.LogError("Tickwise: " + ex.Message);
                _done = true;
            }
            catch (DllNotFoundException ex)
            {
                _status = "native library not found, see the console";
                Debug.LogError("Tickwise: tickwise_ffi could not be loaded. Build it with bridges/tickwise-ffi/scripts/build-for-unity.ps1 or install a released package version. " + ex.Message);
                _done = true;
            }
        }

        private void FixedUpdate()
        {
            if (_done)
            {
                return;
            }

            // Inputs are scripted rather than read from the keyboard, so both
            // runs see identical input and the only difference is the bug.
            _input[0] = ScriptedInput(_tick);

            int chaosNoise = 0;
            if (injectChaos && _tick >= (ulong)chaosAtTick)
            {
                // Wall clock milliseconds, never the same on two runs. Always
                // nonzero, so the divergence lands on chaosAtTick exactly. The
                // simulation adds it to the first ball's position.
                chaosNoise = 1 + (int)((long)(Time.realtimeSinceStartupAsDouble * 1000.0) % 7);
            }

            _sim.Step(_input[0], chaosNoise);

            try
            {
                _recorder.RecordTick(_tick, _input, _sim);
                if (_recorder.WantsSnapshot(_tick))
                {
                    _recorder.RecordSnapshot(_tick, _sim.Serialize());
                }
                if (_tick == 300)
                {
                    _recorder.RecordMarker(_tick, "halfway");
                }
            }
            catch (TickwiseException ex)
            {
                _status = "recording failed: " + ex.Message;
                Debug.LogError("Tickwise: " + ex.Message);
                _done = true;
                return;
            }

            _tick++;
            if (_tick >= (ulong)ticksToRecord)
            {
                FinishSession();
            }
        }

        private void FinishSession()
        {
            _done = true;
            try
            {
                _recorder.Dispose();
                _recorder = null;
                _status = $"finished {ticksToRecord} ticks, score {_sim.Score}\nsaved {_path}";
                Debug.Log(
                    $"Tickwise: finished {ticksToRecord} ticks, saved {_path}\n" +
                    "Run again with a different Session Name and Inject Chaos toggled, then compare:\n" +
                    $"  tickwise compare \"{Path.Combine(Path.GetDirectoryName(_path), "clean.rec")}\" \"{Path.Combine(Path.GetDirectoryName(_path), "chaotic.rec")}\"");
            }
            catch (TickwiseException ex)
            {
                _status = "could not finish the recording: " + ex.Message;
                Debug.LogError("Tickwise: " + ex.Message);
            }
        }

        private void OnDestroy()
        {
            // Leaving Play mode early still produces a readable file.
            if (_recorder != null)
            {
                try
                {
                    _recorder.Dispose();
                }
                catch (TickwiseException ex)
                {
                    Debug.LogError("Tickwise: " + ex.Message);
                }
                _recorder = null;
            }
        }

        private static byte ScriptedInput(ulong tick)
        {
            // A slow four-beat pattern: right, down, left, up, with pauses.
            ulong phase = (tick / 45) % 8;
            switch (phase)
            {
                case 0: return 1;
                case 2: return 4;
                case 4: return 2;
                case 6: return 8;
                default: return 0;
            }
        }

        private void OnGUI()
        {
            const float arenaPixels = 320f;
            const float margin = 16f;
            float scale = arenaPixels / MiniGameSim.ArenaSize;

            GUI.Box(new Rect(margin, margin, arenaPixels, arenaPixels), GUIContent.none);
            for (int i = 0; i < MiniGameSim.BallCount; i++)
            {
                float x = margin + _sim.X(i) * scale;
                float y = margin + _sim.Y(i) * scale;
                GUI.Box(new Rect(x - 6f, y - 6f, 12f, 12f), i == 0 && injectChaos ? "!" : "");
            }

            string mode = injectChaos ? $"chaos from tick {chaosAtTick}" : "clean";
            GUI.Label(
                new Rect(margin, margin + arenaPixels + 8f, 900f, 120f),
                $"Tickwise sample, {mode}\ntick {_tick} / {ticksToRecord}, score {_sim.Score}, light hash {_sim.LightHash():x16}\n{_status}");
        }
    }
}
