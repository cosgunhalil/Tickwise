namespace Tickwise
{
    /// <summary>
    /// The optional third half of a probe: writes the simulation's state
    /// into a <see cref="TickwiseDump"/> by field name. Implement it beside
    /// <see cref="IDeterminismProbe"/> on the same object to get field-level
    /// diffs, both from dumps recorded on an interval during Pass 1 and from
    /// dumps a <see cref="TickwiseReplayer"/> collects in Pass 2.
    /// </summary>
    /// <remarks>
    /// Called only on dump ticks, never every tick, so it may walk all of
    /// gameplay state. Write every field the full hash covers: a field the
    /// hash sees and the dump does not is a divergence the diff cannot name.
    /// Sort anything whose iteration order is not stable, and give every
    /// collection its length with <see cref="TickwiseDump.SetLength"/>.
    /// </remarks>
    public interface ITickwiseStateWriter
    {
        /// <summary>Fills <paramref name="dump"/> with the current state. The builder arrives empty.</summary>
        void WriteState(TickwiseDump dump);
    }
}
