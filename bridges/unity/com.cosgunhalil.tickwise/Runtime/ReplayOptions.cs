using System;

namespace Tickwise
{
    /// <summary>Settings for a <see cref="TickwiseReplayer"/> session.</summary>
    public sealed class ReplayOptions
    {
        /// <summary>
        /// Ticks whose state dumps the replay collects and
        /// <see cref="TickwiseReplayer.Finish"/> writes. Every tick must lie
        /// inside the recording. <c>tickwise compare</c> names the tick to
        /// ask for.
        /// </summary>
        public ulong[] DumpAtTicks { get; set; } = Array.Empty<ulong>();

        /// <summary>
        /// Compare the live hashes against the recording after every step.
        /// On by default; a mismatch means your simulation is not reproducing
        /// the session, which is the self-check worth running first.
        /// </summary>
        public bool VerifyHashes { get; set; } = true;

        /// <summary>
        /// Refuse a recording whose input format id differs from
        /// <see cref="ExpectedInputFormatId"/>, so bytes from an older
        /// encoding never reach the wrong decoder.
        /// </summary>
        public bool CheckInputFormat { get; set; }

        /// <summary>The input format id this build decodes, compared when <see cref="CheckInputFormat"/> is set.</summary>
        public ulong ExpectedInputFormatId { get; set; }
    }
}
