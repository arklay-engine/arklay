//! BioRand's RE1 randomizer, ported from the `staging` branch of
//! `biorand/classic`. The routing of `biorand/common` it places keys with is
//! its own crate, `arklay_routing`, as common is in the C#.
//!
//! The package is a pure library. Its input is the map (each room's
//! `random.xml` and the map-wide `biorand/map.xml`, read from a pack by the
//! engine crate), a configuration and a seed; its output is the plan of a
//! generated game. Applying that plan to a pack is the engine crate's job.
//!
//! `README.md` documents the commands, the generated pack and the room
//! script guards.

pub mod audit;
pub mod config;
pub mod doors;
pub mod enemies;
pub mod inventory;
pub mod items;
pub mod keys;
pub mod locks;
pub mod map;
pub mod plan;
pub mod prepare;
pub mod rng;

pub use plan::{Plan, Players, generate};
