using System;

namespace Tickwise
{
    /// <summary>
    /// Settings for a recording session. Session metadata is written into the
    /// recording header; the intervals and identifiers control what the
    /// recorder keeps and how tools interpret it.
    /// </summary>
    public sealed class RecorderConfig
    {
        /// <summary>Identifier of the game or application.</summary>
        public string GameId { get; set; } = string.Empty;

        /// <summary>Build identifier, for example a git hash or version string.</summary>
        public string BuildHash { get; set; } = string.Empty;

        /// <summary>Platform the session runs on, for example windows-x86_64.</summary>
        public string Platform { get; set; } = string.Empty;

        /// <summary>Simulation rate in ticks per second.</summary>
        public uint TickRate { get; set; }

        /// <summary>Seed the simulation started from.</summary>
        public ulong RngSeed { get; set; }

        /// <summary>Creation time as unix seconds. Metadata only, never compared.</summary>
        public ulong CreatedAt { get; set; }

        /// <summary>Full hash interval in ticks. Zero disables full hashes.</summary>
        public uint FullHashInterval { get; set; } = 300;

        /// <summary>Snapshot interval in ticks. Zero disables snapshots.</summary>
        public uint SnapshotEvery { get; set; }

        /// <summary>
        /// Identifier of the hash algorithm the probe uses, see <see cref="HashAlgo"/>.
        /// Stored so tools can warn when two recordings used different hashes.
        /// </summary>
        public ushort HashAlgoId { get; set; } = HashAlgo.UserDefined;

        /// <summary>
        /// Your own identifier for the input encoding. Change it whenever the
        /// input bytes change meaning, and replay will refuse a recording made
        /// with the old encoding instead of misreading it.
        /// </summary>
        public ulong InputFormatId { get; set; }

        /// <summary>Sets <see cref="CreatedAt"/> to the current time.</summary>
        public RecorderConfig StampCreatedAt()
        {
            CreatedAt = (ulong)DateTimeOffset.UtcNow.ToUnixTimeSeconds();
            return this;
        }
    }
}
