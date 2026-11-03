//! The generator's random stream, with the operations staging's `Rng` and
//! `EndlessBag` offer.
//!
//! The same seed always gives the same stream, on every platform. It is not
//! .NET's `System.Random`: arklay doesn't reproduce staging's games for a
//! seed, so it uses xoshiro256** seeded through SplitMix64.

/// A deterministic random stream.
#[derive(Clone, Debug)]
pub struct Rng {
    state: [u64; 4],
}

impl Rng {
    /// The stream for a seed.
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

    /// The stream for a named stage of the generator, independent of how much
    /// any other stage draws. Staging forks one stream per stage the same way.
    pub fn named(seed: u64, name: &str) -> Self {
        // FNV-1a of the name, mixed into the seed.
        let hash = name.bytes().fold(0xCBF2_9CE4_8422_2325u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01B3)
        });
        Self::new(seed ^ hash.rotate_left(17))
    }

    /// A new stream seeded from this one (staging's `NextFork`).
    pub fn fork(&mut self) -> Self {
        Self::new(self.next_u64())
    }

    pub fn next_u64(&mut self) -> u64 {
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

    /// A number in `0.0..1.0`.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// A number in `min..max`, or `min` when the range is empty.
    pub fn next_f64_in(&mut self, min: f64, max: f64) -> f64 {
        if max <= min {
            return min;
        }
        min + self.next_f64() * (max - min)
    }

    /// An integer in `min..max` (exclusive), or `min` when the range is empty,
    /// as staging's `Next(min, max)`.
    pub fn next_in(&mut self, min: i32, max: i32) -> i32 {
        if max <= min {
            return min;
        }
        let span = (i64::from(max) - i64::from(min)) as u64;
        min + self.below(span) as i32
    }

    /// An index below `len`, which must not be zero.
    pub fn index(&mut self, len: usize) -> usize {
        assert!(len > 0, "index into an empty range");
        self.below(len as u64) as usize
    }

    /// Unbiased `0..span` by rejection.
    fn below(&mut self, span: u64) -> u64 {
        let zone = u64::MAX - u64::MAX % span;
        loop {
            let value = self.next_u64();
            if value < zone {
                return value % span;
            }
        }
    }

    /// Whether a `percent` in 100 chance comes up.
    pub fn chance(&mut self, percent: i32) -> bool {
        self.next_in(0, 100) < percent
    }

    /// One of `values`, which must not be empty.
    pub fn pick<'a, T>(&mut self, values: &'a [T]) -> &'a T {
        &values[self.index(values.len())]
    }

    /// Shuffle in place (Fisher–Yates).
    pub fn shuffle<T>(&mut self, values: &mut [T]) {
        for i in (1..values.len()).rev() {
            let j = self.index(i + 1);
            values.swap(i, j);
        }
    }

    /// A shuffled copy, as staging's `Shuffle` extension returns.
    pub fn shuffled<T: Clone>(&mut self, values: &[T]) -> Vec<T> {
        let mut values = values.to_vec();
        self.shuffle(&mut values);
        values
    }
}

/// A weighted choice (staging's `Rng.Table`). Entries with no weight are
/// left out.
#[derive(Clone, Debug, Default)]
pub struct Table<T> {
    entries: Vec<(T, f64)>,
    total: f64,
}

impl<T: Clone> Table<T> {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            total: 0.0,
        }
    }

    pub fn add(&mut self, value: T, weight: f64) {
        if weight == 0.0 {
            return;
        }
        self.entries.push((value, weight));
        self.total += weight;
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// A value chosen by weight, or `None` when the table is empty.
    pub fn next(&self, rng: &mut Rng) -> Option<T> {
        let (last, rest) = self.entries.split_last()?;
        let target = rng.next_f64() * self.total;
        let mut sum = 0.0;
        for (value, weight) in rest {
            sum += weight;
            if target < sum {
                return Some(value.clone());
            }
        }
        Some(last.0.clone())
    }
}

/// A bag that hands out its items in a shuffled order and refills itself,
/// shuffled again, when empty (staging's `EndlessBag`).
#[derive(Clone, Debug)]
pub struct EndlessBag<T> {
    all: Vec<T>,
    queue: std::collections::VecDeque<T>,
}

impl<T: Clone> EndlessBag<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self {
            all: items,
            queue: Default::default(),
        }
    }

    pub fn len(&self) -> usize {
        self.all.len()
    }

    pub fn is_empty(&self) -> bool {
        self.all.is_empty()
    }

    /// Add items, reshuffling them with what is left of the current round.
    pub fn extend(&mut self, rng: &mut Rng, items: &[T]) {
        let mut remaining: Vec<T> = self.queue.drain(..).chain(items.iter().cloned()).collect();
        rng.shuffle(&mut remaining);
        self.all.extend(items.iter().cloned());
        self.queue.extend(remaining);
    }

    /// The next item, or `None` when the bag holds nothing.
    pub fn next(&mut self, rng: &mut Rng) -> Option<T> {
        if self.all.is_empty() {
            return None;
        }
        if self.queue.is_empty() {
            let round = rng.shuffled(&self.all);
            self.queue.extend(round);
        }
        self.queue.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seed_always_gives_the_same_stream() {
        let a: Vec<u64> = (0..4)
            .map({
                let mut rng = Rng::new(42);
                move |_| rng.next_u64()
            })
            .collect();
        let b: Vec<u64> = (0..4)
            .map({
                let mut rng = Rng::new(42);
                move |_| rng.next_u64()
            })
            .collect();
        assert_eq!(a, b);
        assert_ne!(Rng::new(42).next_u64(), Rng::new(43).next_u64());
    }

    #[test]
    fn named_streams_differ() {
        assert_ne!(
            Rng::named(1, "doors").next_u64(),
            Rng::named(1, "items").next_u64()
        );
    }

    #[test]
    fn ranges_stay_in_bounds() {
        let mut rng = Rng::new(7);
        for _ in 0..10_000 {
            let n = rng.next_in(-3, 5);
            assert!((-3..5).contains(&n));
            let f = rng.next_f64();
            assert!((0.0..1.0).contains(&f));
        }
        assert_eq!(rng.next_in(5, 5), 5);
        assert_eq!(rng.next_in(5, 2), 5);
    }

    #[test]
    fn shuffle_keeps_every_element() {
        let mut rng = Rng::new(3);
        let mut values: Vec<u32> = (0..50).collect();
        rng.shuffle(&mut values);
        let mut sorted = values.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..50).collect::<Vec<_>>());
        assert_ne!(values, sorted);
    }

    #[test]
    fn table_skips_zero_weights_and_respects_weights() {
        let mut rng = Rng::new(9);
        let mut table = Table::new();
        table.add('a', 0.0);
        table.add('b', 1.0);
        table.add('c', 3.0);
        let mut counts = [0; 3];
        for _ in 0..4000 {
            match table.next(&mut rng).unwrap() {
                'a' => counts[0] += 1,
                'b' => counts[1] += 1,
                _ => counts[2] += 1,
            }
        }
        assert_eq!(counts[0], 0);
        assert!(counts[2] > counts[1] * 2);
        assert!(Table::<u8>::new().next(&mut rng).is_none());
    }

    #[test]
    fn endless_bag_hands_out_each_item_once_per_round() {
        let mut rng = Rng::new(5);
        let mut bag = EndlessBag::new(vec![1, 2, 3]);
        let mut round: Vec<i32> = (0..3).map(|_| bag.next(&mut rng).unwrap()).collect();
        round.sort_unstable();
        assert_eq!(round, [1, 2, 3]);
        assert!(bag.next(&mut rng).is_some());
        assert!(EndlessBag::<u8>::new(vec![]).next(&mut rng).is_none());
    }
}
