//! The serde convenience layer, behind the `serde` feature.
//!
//! The callback core stays the honest foundation. This module is the thin
//! wrapper on top of it: an automatic [`SerdeProbe`] for any `Serialize`
//! state, [`to_dump`] turning that state into a structural dump, typed
//! inputs on the recorder and the replayer, and the hashes behind
//! decision #15, xxh3 by default and blake3 behind its own feature.
//!
//! One rule to remember: hashing serializes the whole value, so a
//! `HashMap` in your state hashes in iteration order and is not
//! deterministic. Use `BTreeMap`, or hand the probe a light view that
//! leaves the map out. Dumps are immune to this, they sort by path.
//!
//! # The ten-line onboarding
//!
//! ```
//! use serde::Serialize;
//! use tickwise::serde_probe::SerdeProbe;
//! use tickwise::{Recorder, RecorderConfig};
//!
//! #[derive(Serialize)]
//! struct Game { tick: u64, score: u64, positions: Vec<(f32, f32)> }
//!
//! # fn main() -> Result<(), tickwise::RecordError> {
//! let mut game = Game { tick: 0, score: 0, positions: vec![(0.0, 0.0)] };
//! let mut rec = Recorder::new(Vec::new(), RecorderConfig::default())?;
//! for tick in 0..100 {
//!     game.tick += 1;
//!     game.positions[0].0 += 0.5;
//!     rec.record_tick_typed(tick, &(1u8, 0u8), &SerdeProbe::new(&game))?;
//! }
//! let recording = rec.finish()?;
//! assert!(!recording.is_empty());
//! # Ok(())
//! # }
//! ```

mod ser;

pub use ser::{DumpError, to_dump};

use crate::dump::StateDump;
use crate::probe::DeterminismProbe;
use serde::Serialize;

/// Hash algorithms available to the automatic probe, decision #15.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HashAlgo {
    /// xxh3 64 bit, fast, the default. `hash_algo_id` 1.
    #[default]
    Xxh3,
    /// blake3 truncated to 64 bits, for audit scenarios. `hash_algo_id` 2.
    #[cfg(feature = "blake3")]
    Blake3,
}

impl HashAlgo {
    /// The `hash_algo_id` stored in recording headers. Zero is reserved
    /// for user-defined hashing through the callback core.
    pub fn id(self) -> u16 {
        match self {
            Self::Xxh3 => 1,
            #[cfg(feature = "blake3")]
            Self::Blake3 => 2,
        }
    }

    /// Hashes bytes with this algorithm.
    pub fn hash(self, bytes: &[u8]) -> u64 {
        match self {
            Self::Xxh3 => xxhash_rust::xxh3::xxh3_64(bytes),
            #[cfg(feature = "blake3")]
            Self::Blake3 => {
                let digest = blake3::hash(bytes);
                let mut first = [0u8; 8];
                first.copy_from_slice(&digest.as_bytes()[..8]);
                u64::from_le_bytes(first)
            }
        }
    }
}

/// Derives an input format id from a label such as `"MyInput v3"`.
///
/// Bump the label whenever the input type's encoding changes and the
/// replayer will refuse recordings made with the old one, decision #11.
pub fn format_id(label: &str) -> u64 {
    let mut digest = crate::format::wire::Fnv1a::new();
    digest.update(label.as_bytes());
    digest.value()
}

/// Serializes a value with postcard, the deterministic encoding behind
/// the typed helpers.
pub fn to_bytes<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, postcard::Error> {
    postcard::to_allocvec(value)
}

/// An automatic [`DeterminismProbe`] for any `Serialize` state.
///
/// Hashes are taken over the postcard encoding of the state. With
/// [`SerdeProbe::new`] the light and full hashes both cover the whole
/// state, which is the simplest setup and fine for small states. For the
/// light hash budget on larger states, build a small view struct of the
/// desync-critical fields and use [`SerdeProbe::with_light`].
///
/// If serialization fails, hashes fall back to zero and the dump carries
/// a single `$error` entry describing the failure, so the problem shows
/// up in every report instead of hiding.
pub struct SerdeProbe<'a, S: Serialize + ?Sized, L: Serialize + ?Sized = S> {
    state: &'a S,
    light: Option<&'a L>,
    algo: HashAlgo,
}

impl<'a, S: Serialize + ?Sized> SerdeProbe<'a, S, S> {
    /// A probe whose light and full hashes both cover the whole state.
    pub fn new(state: &'a S) -> Self {
        Self {
            state,
            light: None,
            algo: HashAlgo::default(),
        }
    }
}

impl<'a, S: Serialize + ?Sized, L: Serialize + ?Sized> SerdeProbe<'a, S, L> {
    /// A probe whose light hash covers only the given view.
    pub fn with_light(state: &'a S, light: &'a L) -> Self {
        Self {
            state,
            light: Some(light),
            algo: HashAlgo::default(),
        }
    }

    /// Selects the hash algorithm.
    pub fn with_algo(mut self, algo: HashAlgo) -> Self {
        self.algo = algo;
        self
    }

    /// The `hash_algo_id` to put in [`RecorderConfig`](crate::RecorderConfig).
    pub fn hash_algo_id(&self) -> u16 {
        self.algo.id()
    }

    fn hash_of<T: Serialize + ?Sized>(&self, value: &T) -> u64 {
        to_bytes(value)
            .map(|bytes| self.algo.hash(&bytes))
            .unwrap_or(0)
    }
}

impl<S: Serialize + ?Sized, L: Serialize + ?Sized> DeterminismProbe for SerdeProbe<'_, S, L> {
    fn light_hash(&self) -> u64 {
        match self.light {
            Some(light) => self.hash_of(light),
            None => self.hash_of(self.state),
        }
    }

    fn full_hash(&self) -> u64 {
        self.hash_of(self.state)
    }

    fn state_dump(&self) -> StateDump {
        match to_dump(self.state) {
            Ok(dump) => dump,
            Err(err) => {
                let mut dump = StateDump::empty();
                dump.insert("$error", err.to_string());
                dump
            }
        }
    }
}
