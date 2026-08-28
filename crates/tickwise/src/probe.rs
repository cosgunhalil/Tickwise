//! The callback core of Tickwise.
//!
//! [`DeterminismProbe`] is the single contract between a simulation and
//! Tickwise. Everything else in the kit is built on top of these three
//! callbacks.

/// The single contract between the user's simulation and Tickwise.
///
/// Tickwise is an observer. The user drives their own game loop and calls
/// into the kit, which calls back through this trait. The design rule for
/// this trait is that it must stay simple enough to cross an FFI boundary
/// cleanly: no generics, no closures, plain return values.
///
/// # Examples
///
/// ```
/// use tickwise::{DeterminismProbe, StateDump};
///
/// struct MySim {
///     entity_count: u64,
///     rng_seed: u64,
///     score: u64,
/// }
///
/// impl DeterminismProbe for MySim {
///     fn light_hash(&self) -> u64 {
///         // A cheap digest of desync-critical values, not the whole state.
///         self.entity_count
///             .wrapping_mul(0x9E37_79B9_7F4A_7C15)
///             ^ self.rng_seed.rotate_left(17)
///             ^ self.score
///     }
///
///     fn full_hash(&self) -> u64 {
///         // A real implementation hashes all gameplay state here.
///         self.light_hash()
///     }
///
///     fn state_dump(&self) -> StateDump {
///         StateDump::empty()
///     }
/// }
/// ```
pub trait DeterminismProbe {
    /// Returns a cheap digest of desync-critical state.
    ///
    /// Called every tick during recording and replay, so it must be cheap.
    /// The budget target is below 1 percent of the tick. Hash a critical
    /// digest rather than the whole state, for example entity counts,
    /// player states, the RNG seed, and the score.
    fn light_hash(&self) -> u64;

    /// Returns a hash covering all gameplay state.
    ///
    /// Called every N ticks, where the interval is configurable on the
    /// recorder. May be slower than [`light_hash`], but it still runs
    /// inside the game loop. Anything left out of this hash is a blind
    /// spot where a desync can hide.
    ///
    /// [`light_hash`]: DeterminismProbe::light_hash
    fn full_hash(&self) -> u64;

    /// Returns a full structural dump of the gameplay state.
    ///
    /// Called only during Pass 2 replay, at the target ticks requested for
    /// dumping. This is the expensive path and it is allowed to be: it runs
    /// a handful of times per session, not every tick.
    fn state_dump(&self) -> StateDump;
}

/// A structural, diffable representation of simulation state.
///
/// Conceptually a tree of field paths mapped to typed values, which the
/// diff engine walks to report field-level differences.
///
/// The internal representation is deliberately undecided until M3, so this
/// type is opaque: it can be created empty and passed around, and nothing
/// else. Do not rely on its layout.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StateDump {
    _placeholder: (),
}

impl StateDump {
    /// Creates an empty dump with no recorded fields.
    pub fn empty() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The FFI plan and the replayer both need the probe as a trait object,
    // so dyn compatibility is a contract, not an accident.
    #[test]
    fn probe_is_dyn_compatible() {
        struct Nop;

        impl DeterminismProbe for Nop {
            fn light_hash(&self) -> u64 {
                0
            }

            fn full_hash(&self) -> u64 {
                0
            }

            fn state_dump(&self) -> StateDump {
                StateDump::empty()
            }
        }

        let probe: &dyn DeterminismProbe = &Nop;
        assert_eq!(probe.light_hash(), 0);
        assert_eq!(probe.full_hash(), 0);
        assert_eq!(probe.state_dump(), StateDump::empty());
    }
}
