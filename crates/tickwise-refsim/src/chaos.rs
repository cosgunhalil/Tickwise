//! Deliberate non-determinism injection.
//!
//! Each chaos mode makes the world commit one well-known class of
//! determinism sin, starting at a configured tick. CI records a clean
//! run and a chaotic run and proves Tickwise catches every class at the
//! correct tick. The refsim's own determinism rules are deliberately
//! violated inside the chaos paths, and nowhere else.

/// A class of non-determinism the world can inject.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChaosMode {
    /// Sub-epsilon float deviation, one ULP of ball velocity per tick.
    /// Invisible to the light hash, caught by the full hash: the classic
    /// cross-platform drift with a blind spot demonstration built in.
    FloatDrift,
    /// Order-dependent state folded through a real `HashMap`, whose
    /// iteration order is random per process. The genuine article.
    HashmapIter,
    /// A stale scratch value, left over from the previous tick and never
    /// reset, read where freshly initialized state belongs. Safe Rust
    /// cannot read truly uninitialized memory, so this simulates the
    /// stale-cache bug the same way it appears in production: the value
    /// is initialized, it is simply from the wrong moment.
    StaleValue,
    /// The wall clock leaking into the simulation RNG.
    TimeDependent,
}

impl ChaosMode {
    /// All modes, for tests and CLI listings.
    pub const ALL: [ChaosMode; 4] = [
        ChaosMode::FloatDrift,
        ChaosMode::HashmapIter,
        ChaosMode::StaleValue,
        ChaosMode::TimeDependent,
    ];
}

impl std::fmt::Display for ChaosMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::FloatDrift => "float-drift",
            Self::HashmapIter => "hashmap-iter",
            Self::StaleValue => "stale-value",
            Self::TimeDependent => "time-dependent",
        };
        write!(f, "{name}")
    }
}

/// Error for an unrecognized chaos mode name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownChaosMode(pub String);

impl std::fmt::Display for UnknownChaosMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unknown chaos mode {:?}, expected one of float-drift, \
             hashmap-iter, stale-value, time-dependent",
            self.0
        )
    }
}

impl std::error::Error for UnknownChaosMode {}

impl std::str::FromStr for ChaosMode {
    type Err = UnknownChaosMode;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "float-drift" => Ok(Self::FloatDrift),
            "hashmap-iter" => Ok(Self::HashmapIter),
            // The old name is accepted so existing scripts keep working.
            "stale-value" | "uninit-read" => Ok(Self::StaleValue),
            "time-dependent" => Ok(Self::TimeDependent),
            other => Err(UnknownChaosMode(other.to_string())),
        }
    }
}

/// When and how to inject chaos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChaosConfig {
    /// The class of non-determinism to inject.
    pub mode: ChaosMode,
    /// First tick the chaos strikes at.
    pub start_tick: u64,
}
