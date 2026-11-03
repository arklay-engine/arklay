//! BioRand: the engine's side of generated games.
//!
//! The randomizer is the `arklay-biorand` package, a pure library that reads a
//! map and plans a game. Writing a plan into a pack is being ported (see
//! `docs/biorand/plan.md`), so [`generate`] and [`verify_pack`] refuse until it
//! lands. What stays here is what the launcher and the engine share: a seed's
//! pack id and path, the manifest fields a generated game records, and the
//! map-wide files that `convert-game` packs for the randomizer.
//!
//! The same seed and [`GENERATOR_VERSION`] always make the same bytes; bump the
//! version whenever a change would alter a seed's output.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

/// The version of the generator's output for a given seed.
pub const GENERATOR_VERSION: u32 = 1;

/// The base pack a game is generated from.
pub const BASE_PACK_ID: &str = "re1";

/// The largest seed: seeds are six digits.
pub const MAX_SEED: u32 = 999_999;
/// How many digits a seed can be written with.
pub const SEED_DIGITS: usize = 6;

mod checked_in {
    include!(concat!(env!("OUT_DIR"), "/biorand_files.rs"));
}

/// The checked-in map-wide BioRand files (`games/re1/biorand/*.xml`) as
/// `(pack entry, text)`. `convert-game` packs exactly these, beside the rooms'
/// `random.xml` files.
pub use checked_in::BIORAND_FILES;

/// The pack id of a seed's game.
pub fn pack_id(seed: u32) -> String {
    format!("biorand-{seed:06}")
}

/// A seed's game file in the games directory.
pub fn pack_path(games_dir: &Path, seed: u32) -> PathBuf {
    games_dir.join(format!("{}.akpak", pack_id(seed)))
}

/// A seed from the system clock, for the launcher to offer.
pub fn random_seed() -> u32 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.subsec_nanos());
    nanos.wrapping_mul(2_654_435_761) % (MAX_SEED + 1)
}

/// What generation and verification say until the randomizer is ported.
const NOT_PORTED: &str =
    "BioRand generation is being ported to the new randomizer (see docs/biorand/plan.md)";

/// Generate `seed`'s game pack from the pack at `base`, as `games_dir`'s
/// `biorand-<seed>.akpak`. Refuses until the port lands.
pub fn generate(
    _base: &Path,
    _seed: u32,
    _games_dir: &Path,
    _jobs: usize,
    _progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<PathBuf> {
    Err(anyhow!(NOT_PORTED))
}

/// Check a generated pack. Refuses until the port lands.
pub fn verify_pack(_path: &Path) -> Result<()> {
    Err(anyhow!(NOT_PORTED))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seed_names_its_pack() {
        assert_eq!(pack_id(42), "biorand-000042");
        assert_eq!(
            pack_path(Path::new("games"), 42),
            Path::new("games").join("biorand-000042.akpak")
        );
    }

    #[test]
    fn generation_and_verification_say_they_are_being_ported() {
        let quiet = |_: u64, _: u64| {};
        let error =
            generate(Path::new("re1.akpak"), 42, Path::new("games"), 0, &quiet).unwrap_err();
        assert_eq!(error.to_string(), NOT_PORTED);
        let error = verify_pack(Path::new("biorand-000042.akpak")).unwrap_err();
        assert_eq!(error.to_string(), NOT_PORTED);
    }

    #[test]
    fn the_map_files_are_checked_in_under_biorand() {
        assert!(
            BIORAND_FILES
                .iter()
                .any(|(entry, _)| *entry == "biorand/map.xml")
        );
    }
}
