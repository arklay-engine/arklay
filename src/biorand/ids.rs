//! Every game id the generator uses, named by its source (`re1:item:0x34`).
//!
//! A generated game keeps its source's ids for now. Going through this table
//! leaves one place to change once a pack mixes content from several games and
//! ids have to be assigned afresh.

/// A room's `next_room` byte: its stage in the top bits, always with the stage
/// so the door reads the same whichever room holds it.
pub const fn next_room(room: u16) -> u8 {
    (((room >> 8) as u8) << 5) | ((room & 0x1F) as u8)
}

/// The room a `next_room` byte names from a room in `stage`.
pub const fn room_of(next_room: u8, stage: u8) -> u16 {
    let target_stage = if next_room < 0x20 {
        stage
    } else {
        next_room >> 5
    };
    ((target_stage as u16) << 8) | (next_room & 0x1F) as u16
}

/// The keys a lock can take. The Sword Key is left out: the engine lets Jill's
/// lockpick stand in for it.
pub const GATE_KEYS: [u8; 3] = [0x34, 0x35, 0x36];

/// The lock flags gates use: bits no room script touches.
pub const GATE_LOCK_FLAGS: [u8; 3] = [0x38, 0x39, 0x3A];

/// The lock flag of a sealed door, which nothing ever raises.
pub const SEAL_LOCK_FLAG: u8 = 0x3E;

/// `re1:item:0x2A`, the flare that ends a game in the goal room.
pub const FLARE: u8 = 0x2A;

/// The enemy flags new spawns use: bits above any the rooms' scripts name.
pub const SPAWN_FLAGS: std::ops::RangeInclusive<u8> = 0xC0..=0xFE;

/// `(item, quantity)`, in the order slots get them once the keys are placed.
pub const SUPPLIES: [(u8, u8); 10] = [
    (0x03, 7),  // re1:item:0x03 shotgun
    (0x0B, 15), // re1:item:0x0B handgun ammo
    (0x44, 1),  // re1:item:0x44 green herb
    (0x0C, 7),  // re1:item:0x0C shotgun ammo
    (0x41, 1),  // re1:item:0x41 first aid spray
    (0x0B, 15),
    (0x43, 1), // re1:item:0x43 red herb
    (0x44, 1),
    (0x0C, 7),
    (0x45, 1), // re1:item:0x45 blue herb
];

/// An enemy type of the pool.
#[derive(Clone, Copy, Debug)]
pub struct EnemyType {
    pub id: u8,
    /// The behaviour the original most often gives this enemy standing free.
    pub behaviour: u8,
}

pub const ENEMIES: [EnemyType; 5] = [
    EnemyType {
        id: 0x00, // re1:enemy:0x00 groundskeeper zombie
        behaviour: 0x00,
    },
    EnemyType {
        id: 0x01, // re1:enemy:0x01 naked zombie
        behaviour: 0x00,
    },
    EnemyType {
        id: 0x02, // re1:enemy:0x02 cerberus
        behaviour: 0x00,
    },
    EnemyType {
        id: 0x03, // re1:enemy:0x03 web spinner
        behaviour: 0x02,
    },
    EnemyType {
        id: 0x06, // re1:enemy:0x06 hunter
        behaviour: 0x00,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_bytes_round_trip() {
        for room in [0x118, 0x21A, 0x303, 0x100] {
            assert_eq!(room_of(next_room(room), 0), room);
        }
        assert_eq!(room_of(0x1A, 2), 0x21A);
    }
}
