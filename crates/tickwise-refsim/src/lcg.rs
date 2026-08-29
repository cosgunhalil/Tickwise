//! Self-contained deterministic random generator.
//!
//! The reference simulation must not depend on external crates, and its
//! randomness must be fully reproducible from a seed. A linear congruential
//! generator is enough for both.

/// A 64 bit linear congruential generator with MMIX constants.
///
/// Not cryptographic and not statistically strong, and neither matters
/// here. What matters is that the same seed always produces the same
/// sequence on every platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lcg {
    state: u64,
}

const MULTIPLIER: u64 = 6364136223846793005;
const INCREMENT: u64 = 1442695040888963407;

impl Lcg {
    /// Creates a generator from a seed.
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Advances the generator and returns the next value.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(MULTIPLIER).wrapping_add(INCREMENT);
        self.state
    }

    /// Returns a float in the range 0 to 1, excluding 1.
    ///
    /// Built from the top 24 bits of the next value, so the conversion is
    /// exact and identical on every platform.
    pub fn next_f32(&mut self) -> f32 {
        let bits = self.next_u64() >> 40;
        bits as f32 / (1u32 << 24) as f32
    }

    /// Returns a float in the range min to max, excluding max.
    pub fn next_f32_in(&mut self, min: f32, max: f32) -> f32 {
        min + self.next_f32() * (max - min)
    }

    /// Returns the current internal state, for hashing into digests.
    pub fn state(&self) -> u64 {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Lcg::new(42);
        let mut b = Lcg::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Lcg::new(1);
        let mut b = Lcg::new(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn next_f32_stays_in_range() {
        let mut rng = Lcg::new(7);
        for _ in 0..10_000 {
            let v = rng.next_f32();
            assert!((0.0..1.0).contains(&v));
        }
    }
}
