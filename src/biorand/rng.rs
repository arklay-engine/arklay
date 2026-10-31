//! The generator's own random numbers: a fixed PCG32, so a seed's output does
//! not depend on any dependency's idea of a random stream.

/// A PCG32 stream.
pub struct Rng {
    state: u64,
}

const MULTIPLIER: u64 = 0x5851_F42D_4C95_7F2D;
const INCREMENT: u64 = 0x14_0577_27F1;

impl Rng {
    pub fn new(seed: u32) -> Self {
        let mut rng = Self {
            state: u64::from(seed).wrapping_add(INCREMENT),
        };
        rng.next();
        rng
    }

    fn next(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(MULTIPLIER).wrapping_add(INCREMENT);
        let shifted = (((old >> 18) ^ old) >> 27) as u32;
        shifted.rotate_right((old >> 59) as u32)
    }

    /// A number below `limit`, which must not be zero.
    pub fn below(&mut self, limit: usize) -> usize {
        // The modulo bias of these small limits does not matter here.
        self.next() as usize % limit
    }

    /// A number from `low` to `high`, inclusive.
    pub fn range(&mut self, low: usize, high: usize) -> usize {
        low + self.below(high - low + 1)
    }

    /// One element of a slice that must not be empty.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for last in (1..items.len()).rev() {
            items.swap(last, self.below(last + 1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seed_replays_the_same_stream() {
        let stream = |seed| {
            let mut rng = Rng::new(seed);
            (0..16).map(|_| rng.below(1000)).collect::<Vec<_>>()
        };
        assert_eq!(stream(123_456), stream(123_456));
        assert_ne!(stream(123_456), stream(123_457));
    }

    #[test]
    fn the_stream_is_pinned() {
        // Changing the stream changes every seed: bump `GENERATOR_VERSION`.
        let mut rng = Rng::new(1);
        let first: Vec<_> = (0..4).map(|_| rng.below(1000)).collect();
        assert_eq!(first, [136, 961, 798, 654]);
    }

    #[test]
    fn shuffle_keeps_every_element() {
        let mut rng = Rng::new(9);
        let mut items: Vec<_> = (0..32).collect();
        rng.shuffle(&mut items);
        items.sort_unstable();
        assert_eq!(items, (0..32).collect::<Vec<_>>());
    }
}
