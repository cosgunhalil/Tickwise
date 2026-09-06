namespace Tickwise
{
    /// <summary>
    /// The contract between your simulation and Tickwise: two hashes of
    /// gameplay state that Tickwise records every tick and compares across
    /// machines. Tickwise never runs your simulation; you call
    /// <see cref="TickwiseRecorder.RecordTick(ulong, System.ReadOnlySpan{byte}, IDeterminismProbe)"/>
    /// from your own loop and it asks the probe for what it needs.
    /// </summary>
    public interface IDeterminismProbe
    {
        /// <summary>
        /// A cheap digest of desync-critical state, asked for on every tick.
        /// Hash the values that would make two machines disagree: entity
        /// count, player states, the random seed, the score. Stay well under
        /// one percent of the tick budget.
        /// </summary>
        ulong LightHash();

        /// <summary>
        /// A hash over all gameplay state, asked for only on the ticks where
        /// the recorder keeps a full hash, every 300 ticks by default. It may
        /// be slower than <see cref="LightHash"/>, but anything left out of
        /// it is a blind spot where a desync can hide.
        /// </summary>
        ulong FullHash();
    }
}
