//! A small growable bit set, the route finder's sets of nodes and edges.
//!
//! Its iteration is in index order. Edge ids are ordered by (source,
//! destination), so iterating a set of edges gives the order common sorts
//! them into before it draws anything from its random stream.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BitSet {
    words: Vec<u64>,
}

impl BitSet {
    pub fn contains(&self, index: usize) -> bool {
        self.words
            .get(index / 64)
            .is_some_and(|word| word & (1 << (index % 64)) != 0)
    }

    /// Adds `index`, returning whether it was absent.
    pub fn insert(&mut self, index: usize) -> bool {
        let word = index / 64;
        if word >= self.words.len() {
            self.words.resize(word + 1, 0);
        }
        let bit = 1 << (index % 64);
        let absent = self.words[word] & bit == 0;
        self.words[word] |= bit;
        absent
    }

    /// Removes `index`, returning whether it was present.
    pub fn remove(&mut self, index: usize) -> bool {
        let Some(word) = self.words.get_mut(index / 64) else {
            return false;
        };
        let bit = 1 << (index % 64);
        let present = *word & bit != 0;
        *word &= !bit;
        present
    }

    pub fn union_with(&mut self, other: &BitSet) {
        if other.words.len() > self.words.len() {
            self.words.resize(other.words.len(), 0);
        }
        for (word, other) in self.words.iter_mut().zip(&other.words) {
            *word |= other;
        }
    }

    pub fn clear(&mut self) {
        self.words.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|&word| word == 0)
    }

    pub fn len(&self) -> usize {
        self.words
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum()
    }

    /// The members in ascending order.
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.words.iter().enumerate().flat_map(|(i, &word)| {
            let mut rest = word;
            std::iter::from_fn(move || {
                if rest == 0 {
                    return None;
                }
                let bit = rest.trailing_zeros() as usize;
                rest &= rest - 1;
                Some(i * 64 + bit)
            })
        })
    }
}

impl FromIterator<usize> for BitSet {
    fn from_iter<I: IntoIterator<Item = usize>>(iter: I) -> Self {
        let mut set = BitSet::default();
        for index in iter {
            set.insert(index);
        }
        set
    }
}

#[cfg(test)]
mod tests {
    use super::BitSet;

    #[test]
    fn inserts_removes_and_iterates_in_order() {
        let mut set = BitSet::default();
        assert!(set.insert(130));
        assert!(set.insert(3));
        assert!(!set.insert(3));
        assert!(set.insert(64));
        assert_eq!(set.iter().collect::<Vec<_>>(), [3, 64, 130]);
        assert_eq!(set.len(), 3);
        assert!(set.remove(64));
        assert!(!set.remove(64));
        assert!(!set.remove(1000));
        assert!(!set.contains(64));
        assert!(set.contains(130));
        let other: BitSet = [1, 200].into_iter().collect();
        set.union_with(&other);
        assert_eq!(set.iter().collect::<Vec<_>>(), [1, 3, 130, 200]);
        set.clear();
        assert!(set.is_empty());
    }
}
