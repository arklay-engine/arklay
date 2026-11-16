//! A seeded stream for the tests: the same xoshiro256** through SplitMix64 as
//! `arklay_biorand::rng::Rng`, so seed `n` here gives the same routes as
//! `Rng::new(n)` there. The crate can't use that type in its own tests, as a
//! dev-dependency on arklay-biorand would build this crate twice.
//!
//! The unit tests include this file too (`src/lib.rs`).

use arklay_routing::RouteRng;

/// A deterministic stream for a seed.
pub struct Seeded {
    state: [u64; 4],
}

impl Seeded {
    pub fn new(seed: u64) -> Self {
        let mut mix = seed;
        let mut next = || {
            mix = mix.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = mix;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        Self {
            state: [next(), next(), next(), next()],
        }
    }

    fn next_u64(&mut self) -> u64 {
        let [s0, s1, s2, s3] = &mut self.state;
        let result = s1.wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = *s1 << 17;
        *s2 ^= *s0;
        *s3 ^= *s1;
        *s1 ^= *s2;
        *s0 ^= *s3;
        *s2 ^= t;
        *s3 = s3.rotate_left(45);
        result
    }
}

impl RouteRng for Seeded {
    fn index(&mut self, len: usize) -> usize {
        let span = len as u64;
        let zone = u64::MAX - u64::MAX % span;
        loop {
            let value = self.next_u64();
            if value < zone {
                return (value % span) as usize;
            }
        }
    }
}
