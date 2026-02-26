//! Fast PRNG for combat simulation. Uses SplitMix64 for throughput and good statistical quality.
//! Deterministic: same seed produces the same sequence. Not cryptographically secure.

const SPLITMIX64_GOLDEN: u64 = 0x9e3779b97f4a7c15;
const SPLITMIX64_M1: u64 = 0xbf58476d1ce4e5b9;
const SPLITMIX64_M2: u64 = 0x94d049bb133111eb;

#[derive(Debug, Clone, Copy)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Returns the next 64-bit value. Same API as before; sequence differs from the prior LCG.
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(SPLITMIX64_GOLDEN);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(SPLITMIX64_M1);
        z = (z ^ (z >> 27)).wrapping_mul(SPLITMIX64_M2);
        z ^ (z >> 31)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitmix64_deterministic() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn splitmix64_different_seeds_differ() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }
}
