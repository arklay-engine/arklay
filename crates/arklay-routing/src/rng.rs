//! The random stream the finder draws its choices from.
//!
//! Common's `RouteFinder` takes a .NET `Random` and knows nothing of the
//! randomizer's own stream. The port does the same through [`RouteRng`], which
//! the randomizer implements for its stream.

/// A deterministic random stream: the same stream must give the same indices
/// for a route to be the same.
pub trait RouteRng {
    /// An index below `len`, which is never zero.
    fn index(&mut self, len: usize) -> usize;
}

/// Shuffles in place (Fisher–Yates), drawing from the end down as
/// `arklay_biorand::rng::Rng::shuffle` does, so the randomizer's routes are
/// the same as when the finder called that directly.
pub(crate) fn shuffle<T>(rng: &mut dyn RouteRng, values: &mut [T]) {
    for i in (1..values.len()).rev() {
        let j = rng.index(i + 1);
        values.swap(i, j);
    }
}
