//! BioRand's RE1 randomizer, ported from the `staging` branch of
//! `biorand/classic` and the routing of `biorand/common`.
//!
//! The package is a pure library. Its input is the map (each room's
//! `random.xml` and the map-wide `biorand/map.xml`, read from a pack by the
//! engine crate), a configuration and a seed; its output is the plan of a
//! generated game. Applying that plan to a pack is the engine crate's job.
//!
//! See `docs/biorand/plan.md` in the repository.

pub mod config;
pub mod enemies;
pub mod inventory;
pub mod items;
pub mod keys;
pub mod map;
pub mod prepare;
pub mod rng;
pub mod routing;
