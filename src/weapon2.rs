//! RE2's shots: `Em_ck`, `hit_scan_ck` and `Weapon_at_ck`.
//!
//! The player's routine finds its target here, from a view of the enemy slots
//! taken before it ran, and leaves a [`Hit`] for `zombie2` to take. The
//! instant hits ported are the handguns', the magnums', the shotguns', the
//! Colt S.A.A.'s and the Beretta's; the launchers' and the bowgun's are their
//! effects'.

use crate::player2::rotate;
use crate::sca::square_root0;
use crate::state::ScaRect;
use crate::zombie2::sca_ck_line;

/// What `Sca_ck_line` is asked for in both.
const LINE_MASK: u16 = 0x80;
const COLT: u8 = crate::player2::COLT;
const BERETTA: u8 = crate::player2::BERETTA;
const SMG: u8 = crate::player2::SMG;
const GATLING: u8 = crate::player2::GATLING;
const SPARK: u8 = crate::player2::SPARK;
/// `Type`'s bits for out of reach, and for someone only aimed at if nobody else is.
const TYPE_UNTARGETED: u16 = 0xC000;
const TYPE_ON_FLOOR: u16 = 0x2000;

/// What the player's routine sees of an enemy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Target {
    pub slot: usize,
    pub id: u8,
    pub be_flg: u32,
    pub kind: u16,
    pub life: i16,
    pub damage_cnt: u8,
    pub pos: [i32; 3],
    /// `Atd[0]`'s `X` and `Z`, and its `At_w`.
    pub at: [i32; 2],
    /// `Atd[0]`'s `Y`, where the spark shot's strike (`Esp_0E`) ends.
    pub at_y: i32,
    pub at_w: u16,
    /// `Atd[1]`'s `At_w`.
    pub at_w1: u16,
    /// `Workm.t[1]` of its neck.
    pub neck_y: i32,
    /// `Atd[0]`'s `Ofs_y` (`+0x98`) and `At_h` (`+0x9e`), and `Cdir_y`, for
    /// what a blast hits.
    pub ofs_y: i16,
    pub at_h: u16,
    pub angle: u16,
}

impl Target {
    fn in_play(&self) -> bool {
        self.be_flg & 1 != 0
    }
}

/// What `Weapon_at_ck` does to the enemy it hits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    pub slot: usize,
    /// The body's id, whose `Wp_dmg_tbl` entry the damage came from.
    pub id: u8,
    /// `Routine_1`: the player's `Type`.
    pub weapon: u8,
    /// The player's `Be_flg` sign: the shot was aimed up.
    pub high: bool,
    /// Which record of the shot's three: the damage word's field.
    pub band: u8,
    /// Which of `D_life_d`, `D_life_c` and `D_life_u`.
    pub region: u8,
    pub damage: i16,
    /// What the region's count loses.
    pub stun: u8,
    /// `Damage_cnt`'s new count.
    pub count: u8,
    /// `Damage_flg`'s bits for what the shot crossed.
    pub flags: u16,
}

/// `Em_ck`: the slot to aim at, the nearest in view, but one on the floor only
/// when there's nobody else. A `range` of 0 is no limit.
pub fn em_ck(
    targets: &[Target],
    sca: &[ScaRect],
    pos: [i32; 3],
    neck: [i32; 3],
    range: u32,
) -> Option<usize> {
    let mut standing = (if range == 0 { i32::MAX as u32 } else { range }, None);
    let mut floored = (i32::MAX as u32, None);
    for target in targets.iter().filter(|target| target.in_play()) {
        if target.kind & TYPE_UNTARGETED != 0 || target.life < 0 {
            continue;
        }
        let (dx, dz) = (pos[0] - target.pos[0], pos[2] - target.pos[2]);
        let distance = square_root0(dz.wrapping_mul(dz).wrapping_add(dx.wrapping_mul(dx))) as u32;
        let to = [target.at[0], target.neck_y, target.at[1]];
        if sca_ck_line(sca, neck, to, LINE_MASK) {
            continue;
        }
        let best = if target.kind & TYPE_ON_FLOOR == 0 {
            &mut standing
        } else {
            &mut floored
        };
        if distance < best.0 {
            *best = (distance, Some(target.slot));
        }
    }
    standing.1.or(floored.1)
}

/// `hit_scan_data` of a weapon: how far above and below the shoulder a shot is
/// level, and the shoulder's height by `pld_to_player_map`.
const fn hit_scan_data(weapon: u8) -> (i32, i32, [i32; 2]) {
    match weapon {
        5 | 6 => (-200, 300, [-2427, 0]),
        COLT => (-400, 30, [-1850, -1890]),
        SMG => (-400, 200, [-1894, -1809]),
        // The flamethrower's, and the launchers', have no shoulder.
        crate::player2::FLAMETHROWER => (-200, 300, [0, 0]),
        7 => (-500, 150, [-1813, 0]),
        8 => (-500, 200, [-1813, 0]),
        _ => (-200, 300, [-2427, -2120]),
    }
}

/// `hit_scan_ck`: `Be_flg`'s level of the shot, as `BYTE_0053a520` has it: 0 for
/// low, 1 level and 2 up. `y` is the hand's.
pub fn hit_scan_ck(id: u8, weapon: u8, pos_y: i32, parts0_y: i32, y: i32) -> usize {
    let (low, high, shoulders) = hit_scan_data(weapon);
    // Character 0xE's own shoulder with the Colt (`DAT_0053a3c4`) is -1850,
    // its even row's.
    let shoulder = ((shoulders[usize::from(id & 1)] + pos_y) * parts0_y) >> 12;
    if y < low + shoulder {
        2
    } else if high + shoulder < y {
        0
    } else {
        1
    }
}

/// A record of `At_tbl_top`: what each of its three rectangles may reach, and
/// `Attack_r_ck0`'s `x`, `z`, width and length of each.
struct Record {
    reach: [u8; 3],
    rects: [[i16; 4]; 3],
}

/// A record's three rectangles, from the reaches of each level below.
const fn record(reach: [u8; 3], rects: [[i16; 4]; 3]) -> Record {
    Record { reach, rects }
}

const REACH: [[u8; 3]; 3] = [[0x4, 0x2, 0x1], [0x8, 0x8, 0x8], [0x10, 0x20, 0x40]];

/// `At_tbl_top`'s handguns, by the shot's level.
const HANDGUN: [Record; 3] = [
    record(
        REACH[0],
        [
            [100, 0, 1000, 375],
            [4100, 0, 1000, 500],
            [8100, 0, 2500, 625],
        ],
    ),
    record(
        REACH[1],
        [
            [200, 0, 1000, 375],
            [4200, 0, 1000, 500],
            [8200, 0, 7500, 625],
        ],
    ),
    record(
        REACH[2],
        [
            [100, 0, 1000, 375],
            [4100, 0, 1000, 500],
            [8200, 0, 2500, 625],
        ],
    ),
];

/// The magnums'.
const MAGNUM: [Record; 3] = [
    record(
        REACH[0],
        [
            [100, 0, 1000, 125],
            [4100, 0, 1000, 150],
            [8100, 0, 2500, 175],
        ],
    ),
    record(
        REACH[1],
        [
            [200, 0, 1000, 125],
            [4200, 0, 1000, 150],
            [7200, 0, 7500, 175],
        ],
    ),
    record(
        REACH[2],
        [
            [100, 0, 1000, 125],
            [4100, 0, 1000, 150],
            [8100, 0, 2500, 175],
        ],
    ),
];

/// The shotgun's.
const SHOTGUN: [Record; 3] = [
    record(
        REACH[0],
        [
            [100, 0, 1000, 500],
            [4100, 0, 1000, 750],
            [8100, 0, 2500, 1000],
        ],
    ),
    record(
        REACH[1],
        [
            [200, 0, 1000, 625],
            [4200, 0, 1000, 875],
            [8200, 0, 7500, 1125],
        ],
    ),
    record(
        REACH[2],
        [
            [100, 0, 750, 500],
            [3100, 0, 1500, 750],
            [9100, 0, 2500, 1000],
        ],
    ),
];

/// The custom shotgun's.
const CUSTOM_SHOTGUN: [Record; 3] = [
    record(
        REACH[0],
        [
            [100, 0, 1000, 500],
            [4100, 0, 1000, 875],
            [8100, 0, 2500, 1125],
        ],
    ),
    record(
        REACH[1],
        [
            [200, 0, 1000, 625],
            [4200, 0, 1000, 1000],
            [8200, 0, 7500, 1250],
        ],
    ),
    record(
        REACH[2],
        [
            [100, 0, 750, 500],
            [3100, 0, 1000, 875],
            [7100, 0, 2500, 1125],
        ],
    ),
];

/// The spark shot's (`0x539CB0`): a rectangle at each level, which only
/// finds the body its effect strikes.
const SPARK_RECORDS: [Record; 3] = [
    record([0x4, 0, 0], [[200, 0, 3500, 500], [0; 4], [0; 4]]),
    record([0x8, 0, 0], [[200, 0, 3500, 500], [0; 4], [0; 4]]),
    record([0x10, 0, 0], [[200, 0, 3500, 500], [0; 4], [0; 4]]),
];

/// The gatling gun's (`0x539D68`). `pl_mv05_w5_t2` only ever looks with
/// the level's, one long rectangle of 7500.
const GATLING_RECORDS: [Record; 3] = [
    record(
        REACH[0],
        [
            [100, 0, 1000, 500],
            [4100, 0, 1000, 625],
            [8100, 0, 2500, 625],
        ],
    ),
    record([0x8, 0x4, 0x10], [[200, 0, 7500, 500]; 3]),
    record(
        REACH[2],
        [
            [100, 0, 1000, 375],
            [4100, 0, 1000, 500],
            [8200, 0, 2500, 625],
        ],
    ),
];

/// What a weapon's shot is: its rectangles, and what it does to a zombie.
fn shot_records(weapon: u8) -> Option<&'static [Record; 3]> {
    let records = match weapon {
        SPARK => &SPARK_RECORDS,
        GATLING => &GATLING_RECORDS,
        // The Colt's, the sub-machine gun's and the Beretta's entries point at
        // the handguns' records.
        2..=4 | COLT | SMG | BERETTA => &HANDGUN,
        5 | 6 => &MAGNUM,
        7 => &SHOTGUN,
        8 => &CUSTOM_SHOTGUN,
        _ => return None,
    };
    Some(records)
}

/// `Wp_dmg_tbl`'s entry for the target.
fn damage_table(id: u8, weapon: u8) -> Option<&'static Damage> {
    // The Colt's and the Beretta's rows are the handgun's but for `Em_30`'s
    // Colt (`0x5369d0`'s fourteenth). `Em_3B`'s Beretta row has the lower
    // heights, but arklay has no `Em_3B`.
    let weapon = match (id, weapon) {
        (G1_ID, COLT) => return Some(&G1_COLT),
        (_, SMG) => return smg_damage(id),
        (_, GATLING) => return gatling_damage(id),
        (_, SPARK) => return spark_damage(id),
        (_, COLT | BERETTA) => 2,
        _ => weapon,
    };
    let table = match id {
        LICKER_ID => &LICKER_DAMAGE,
        LICKER_24_ID => &LICKER_24_DAMAGE,
        DOG_ID => &DOG_DAMAGE,
        CROW_ID => &CROW_DAMAGE,
        MRX_ID => &MRX_DAMAGE,
        G1_ID => &G1_DAMAGE,
        G2_ID => &G2_DAMAGE,
        G3_ID => &G3_DAMAGE,
        crate::zombie2::EMBRYO_ID | crate::zombie2::ADULT_ID => &ADULT_DAMAGE,
        // `Em_26` points at `Em_25`'s rows.
        SPIDER_ID | YOUNG_ID => &SPIDER_DAMAGE,
        ALLIGATOR_ID => &ALLIGATOR_DAMAGE,
        IVY_ID | VINE_ID | POISON_IVY_ID => &PLANT_DAMAGE,
        _ => &DAMAGE,
    };
    table.get(usize::from(weapon).checked_sub(2)?)
}

/// `Wp_dmg_tbl`'s entry for a weapon on a zombie: the damages by band, in ten
/// bits each; what the region loses in three bits each from the bottom, and
/// `Damage_cnt` in seven from the ninth; and the heights `Set_MinMax` reads.
#[derive(Clone, Copy)]
struct Damage {
    damage: u32,
    stun: u32,
    heights: [i32; 6],
}

const HANDGUN_HEIGHTS: [i32; 6] = [-5000, -2000, -3000, 2000, -500, 3000];
const SHOTGUN_HEIGHTS: [i32; 6] = [-5000, 500, -3000, 2000, -500, 3000];
/// `Type & 0xFFF` of 2 to 8.
const DAMAGE: [Damage; 7] = [
    Damage {
        damage: 0x00E0_3C10,
        stun: 0x0285_0A14,
        heights: HANDGUN_HEIGHTS,
    },
    Damage {
        damage: 0x00E0_3C10,
        stun: 0x0285_0A14,
        heights: HANDGUN_HEIGHTS,
    },
    Damage {
        damage: 0x00E0_3C10,
        stun: 0x0081_0214,
        heights: HANDGUN_HEIGHTS,
    },
    Damage {
        damage: 0x384E_1384,
        stun: 0x078F_1E0A,
        heights: HANDGUN_HEIGHTS,
    },
    Damage {
        damage: 0x384E_1384,
        stun: 0x078F_1E0A,
        heights: HANDGUN_HEIGHTS,
    },
    Damage {
        damage: 0x0280_F0C8,
        stun: 0x078F_1E0A,
        heights: SHOTGUN_HEIGHTS,
    },
    Damage {
        damage: 0x03C1_412C,
        stun: 0x078F_1E0A,
        heights: SHOTGUN_HEIGHTS,
    },
];

const CROW_ID: u8 = crate::zombie2::CROW_ID;
const CROW_HEIGHTS: [i32; 6] = [-6000, -3000, -4000, -500, -500, 3000];
/// The same rows for `Em_21`, `Em_21_wp_tbl`.
const CROW_DAMAGE: [Damage; 7] = [
    Damage {
        damage: 0x00F0_3C0F,
        stun: 0x078F_1E0A,
        heights: CROW_HEIGHTS,
    },
    Damage {
        damage: 0x00F0_3C0F,
        stun: 0x078F_1E0A,
        heights: CROW_HEIGHTS,
    },
    Damage {
        damage: 0x00F0_3C0F,
        stun: 0x078F_1E0A,
        heights: CROW_HEIGHTS,
    },
    Damage {
        damage: 0x0C83_20C8,
        stun: 0x078F_1E0A,
        heights: CROW_HEIGHTS,
    },
    Damage {
        damage: 0x12C4_B12C,
        stun: 0x078F_1E0A,
        heights: CROW_HEIGHTS,
    },
    Damage {
        damage: 0x00F0_503C,
        stun: 0x078F_1E0A,
        heights: CROW_HEIGHTS,
    },
    Damage {
        damage: 0x01E0_A078,
        stun: 0x078F_1E0A,
        heights: CROW_HEIGHTS,
    },
];

const MRX_ID: u8 = crate::zombie2::MRX_ID;
const MRX_HANDGUN: [i32; 6] = [-4000, 100, -3000, 2400, -1000, 2600];
const MRX_SHOTGUN: [i32; 6] = [-4500, 800, -3000, 2400, -1000, 2600];
/// The same rows for `Em_2A`.
const MRX_DAMAGE: [Damage; 7] = [
    Damage {
        damage: 0x0100_4010,
        stun: 0x078F_1E0A,
        heights: MRX_HANDGUN,
    },
    Damage {
        damage: 0x0100_4010,
        stun: 0x078F_1E0A,
        heights: MRX_HANDGUN,
    },
    Damage {
        damage: 0x00F0_3C0F,
        stun: 0x0000_000A,
        heights: MRX_HANDGUN,
    },
    Damage {
        damage: 0x04B1_3851,
        stun: 0x078F_1E0A,
        heights: MRX_HANDGUN,
    },
    Damage {
        damage: 0x0641_9064,
        stun: 0x078F_1E0A,
        heights: MRX_HANDGUN,
    },
    Damage {
        damage: 0x0280_B432,
        stun: 0x078F_1E0A,
        heights: MRX_SHOTGUN,
    },
    Damage {
        damage: 0x0461_2C50,
        stun: 0x078F_1FB4,
        heights: MRX_SHOTGUN,
    },
];

const G1_ID: u8 = crate::zombie2::G1_ID;
/// The same rows for `Em_30` (`0x5369d0`).
const G1_DAMAGE: [Damage; 7] = [
    Damage {
        damage: 0x00E0_3C10,
        stun: 0x0285_0A14,
        heights: HANDGUN_HEIGHTS,
    },
    Damage {
        damage: 0x00E0_3C10,
        stun: 0x0285_0A14,
        heights: HANDGUN_HEIGHTS,
    },
    Damage {
        damage: 0x00E0_3C10,
        stun: 0x0285_0A14,
        heights: HANDGUN_HEIGHTS,
    },
    Damage {
        damage: 0x0551_605A,
        stun: 0x078F_1E0A,
        heights: HANDGUN_HEIGHTS,
    },
    Damage {
        damage: 0x0641_9064,
        stun: 0x078F_1E0A,
        heights: HANDGUN_HEIGHTS,
    },
    Damage {
        damage: 0x0330_DC3C,
        stun: 0x078F_1E0A,
        heights: SHOTGUN_HEIGHTS,
    },
    Damage {
        damage: 0x0501_545A,
        stun: 0x078F_1E0A,
        heights: SHOTGUN_HEIGHTS,
    },
];

/// `Wp_dmg_tbl`'s sub-machine gun rows (each table's fifteenth).
fn smg_damage(id: u8) -> Option<&'static Damage> {
    const fn row(damage: u32, stun: u32, heights: [i32; 6]) -> Damage {
        Damage {
            damage,
            stun,
            heights,
        }
    }
    const LICKER_HEIGHTS: [i32; 6] = [-5000, -2000, -3500, 500, -500, 2000];
    const ZOMBIE: Damage = row(0x0040_1004, 0x0081_020A, SHOTGUN_HEIGHTS);
    const LICKER: Damage = row(0x00B0_2C0B, 0x0285_0A0A, LICKER_HEIGHTS);
    const DOG: Damage = row(
        0x00A0_280A,
        0x0183_060A,
        [-5000, 50, -2000, 2000, -500, 3000],
    );
    const CROW: Damage = row(
        0x00F0_3C0F,
        0x078F_1E0A,
        [-6000, -3000, -4000, -500, -500, 3000],
    );
    const MRX: Damage = row(
        0x00D0_340D,
        0x0489_120A,
        [-3200, 100, -1000, 1600, -1000, 2600],
    );
    const G1: Damage = row(0x0080_2008, 0x0285_0A0A, SHOTGUN_HEIGHTS);
    const G2: Damage = row(
        0x00A0_280A,
        0x0285_0A0A,
        [-5000, 500, -6000, 2000, -500, 3000],
    );
    const G3: Damage = row(0x00C0_300C, 0x0408_100A, SHOTGUN_HEIGHTS);
    const SPIDER: Damage = row(
        0x00A0_280A,
        0x0285_0A0A,
        [-7000, -2000, -4200, 500, -500, 2000],
    );
    const ALLIGATOR: Damage = row(0x00A0_280A, 0x050A_140A, SHOTGUN_HEIGHTS);
    const PLANT: Damage = row(
        0x0050_1405,
        0x0306_0C0A,
        [-3000, 500, -3000, 2000, -500, 3000],
    );
    // `0x535F50`, `0x536250` and `0x5363D0`'s fifteenth rows.
    const EMBRYO: Damage = row(0x00A0_280A, 0x0285_0A0A, HANDGUN_HEIGHTS);
    const ROACH: Damage = row(
        0x0070_1C07,
        0x0081_020A,
        [-4000, -2800, -3100, -1200, -1200, 100],
    );
    const EM2D: Damage = row(
        0x0070_1C07,
        0x0081_020A,
        [-3000, -2000, -3000, 2000, -500, 3000],
    );
    Some(match id {
        LICKER_ID | LICKER_24_ID => &LICKER,
        DOG_ID => &DOG,
        CROW_ID => &CROW,
        MRX_ID => &MRX,
        G1_ID => &G1,
        G2_ID => &G2,
        G3_ID => &G3,
        SPIDER_ID | YOUNG_ID => &SPIDER,
        ALLIGATOR_ID => &ALLIGATOR,
        IVY_ID | VINE_ID | POISON_IVY_ID => &PLANT,
        crate::zombie2::EMBRYO_ID => &EMBRYO,
        crate::zombie2::ROACH_ID => &ROACH,
        crate::zombie2::EM2D_ID => &EM2D,
        // The zombies' two tables (`0x535350`, `0x5354D0`) share the row.
        _ => &ZOMBIE,
    })
}

/// `Wp_dmg_tbl`'s gatling gun rows (each table's eighteenth).
fn gatling_damage(id: u8) -> Option<&'static Damage> {
    const fn row(damage: u32, stun: u32, heights: [i32; 6]) -> Damage {
        Damage {
            damage,
            stun,
            heights,
        }
    }
    const LICKER_HEIGHTS: [i32; 6] = [-5000, -2000, -3500, 500, -500, 2000];
    const ZOMBIE: Damage = row(0x0080_2008, 0x0102_040A, SHOTGUN_HEIGHTS);
    const LICKER: Damage = row(0x0140_5014, 0x0285_0A0A, LICKER_HEIGHTS);
    const DOG: Damage = row(
        0x0120_4812,
        0x0183_060A,
        [-5000, 50, -2000, 2000, -500, 3000],
    );
    const CROW: Damage = row(
        0x01E0_781E,
        0x078F_1E0A,
        [-6000, -3000, -4000, -500, -500, 3000],
    );
    const MRX: Damage = row(
        0x0180_6018,
        0x0489_120A,
        [-3200, 100, -1000, 1600, -1000, 2600],
    );
    const G1: Damage = row(0x0100_4010, 0x0285_0A0A, SHOTGUN_HEIGHTS);
    const G2: Damage = row(
        0x0120_4812,
        0x0285_0A0A,
        [-5000, 500, -6000, 2000, -500, 3000],
    );
    const G3: Damage = row(0x0140_5014, 0x0387_0E0A, SHOTGUN_HEIGHTS);
    // The spider's row has -5000 where its sub-machine gun row has -7000.
    const SPIDER: Damage = row(
        0x0120_4812,
        0x0285_0A0A,
        [-5000, -2000, -4200, 500, -500, 2000],
    );
    const ALLIGATOR: Damage = row(0x0120_4812, 0x050A_140A, SHOTGUN_HEIGHTS);
    const PLANT: Damage = row(
        0x00A0_280A,
        0x0306_0C0A,
        [-3000, 500, -3000, 2000, -500, 3000],
    );
    const EMBRYO: Damage = row(
        0x0110_4411,
        0x0285_0A0A,
        [-5000, -2000, -3000, 2000, -500, 3000],
    );
    const EM28: Damage = row(0x0110_4411, 0x0285_0A0A, SHOTGUN_HEIGHTS);
    const ROACH: Damage = row(
        0x0070_1C07,
        0x0081_020A,
        [-4000, -2800, -3100, -1200, -1200, 100],
    );
    const EM2D: Damage = row(
        0x00E0_380E,
        0x0081_020A,
        [-3000, -2000, -3000, 2000, -500, 3000],
    );
    Some(match id {
        LICKER_ID | LICKER_24_ID => &LICKER,
        DOG_ID => &DOG,
        CROW_ID => &CROW,
        MRX_ID => &MRX,
        G1_ID => &G1,
        G2_ID => &G2,
        G3_ID => &G3,
        SPIDER_ID | YOUNG_ID => &SPIDER,
        ALLIGATOR_ID => &ALLIGATOR,
        IVY_ID | VINE_ID | POISON_IVY_ID => &PLANT,
        crate::zombie2::EMBRYO_ID => &EMBRYO,
        0x28 => &EM28,
        crate::zombie2::ROACH_ID => &ROACH,
        crate::zombie2::EM2D_ID => &EM2D,
        // The zombies' two tables (`0x535350`, `0x5354D0`) share the row.
        _ => &ZOMBIE,
    })
}

/// The heights of `Wp_dmg_tbl`'s fourteenth rows, the spark shot's: its look
/// for a body (`Weapon_at_ck` with `0x8000`) reads only them, and hurts
/// nothing, so the damage and stun are left out. Its effect's strike reads
/// the rows' damage through [`heavy_rows`].
fn spark_damage(id: u8) -> Option<&'static Damage> {
    const fn row(heights: [i32; 6]) -> Damage {
        Damage {
            damage: 0,
            stun: 0,
            heights,
        }
    }
    const SPARK_HEIGHTS: [i32; 6] = [-4000, -2000, -3000, 2000, -500, 3000];
    const ZOMBIE: Damage = row(SPARK_HEIGHTS);
    const DOG: Damage = row([-3000, -2000, -2000, -1000, -1000, 3000]);
    const CROW: Damage = row([-6000, -3000, -4000, -500, -500, 3000]);
    const LICKER: Damage = row([-5000, -2000, -3500, 500, -500, 2000]);
    const SPIDER: Damage = row([-7000, -2000, -6000, 6000, -500, 500]);
    const MRX: Damage = row([-3200, 100, -2000, 1600, -1000, 2600]);
    const G2: Damage = row([-4000, -2000, -6000, 2000, -500, 3000]);
    Some(match id {
        DOG_ID | crate::zombie2::ROACH_ID => &DOG,
        CROW_ID => &CROW,
        LICKER_ID | LICKER_24_ID => &LICKER,
        SPIDER_ID | YOUNG_ID => &SPIDER,
        MRX_ID => &MRX,
        G2_ID => &G2,
        // The zombies', the alligator's, the embryo's, room 205's bodies',
        // the plants', G1's and G3's.
        _ => &ZOMBIE,
    })
}

/// `Em_30`'s Colt row, harder than its handgun's.
const G1_COLT: Damage = Damage {
    damage: 0x00F0_4011,
    stun: 0x0285_0A14,
    heights: HANDGUN_HEIGHTS,
};

const G2_ID: u8 = crate::zombie2::G2_ID;
const G2_HANDGUN: [i32; 6] = [-5000, -2000, -6000, 2000, -500, 3000];
const G2_SHOTGUN: [i32; 6] = [-5000, 500, -6000, 2000, -500, 3000];
/// The same rows for `Em_31` (`0x536b50`).
const G2_DAMAGE: [Damage; 7] = [
    Damage {
        damage: 0x0130_5015,
        stun: 0x0285_0A14,
        heights: G2_HANDGUN,
    },
    Damage {
        damage: 0x0130_5015,
        stun: 0x0285_0A14,
        heights: G2_HANDGUN,
    },
    Damage {
        damage: 0x0130_5015,
        stun: 0x0000_0014,
        heights: G2_HANDGUN,
    },
    Damage {
        damage: 0x0551_605A,
        stun: 0x078F_1E0A,
        heights: G2_HANDGUN,
    },
    Damage {
        damage: 0x0691_A469,
        stun: 0x078F_1E0A,
        heights: G2_HANDGUN,
    },
    Damage {
        damage: 0x03C1_0446,
        stun: 0x078F_1E0A,
        heights: G2_SHOTGUN,
    },
    Damage {
        damage: 0x0501_545A,
        stun: 0x078F_1E0A,
        heights: G2_SHOTGUN,
    },
];

/// The G embryos' and the G adult's rows, `Em_27`'s (`0x535f50`) and `Em_28`'s (`0x5360d0`), the same: every weapon's
/// heights are the handgun's.
const ADULT_DAMAGE: [Damage; 7] = [
    Damage {
        damage: 0x0120_4C14,
        stun: 0x0285_0A14,
        heights: HANDGUN_HEIGHTS,
    },
    Damage {
        damage: 0x0120_4C14,
        stun: 0x0285_0A14,
        heights: HANDGUN_HEIGHTS,
    },
    Damage {
        damage: 0x0120_4C14,
        stun: 0x0285_0A14,
        heights: HANDGUN_HEIGHTS,
    },
    Damage {
        damage: 0x05F1_9069,
        stun: 0x078F_1E0A,
        heights: HANDGUN_HEIGHTS,
    },
    Damage {
        damage: 0x0781_E078,
        stun: 0x078F_1E0A,
        heights: HANDGUN_HEIGHTS,
    },
    Damage {
        damage: 0x0370_F041,
        stun: 0x078F_1E0A,
        heights: HANDGUN_HEIGHTS,
    },
    Damage {
        damage: 0x0461_2C50,
        stun: 0x078F_1E0A,
        heights: HANDGUN_HEIGHTS,
    },
];

const G3_ID: u8 = crate::zombie2::G3_ID;
const G3_HANDGUN: [i32; 6] = [-5000, -2000, -3000, 2000, -500, 3000];
const G3_SHOTGUN: [i32; 6] = [-5000, 500, -3000, 2000, -500, 3000];
/// The same rows for `Em_33` (`0x536cd0`).
const G3_DAMAGE: [Damage; 7] = [
    Damage {
        damage: 0x0190_681B,
        stun: 0x0285_0A14,
        heights: G3_HANDGUN,
    },
    Damage {
        damage: 0x0190_681B,
        stun: 0x0285_0A14,
        heights: G3_HANDGUN,
    },
    Damage {
        damage: 0x0190_681B,
        stun: 0x0000_0014,
        heights: G3_HANDGUN,
    },
    Damage {
        damage: 0x05F1_8865,
        stun: 0x078F_1E0A,
        heights: G3_HANDGUN,
    },
    Damage {
        damage: 0x0781_E078,
        stun: 0x078F_1E0A,
        heights: G3_HANDGUN,
    },
    Damage {
        damage: 0x03C1_0446,
        stun: 0x078F_1E0A,
        heights: G3_SHOTGUN,
    },
    Damage {
        damage: 0x05A1_7C64,
        stun: 0x078F_1E0A,
        heights: G3_SHOTGUN,
    },
];

const IVY_ID: u8 = crate::zombie2::IVY_ID;
const VINE_ID: u8 = crate::zombie2::VINE_ID;
const POISON_IVY_ID: u8 = crate::zombie2::POISON_IVY_ID;
const PLANT_HANDGUN: [i32; 6] = [-4000, -2000, -3000, 2000, -500, 3000];
const PLANT_SHOTGUN: [i32; 6] = [-3000, 500, -3000, 2000, -500, 3000];
/// The same rows for the plants, `Em_2E` and `Em_2F` (`0x536850`).
const PLANT_DAMAGE: [Damage; 7] = [
    Damage {
        damage: 0x0070_1C07,
        stun: 0x0285_0A14,
        heights: PLANT_HANDGUN,
    },
    Damage {
        damage: 0x0070_1C07,
        stun: 0x0285_0A14,
        heights: PLANT_HANDGUN,
    },
    Damage {
        damage: 0x0070_1C07,
        stun: 0x0081_0214,
        heights: PLANT_HANDGUN,
    },
    Damage {
        damage: 0x02D0_C032,
        stun: 0x078F_1E0A,
        heights: PLANT_HANDGUN,
    },
    Damage {
        damage: 0x0641_9064,
        stun: 0x078F_1E0A,
        heights: PLANT_HANDGUN,
    },
    Damage {
        damage: 0x01C0_741E,
        stun: 0x078F_1E0A,
        heights: PLANT_SHOTGUN,
    },
    Damage {
        damage: 0x03A0_EC3C,
        stun: 0x078F_1E0A,
        heights: PLANT_SHOTGUN,
    },
];

const SPIDER_ID: u8 = crate::zombie2::SPIDER_ID;
const ALLIGATOR_ID: u8 = crate::zombie2::ALLIGATOR_ID;
/// The same rows for `Em_23`, `Em_23_wp_tbl` (`0x535C50`): a few units of
/// its 25300 a shot.
const ALLIGATOR_DAMAGE: [Damage; 7] = [
    Damage {
        damage: 0x00F0_4011,
        stun: 0x0285_0A14,
        heights: HANDGUN_HEIGHTS,
    },
    Damage {
        damage: 0x00F0_4011,
        stun: 0x0285_0A14,
        heights: HANDGUN_HEIGHTS,
    },
    Damage {
        damage: 0x00F0_4011,
        stun: 0x0102_0414,
        heights: HANDGUN_HEIGHTS,
    },
    Damage {
        damage: 0x01F0_7C1F,
        stun: 0x078F_1E0A,
        heights: HANDGUN_HEIGHTS,
    },
    Damage {
        damage: 0x0320_C832,
        stun: 0x078F_1E0A,
        heights: HANDGUN_HEIGHTS,
    },
    Damage {
        damage: 0x0180_641B,
        stun: 0x078F_1E0A,
        heights: SHOTGUN_HEIGHTS,
    },
    Damage {
        damage: 0x0280_B432,
        stun: 0x078F_1E0A,
        heights: SHOTGUN_HEIGHTS,
    },
];
const YOUNG_ID: u8 = crate::zombie2::YOUNG_ID;
const SPIDER_HEIGHTS: [i32; 6] = [-7000, -2000, -4200, 500, -500, 2000];
/// The same rows for `Em_25` and `Em_26` (`0x535dd0`).
const SPIDER_DAMAGE: [Damage; 7] = [
    Damage {
        damage: 0x00F0_4011,
        stun: 0x078F_1E0A,
        heights: SPIDER_HEIGHTS,
    },
    Damage {
        damage: 0x00F0_4011,
        stun: 0x078F_1E0A,
        heights: SPIDER_HEIGHTS,
    },
    Damage {
        damage: 0x00F0_4011,
        stun: 0x0000_000A,
        heights: SPIDER_HEIGHTS,
    },
    Damage {
        damage: 0x0822_0882,
        stun: 0x078F_1E0A,
        heights: SPIDER_HEIGHTS,
    },
    Damage {
        damage: 0x0962_5896,
        stun: 0x078F_1E0A,
        heights: SPIDER_HEIGHTS,
    },
    Damage {
        damage: 0x0280_B437,
        stun: 0x078F_1E0A,
        heights: SPIDER_HEIGHTS,
    },
    Damage {
        damage: 0x05A1_9082,
        stun: 0x078F_1E0A,
        heights: SPIDER_HEIGHTS,
    },
];

const DOG_ID: u8 = crate::zombie2::DOG_ID;
const DOG_HANDGUN: [i32; 6] = [-4000, -2000, -3000, 2000, -500, 3000];
const DOG_SHOTGUN: [i32; 6] = [-3000, 500, -3000, 2000, -500, 3000];
/// The same rows for `Em_20`.
const DOG_DAMAGE: [Damage; 7] = [
    Damage {
        damage: 0x00F0_4012,
        stun: 0x078F_1E0A,
        heights: DOG_HANDGUN,
    },
    Damage {
        damage: 0x00F0_4012,
        stun: 0x078F_1E0A,
        heights: DOG_HANDGUN,
    },
    Damage {
        damage: 0x00F0_4012,
        stun: 0x0000_000A,
        heights: DOG_HANDGUN,
    },
    Damage {
        damage: 0x0C83_20C8,
        stun: 0x078F_1E0A,
        heights: DOG_HANDGUN,
    },
    Damage {
        damage: 0x12C4_B12C,
        stun: 0x078F_1E0A,
        heights: DOG_HANDGUN,
    },
    Damage {
        damage: 0x01E0_A03B,
        stun: 0x078F_1E0A,
        heights: DOG_SHOTGUN,
    },
    Damage {
        damage: 0x0411_2D2C,
        stun: 0x078F_1E0A,
        heights: DOG_SHOTGUN,
    },
];
const LICKER_ID: u8 = 0x22;
const LICKER_HEIGHTS: [i32; 6] = [-5000, -2000, -3500, 500, -500, 2000];
/// The same rows for `Em_22`.
const LICKER_DAMAGE: [Damage; 7] = [
    Damage {
        damage: 0x00C0_340E,
        stun: 0x078F_1E0A,
        heights: LICKER_HEIGHTS,
    },
    Damage {
        damage: 0x00C0_340E,
        stun: 0x078F_1E0A,
        heights: LICKER_HEIGHTS,
    },
    Damage {
        damage: 0x00C0_340E,
        stun: 0x0000_000A,
        heights: LICKER_HEIGHTS,
    },
    Damage {
        damage: 0x0C83_20C8,
        stun: 0x078F_1E0A,
        heights: LICKER_HEIGHTS,
    },
    Damage {
        damage: 0x12C4_B12C,
        stun: 0x078F_1E0A,
        heights: LICKER_HEIGHTS,
    },
    Damage {
        damage: 0x0280_B432,
        stun: 0x078F_1E0A,
        heights: LICKER_HEIGHTS,
    },
    Damage {
        damage: 0x0501_7C6E,
        stun: 0x078F_1E0A,
        heights: LICKER_HEIGHTS,
    },
];
const LICKER_24_ID: u8 = 0x24;
/// The same rows for `Em_24`: weaker to the magnum and the shotguns.
const LICKER_24_DAMAGE: [Damage; 7] = [
    LICKER_DAMAGE[0],
    LICKER_DAMAGE[1],
    LICKER_DAMAGE[2],
    Damage {
        damage: 0x0501_4050,
        stun: 0x078F_1E0A,
        heights: LICKER_HEIGHTS,
    },
    Damage {
        damage: 0x0641_9064,
        stun: 0x078F_1E0A,
        heights: LICKER_HEIGHTS,
    },
    Damage {
        damage: 0x01E0_8423,
        stun: 0x078F_1E0A,
        heights: LICKER_HEIGHTS,
    },
    Damage {
        damage: 0x0320_DC3C,
        stun: 0x078F_1E0A,
        heights: LICKER_HEIGHTS,
    },
];

/// `W_at_ck`, read at three offsets: what a body's height band can be hit in.
const W_AT_CK: [u8; 0x14] = [4, 2, 1, 2, 1, 4, 1, 2, 4, 4, 2, 0, 2, 0, 0, 1, 2, 0, 0, 0];

/// `Set_MinMax`: the heights of a reach's lowest bit, and a third of them.
fn set_min_max(heights: &[i32; 6], reach: u8) -> (i32, i32, i32) {
    let bit = reach.trailing_zeros();
    let third = |low: i32, high: i32| (low - high) / 3;
    let [a, b, c, d, e, f] = *heights;
    match bit {
        1 => {
            let step = third(e, f);
            (e - step, f - step, step)
        }
        2 => (e, f, third(e, f)),
        3 => (c, d, third(c, d)),
        4 => (a, b, third(a, b)),
        5 => {
            let step = third(a, b);
            (a + step, b + step, step)
        }
        6 => {
            let step = third(a, b);
            (a + step * 2, b + step * 2, step)
        }
        _ => {
            let step = third(e, f);
            (e - step * 2, f - step * 2, step)
        }
    }
}

/// `Attack_r_ck0`: `at` is inside the rectangle of `rect`, which turns with the
/// shooter. The original scales by four to fit the GTE's sixteen bits.
fn attack_r_ck0(pos: [i32; 3], angle: u16, rect: [i16; 4], at: [i32; 2]) -> bool {
    let [x, z, width, length] = rect.map(i32::from);
    let (ox, oz) = rotate(angle, x, -(z + length * 4));
    let (origin_x, origin_z) = ((ox + pos[0]) >> 2, (oz + pos[2]) >> 2);
    let (wide_x, wide_z) = rotate(angle, width, 0);
    let (long_x, long_z) = rotate(angle, 0, length * 2);
    let clamp = |value: i32| value.clamp(-0x8000, 0x7FFF);
    let (ex, ez) = (at[0] >> 2, at[1] >> 2);
    let corners = [
        (ex - origin_x, ez - origin_z),
        (ex - (origin_x + wide_x), ez - (origin_z + wide_z)),
        (ex - (origin_x + long_x), ez - (origin_z + long_z)),
    ];
    let along =
        |axis: (i32, i32), corner: (i32, i32)| clamp((axis.0 * corner.0 + axis.1 * corner.1) >> 12);
    let width_axis = (wide_x, wide_z);
    let length_axis = (long_x, long_z);
    (along(width_axis, corners[0]) ^ along(width_axis, corners[1])) < 0
        && (along(length_axis, corners[0]) ^ along(length_axis, corners[2])) < 0
}

/// A `FUN_004d7e30`: a search for the bodies in a rectangle at `at`, turned
/// by `dir`. `word` is its last argument: the weapon's row of `Wp_dmg_tbl` in
/// the low half, `0x10000` for every body in it rather than the first,
/// `0x20000` for the band by the height, and the damage word's field in the
/// top four bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blast {
    pub at: [i32; 3],
    pub dir: i16,
    pub word: u32,
}

/// What a blast did to a body, as `FUN_004d7e30` writes it straight to the
/// enemy's work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlastEffect {
    /// A body it tested: `Damage_flg` (`+0x1d0`) starts again at 0.
    Tested(usize),
    Hit(BlastHit),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlastHit {
    pub slot: usize,
    pub weapon: u8,
    pub damage: i16,
    /// `Damage_no` (`+0x1d2`).
    pub damage_no: u8,
    /// `Damage_cnt`'s new count.
    pub count: u8,
    /// `Damage_flg`'s low byte.
    pub flags: u8,
}

/// `FUN_004d7e30`. Each body hit loses its life on `targets` at once, as the
/// original does, so a later search the same tick passes it by.
///
/// `rect` is the caller's `x`, `z`, width and length, as `attack_r_ck0`
/// takes them. Each body tested widens it by its `+0x1ee`, and only a miss
/// takes that back: a hit leaves it wider for the bodies after, and for the
/// caller's next search.
///
/// The routine a hit replaces isn't kept at `+0x1fc`, and the search for the
/// first body only (no `0x10000`) leaves out `0x40000`'s test of the floor.
pub fn blast_ck(blast: &Blast, rect: &mut [i16; 4], targets: &mut [Target]) -> Vec<BlastEffect> {
    let mut effects = Vec::new();
    let band = (blast.word >> 28) as u8;
    let every = blast.word & 0x10000 != 0;
    let weapon = blast.word as u8;
    let y = blast.at[1];
    for target in targets.iter_mut() {
        if !target.in_play()
            || target.damage_cnt != 0
            || target.life < 0
            || target.kind & TYPE_UNTARGETED != 0
        {
            continue;
        }
        effects.push(BlastEffect::Tested(target.slot));
        let h = u32::from(target.at_h);
        let above = (i32::from(target.ofs_y) - y) as u32;
        if above
            .wrapping_add(h + 100)
            .wrapping_add(target.pos[1] as u32)
            >= h * 2 + 200
        {
            continue;
        }
        let wide = (target.at_w1 as i16) >> 2;
        rect[2] = rect[2].wrapping_add(wide);
        rect[3] = rect[3].wrapping_add(wide);
        if !attack_r_ck0(blast.at, blast.dir as u16, *rect, target.at) {
            rect[2] = rect[2].wrapping_sub(wide);
            rect[3] = rect[3].wrapping_sub(wide);
            continue;
        }
        let Some(&[damage, stun]) =
            heavy_rows(target.id).and_then(|rows| rows.get(usize::from(weapon).checked_sub(9)?))
        else {
            continue;
        };
        let damage = (damage >> (u32::from(band) * 10) & 0x3FF) as i16;
        target.life = target.life.wrapping_sub(damage);
        let mut damage_no = 1u8;
        if blast.word & 0x20000 != 0 {
            let half = i32::from(target.ofs_y) >> 1;
            if target.pos[1] + half < y {
                damage_no = 0;
            }
            if target.be_flg & 0x1000_0000 != 0 && y < target.pos[1] + half * 3 {
                damage_no = 2;
            }
        }
        let count = (stun >> 9 & 0x7F) as u8;
        target.damage_cnt = (target.damage_cnt & 0x80) | count;
        let direction = i32::from(crate::player2::direction_ck(
            blast.at[0],
            blast.at[2],
            target.pos[0],
            target.pos[2],
        ) as i16);
        let side = direction - i32::from(target.angle as i16);
        let mut flags = 1u8;
        if (side + 0x400) & 0xFFF < 0x800 {
            flags |= 0x20;
        }
        if (side + 0x600) & 0xFFF < 0x400 {
            flags |= 0x40;
        }
        if (side - 0x200) & 0xFFF < 0x400 {
            flags |= 0x80;
        }
        effects.push(BlastEffect::Hit(BlastHit {
            slot: target.slot,
            weapon,
            damage,
            damage_no: damage_no.wrapping_add(band * 3),
            count,
            flags,
        }));
        if !every {
            break;
        }
    }
    effects
}

/// `Wp_dmg_tbl`'s rows from the grenade launcher on, `Type & 0xFFF` of 9 to
/// 0x13: each the damage word and the stun word, as [`Damage`] has them.
type HeavyRows = [[u32; 2]; 11];

fn heavy_rows(id: u8) -> Option<&'static HeavyRows> {
    let rows = match id {
        // 0x535350.
        0x10..=0x14 | 0x18..=0x1F | 0x2C | 0x37 | 0x38 | 0x3C..=0x3F => &[
            [0x00A0_C8C8, 0x078F_1E0A],
            [0x0050_C8C8, 0x078F_1E0A],
            [0x00A0_C8C8, 0x078F_1E0A],
            [0x01E0_781E, 0x0000_000A],
            [0x00E0_3C10, 0x0285_0A14],
            [0x03C0_F03C, 0x078F_1E0A],
            [0x0040_1004, 0x0081_020A],
            [0x00F0_3C0F, 0x0285_0A0A],
            [0x384E_1384, 0x078F_1E0A],
            [0x0080_2008, 0x0102_040A],
            [0x00E0_3C10, 0x0285_0A14],
        ],
        // 0x5354D0.
        0x15..=0x17 => &[
            [0x00A0_C8C8, 0x078F_1E0A],
            [0x0050_C8C8, 0x078F_1E0A],
            [0x00A0_C8C8, 0x078F_1E0A],
            [0x01E0_781E, 0x0000_000A],
            [0x00E0_3C10, 0x0285_0A14],
            [0x03C0_F03C, 0x078F_1E0A],
            [0x0040_1004, 0x0081_020A],
            [0x00F0_3C0F, 0x0285_0A0A],
            [0x384E_1384, 0x078F_1E0A],
            [0x0080_2008, 0x0102_040A],
            [0x00E0_3C10, 0x0285_0A14],
        ],
        // 0x535650.
        0x20 => &[
            [0x00A0_C92C, 0x078F_1E0A],
            [0x0050_C92C, 0x078F_1E0A],
            [0x00A0_C92C, 0x078F_1E0A],
            [0x01A0_681A, 0x0000_000A],
            [0x00F0_4012, 0x078F_1E0A],
            [0x0411_0441, 0x078F_1E0A],
            [0x00A0_280A, 0x0183_060A],
            [0x00C0_300C, 0x0285_0A0A],
            [0x12C4_B12C, 0x078F_1E0A],
            [0x0120_4812, 0x0183_060A],
            [0x00F0_4012, 0x078F_1E0A],
        ],
        // 0x5357D0.
        0x21 => &[
            [0x00F0_F03C, 0x078F_1E0A],
            [0x00F0_F03C, 0x078F_1E0A],
            [0x00F0_F03C, 0x078F_1E0A],
            [0x00F0_3C0F, 0x0000_000A],
            [0x00F0_3C0F, 0x078F_1E0A],
            [0x03C0_F03C, 0x078F_1E0A],
            [0x00F0_3C0F, 0x078F_1E0A],
            [0x00F0_3C0F, 0x078F_1E0A],
            [0x12C4_B12C, 0x078F_1E0A],
            [0x01E0_781E, 0x078F_1E0A],
            [0x00F0_3C0F, 0x078F_1E0A],
        ],
        // 0x535950.
        0x22 => &[
            [0x0140_F03C, 0x078F_1E0A],
            [0x0050_F055, 0x078F_1E0A],
            [0x00A0_F082, 0x078F_1E0A],
            [0x0140_5014, 0x0000_000A],
            [0x00C0_340E, 0x078F_1E0A],
            [0x0461_1846, 0x078F_1E0A],
            [0x00B0_2C0B, 0x0285_0A0A],
            [0x00C0_300C, 0x0387_0E0A],
            [0x0C83_20C8, 0x078F_1E0A],
            [0x0140_5014, 0x0285_0A0A],
            [0x00C0_340E, 0x078F_1E0A],
        ],
        // 0x535C50.
        0x23 => &[
            [0x01E0_781E, 0x078F_1E0A],
            [0x0050_7C1F, 0x078F_1E0A],
            [0x0190_6419, 0x078F_1E0A],
            [0x0140_5014, 0x0000_000A],
            [0x00F0_4011, 0x0285_0A14],
            [0x01E0_781E, 0x078F_1E0A],
            [0x00A0_280A, 0x050A_140A],
            [0x00A0_280A, 0x050A_140A],
            [0x06E1_B86E, 0x078F_1E0A],
            [0x0120_4812, 0x050A_140A],
            [0x00F0_4011, 0x0285_0A14],
        ],
        // 0x535AD0.
        0x24 => &[
            [0x00A0_F03C, 0x078F_1E0A],
            [0x0050_F050, 0x078F_1E0A],
            [0x00F1_9064, 0x078F_1E0A],
            [0x0140_5014, 0x0000_000A],
            [0x00C0_340E, 0x078F_1E0A],
            [0x0320_C832, 0x078F_1E0A],
            [0x00B0_2C0B, 0x0285_0A0A],
            [0x00B0_2C0B, 0x0387_0E0A],
            [0x0C83_20C8, 0x078F_1E0A],
            [0x0140_5014, 0x0285_0A0A],
            [0x00C0_340E, 0x078F_1E0A],
        ],
        // 0x535DD0.
        0x25 | 0x26 => &[
            [0x0140_F03C, 0x078F_1FB4],
            [0x0052_0882, 0x078F_1F68],
            [0x00A0_F05A, 0x078F_1F68],
            [0x0140_5014, 0x0000_000A],
            [0x00F0_4011, 0x078F_1E0A],
            [0x0461_1846, 0x078F_1E0A],
            [0x00A0_280A, 0x0285_0A0A],
            [0x00A0_280A, 0x0285_0A0A],
            [0x0C83_20C8, 0x078F_1E0A],
            [0x0120_4812, 0x0285_0A0A],
            [0x00F0_4011, 0x078F_1E0A],
        ],
        // 0x535F50.
        0x27 => &[
            [0x03C0_F03C, 0x078F_1E0A],
            [0x0051_4050, 0x078F_1E0A],
            [0x00A1_E479, 0x078F_1E0A],
            [0x0120_4812, 0x0000_000A],
            [0x0120_4C14, 0x0285_0A14],
            [0x03C0_F03C, 0x078F_1E0A],
            [0x00A0_280A, 0x0285_0A0A],
            [0x0030_0C03, 0x0081_020A],
            [0x0C93_24C9, 0x078F_1E0A],
            [0x0110_4411, 0x0285_0A0A],
            [0x0120_4C14, 0x0285_0A14],
        ],
        // 0x5360D0.
        0x28 => &[
            [0x03C0_F03C, 0x078F_1E0A],
            [0x0051_4050, 0x078F_1E0A],
            [0x00A1_E479, 0x078F_1E0A],
            [0x0120_4812, 0x0000_000A],
            [0x0120_4C14, 0x0285_0A14],
            [0x03C0_F03C, 0x078F_1E0A],
            [0x00A0_280A, 0x0285_0A0A],
            [0x0030_0C03, 0x0081_020A],
            [0x0C93_24C9, 0x078F_1E0A],
            [0x0110_4411, 0x0285_0A0A],
            [0x0120_4C14, 0x0285_0A14],
        ],
        // 0x536250.
        0x29 => &[
            [0x00F0_F03C, 0x078F_1E0A],
            [0x00A0_F03C, 0x078F_1E0A],
            [0x00A0_F03C, 0x078F_1E0A],
            [0x00F0_3C0F, 0x0000_000A],
            [0x00E0_3C10, 0x078F_1E14],
            [0x0280_A028, 0x078F_1E0A],
            [0x0070_1C07, 0x0081_020A],
            [0x0280_A028, 0x078F_1E0A],
            [0x12C4_B12C, 0x078F_1E0A],
            [0x0070_1C07, 0x0081_020A],
            [0x00E0_3C10, 0x078F_1E14],
        ],
        // 0x536550.
        0x2A => &[
            [0x0320_C832, 0x078F_1E0A],
            [0x0050_F03C, 0x078F_1EDA],
            [0x00A1_4451, 0x078F_1EDA],
            [0x0100_4010, 0x0000_000A],
            [0x0100_4010, 0x078F_1E0A],
            [0x0461_1846, 0x078F_1E0A],
            [0x00D0_340D, 0x0489_120A],
            [0x0140_5014, 0x078F_1E0A],
            [0x1F47_D1F4, 0x078F_1E0A],
            [0x0180_6018, 0x0489_120A],
            [0x0100_4010, 0x078F_1E0A],
        ],
        // 0x5366D0.
        0x2B => &[
            [0x0320_C832, 0x078F_1E0A],
            [0x0050_F03C, 0x078F_1EDA],
            [0x00A1_9465, 0x078F_1EDA],
            [0x0100_4010, 0x0000_000A],
            [0x0100_4010, 0x078F_1E0A],
            [0x03C0_F03C, 0x078F_1E0A],
            [0x00D0_340D, 0x0489_120A],
            [0x0140_5014, 0x078F_1E0A],
            [0x1F47_D1F4, 0x078F_1E0A],
            [0x0180_6018, 0x0489_120A],
            [0x0100_4010, 0x078F_1E0A],
        ],
        // 0x5363D0.
        0x2D => &[
            [0x0140_F03C, 0x078F_1E0A],
            [0x00A0_F03C, 0x078F_1E0A],
            [0x00A0_F03C, 0x078F_1E0A],
            [0x00F0_4414, 0x0000_000A],
            [0x00E0_3C10, 0x0285_0A14],
            [0x0280_A028, 0x078F_1E0A],
            [0x0070_1C07, 0x0081_020A],
            [0x0280_A028, 0x0387_0E0A],
            [0x0280_A028, 0x078F_1E0A],
            [0x00E0_380E, 0x0081_020A],
            [0x00E0_3C10, 0x0285_0A14],
        ],
        // 0x536850.
        0x2E | 0x2F | 0x39 => &[
            [0x0280_A028, 0x078F_1E0A],
            [0x00A1_B92C, 0x078F_1E0A],
            [0x00A1_1846, 0x078F_1E0A],
            [0x00A0_280A, 0x0000_000A],
            [0x0070_1C07, 0x0285_0A14],
            [0x0320_C832, 0x078F_1E0A],
            [0x0050_1405, 0x0306_0C0A],
            [0x00F0_3C0F, 0x0183_060A],
            [0x12C4_B12C, 0x078F_1E0A],
            [0x00A0_280A, 0x0306_0C0A],
            [0x0070_1C07, 0x0285_0A14],
        ],
        // 0x5369D0.
        0x30 | 0x32 | 0x35 => &[
            [0x03C0_F03C, 0x078F_1E0A],
            [0x0051_1846, 0x078F_1E0A],
            [0x00A1_9465, 0x078F_1E0A],
            [0x0110_4411, 0x0000_000A],
            [0x00F0_4011, 0x0285_0A14],
            [0x0501_4050, 0x078F_1E0A],
            [0x0080_2008, 0x0285_0A0A],
            [0x0040_1004, 0x0102_040A],
            [0x0C83_20C8, 0x078F_1E0A],
            [0x0100_4010, 0x0285_0A0A],
            [0x00E0_3C10, 0x0285_0A14],
        ],
        // 0x536B50.
        0x31 => &[
            [0x03C0_F03C, 0x078F_1E0A],
            [0x0051_9064, 0x078F_1E0A],
            [0x00A1_4050, 0x078F_1E0A],
            [0x0110_4411, 0x0000_000A],
            [0x0130_5015, 0x0285_0A14],
            [0x0461_1846, 0x078F_1E0A],
            [0x00A0_280A, 0x0285_0A0A],
            [0x0050_1405, 0x0102_040A],
            [0x12C4_B12C, 0x078F_1E0A],
            [0x0120_4812, 0x0285_0A0A],
            [0x0130_5015, 0x0285_0A14],
        ],
        // 0x536CD0.
        0x33 => &[
            [0x03C0_F03C, 0x078F_1E0A],
            [0x0051_9465, 0x078F_1E0A],
            [0x00A1_4050, 0x078F_1E0A],
            [0x0130_4C13, 0x0000_000A],
            [0x0190_681B, 0x0285_0A14],
            [0x0501_4050, 0x078F_1E0A],
            [0x00C0_300C, 0x0408_100A],
            [0x0080_2008, 0x0204_080A],
            [0x1906_4190, 0x078F_1E0A],
            [0x0140_5014, 0x0387_0E0A],
            [0x0190_681B, 0x0285_0A14],
        ],
        // 0x536E50.
        0x34 => &[
            [0x05A1_685A, 0x078F_1E0A],
            [0x0051_1846, 0x078F_1E0A],
            [0x00A0_F03C, 0x078F_1E0A],
            [0x0130_4C13, 0x0000_000A],
            [0x01B0_701D, 0x0285_0A14],
            [0x03C0_F03C, 0x078F_1E0A],
            [0x0090_2409, 0x0285_0A0A],
            [0x0050_1405, 0x0102_040A],
            [0x12D4_B52D, 0x078F_1E0A],
            [0x0140_5014, 0x0285_0A0A],
            [0x01B0_701D, 0x0285_0A14],
        ],
        // 0x537150.
        0x36 => &[
            [0x0501_4050, 0x078F_1E0A],
            [0x0051_1846, 0x078F_1E0A],
            [0x00A1_1846, 0x078F_1E0A],
            [0x0140_5014, 0x0000_000A],
            [0x00F0_4011, 0x0285_0A14],
            [0x03C0_F03C, 0x078F_1E0A],
            [0x00A0_280A, 0x0285_0A0A],
            [0x0050_1405, 0x0102_040A],
            [0x0C83_20C8, 0x078F_1E0A],
            [0x0110_4411, 0x0285_0A0A],
            [0x00F0_4011, 0x0285_0A14],
        ],
        // 0x5372D0.
        0x3A => &[
            [0x0320_C832, 0x078F_1E0A],
            [0x0051_9465, 0x078F_1E0A],
            [0x00A0_F03C, 0x078F_1E0A],
            [0x00A0_280A, 0x0000_000A],
            [0x0090_280B, 0x078F_1E0A],
            [0x0280_A028, 0x078F_1E0A],
            [0x00A0_280A, 0x0285_0A0A],
            [0x00C0_300C, 0x0285_0A0A],
            [0x12C4_B12C, 0x078F_1E0A],
            [0x0140_5014, 0x0285_0A0A],
            [0x0090_280B, 0x078F_1E0A],
        ],
        // 0x537450.
        0x3B => &[
            [0x0280_A028, 0x078F_1E0A],
            [0x00A0_A028, 0x078F_1E0A],
            [0x00A0_A028, 0x078F_1E0A],
            [0x00F0_4014, 0x0000_000A],
            [0x00F0_3C0F, 0x078F_1E0A],
            [0x0280_A028, 0x078F_1E0A],
            [0x0070_1C07, 0x0285_0A0A],
            [0x0070_1C07, 0x0285_0A0A],
            [0x12C4_B12C, 0x078F_1E0A],
            [0x00E0_380E, 0x0285_0A0A],
            [0x00F0_3C0F, 0x078F_1E0A],
        ],
        _ => return None,
    };
    Some(rows)
}

/// What `Weapon_at_ck` needs of the shooter.
pub struct Shooter {
    pub pos: [i32; 3],
    pub neck: [i32; 3],
    pub angle: u16,
    /// `Type & 0xFFF`, from 2 to 8.
    pub weapon: u8,
    /// [`hit_scan_ck`]'s level.
    pub level: usize,
}

/// `Weapon_at_ck`, for one frame of a shot: the nearest enemy the shot
/// reaches. Its three rectangles run out from the gun and take in a body by
/// its width, once the height of the body picks a line of `W_AT_CK`.
pub fn weapon_at_ck(shooter: &Shooter, targets: &[Target], sca: &[ScaRect]) -> Option<Hit> {
    let records = shot_records(shooter.weapon)?;
    let record = records.get(shooter.level)?;
    let mut nearest = i32::MAX as u32;
    let mut found = None;
    let mut region = 0;
    for target in targets.iter().filter(|target| target.in_play()) {
        if target.damage_cnt != 0 || target.life < 0 || target.kind & TYPE_UNTARGETED != 0 {
            continue;
        }
        let Some(table) = damage_table(target.id, shooter.weapon) else {
            continue;
        };
        let to = [target.at[0], target.neck_y, target.at[1]];
        if sca_ck_line(sca, shooter.neck, to, LINE_MASK) {
            continue;
        }
        let height = target.pos[1] - shooter.pos[1];
        let stance = (target.be_flg >> 26 & 7) as u8;
        let mut result = 0u8;
        if stance != 0 {
            for (index, (&reach, rect)) in record.reach.iter().zip(&record.rects).enumerate() {
                if reach == 0 {
                    continue;
                }
                let (low, high, step) = set_min_max(&table.heights, reach);
                if (height.wrapping_sub(low) as u32) > (high.wrapping_sub(low) as u32) {
                    continue;
                }
                let mut line = 6;
                if high + step * 2 <= height {
                    line = 3;
                }
                if high + step <= height {
                    line = 0;
                }
                let line = line + usize::from(reach & 8 != 0) * 9;
                let mut rect = *rect;
                rect[3] += (target.at_w >> 2) as i16;
                if index == 0 {
                    rect[2] += (target.at_w1 >> 2) as i16;
                }
                if !attack_r_ck0(shooter.pos, shooter.angle, rect, target.at) {
                    continue;
                }
                result = 0;
                for offset in [2, 1, 0] {
                    let bits = W_AT_CK[line + offset];
                    if stance & bits != 0 {
                        region = bits >> 1;
                        result = region * 8 + (1 << index);
                    }
                }
            }
        }
        if result != 0 {
            let (dx, dz) = (shooter.pos[0] - target.at[0], shooter.pos[2] - target.at[1]);
            let distance =
                square_root0(dz.wrapping_mul(dz).wrapping_add(dx.wrapping_mul(dx))) as u32;
            if distance < nearest {
                nearest = distance;
                found = Some((target.slot, target.id, table, result, region));
            }
        }
    }
    let (slot, id, table, result, region) = found?;
    let band = if result & 4 != 0 {
        2
    } else {
        u8::from(result & 2 != 0)
    };
    let mut hit = Hit {
        slot,
        id,
        weapon: shooter.weapon,
        high: shooter.level == 2,
        band,
        region,
        damage: 0,
        stun: 0,
        count: 0,
        flags: u16::from(result),
    };
    hit.take_row(table);
    Some(hit)
}

impl Hit {
    /// The damage, the region's loss and the count from `table`'s word for
    /// the band.
    fn take_row(&mut self, table: &Damage) {
        let band = u32::from(self.band);
        self.damage = (table.damage >> (band * 10) & 0x3FF) as i16;
        self.stun = (table.stun >> (band * 3) & 7) as u8;
        self.count = (table.stun >> (band * 7 + 9) & 0x7F) as u8;
    }
}

/// The end of `Weapon_at_ck` for a Beretta hit (`W_no` 0x120000), after the
/// body is found: two `Rnd`s, `r1` then `r2`. One time in sixteen or so
/// (`(r2 >> (r1 & 3)) & 0xF == 0`) the damage row steps back 14 rows, to the
/// magnum's (weapon 5), and the hit lands as weapon 6; otherwise 17, to the
/// handgun's (weapon 2), and it lands as weapon 3. So no body sees 0x13.
pub fn beretta_hit(hit: &mut Hit, rnd: &mut crate::zombie2::Rnd) {
    if hit.weapon != BERETTA {
        return;
    }
    let shift = rnd.draw() & 3;
    let (row, weapon) = if (rnd.draw() >> shift) & 0xF == 0 {
        (5, 6)
    } else {
        (2, 3)
    };
    hit.weapon = weapon;
    if let Some(table) = damage_table(hit.id, row) {
        hit.take_row(table);
    }
}

/// `Shoot_camera` does nothing with this many enemies in the room.
pub const SHOOT_CAMERA_ENEMIES: usize = 3;

/// `Shoot_camera`'s table at 0x53A878: each weapon's row, plus one, of
/// [`SHOOT_CAMERA_ROWS`], or none.
const SHOOT_CAMERA_ROW: [u8; 20] = [0, 0, 1, 1, 1, 1, 1, 2, 3, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1];

/// The table at 0x53A88C: half the width of the target around the camera, the
/// gun's axis in its hand, and how far below the camera the target's centre is.
const SHOOT_CAMERA_ROWS: [(i32, [i32; 2], i32); 3] = [
    (100, [0, 4096], 120),
    (400, [0, 4096], 150),
    (400, [3120, 5320], 0),
];

/// The part of the hand's matrix `Shoot_camera` works from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShotLine {
    pub from: [i32; 3],
    pub matrix: [[i16; 3]; 3],
}

fn outer_product0(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [
        b[2].wrapping_mul(a[1])
            .wrapping_sub(b[1].wrapping_mul(a[2])),
        b[0].wrapping_mul(a[2])
            .wrapping_sub(a[0].wrapping_mul(b[2])),
        a[0].wrapping_mul(b[1])
            .wrapping_sub(b[0].wrapping_mul(a[1])),
    ]
}

/// `VectorNormal`, which works in floating point and truncates.
fn vector_normal(v: [i32; 3]) -> [i32; 3] {
    let square: f64 = v
        .iter()
        .map(|&axis| f64::from(axis) * f64::from(axis))
        .sum();
    let scale = 1.0 / square.sqrt();
    v.map(|axis| (f64::from(axis) * 4096.0 * scale) as i32)
}

/// `Shoot_camera`: if the gun's line passes between two points beside the
/// camera, where its effects go, drawing `Rnd` for each. The original calls
/// `Esp_call` as it goes, which the caller does with these in turn.
pub fn shoot_camera(
    weapon: u8,
    line: ShotLine,
    eye: [i32; 3],
    look: [i32; 3],
    rnd: &mut crate::zombie2::Rnd,
) -> Vec<[i16; 3]> {
    let row = SHOOT_CAMERA_ROW
        .get(usize::from(weapon))
        .copied()
        .unwrap_or(0);
    if row == 0 {
        return Vec::new();
    }
    let row = usize::from(row - 1);
    let (width, axis, below) = SHOOT_CAMERA_ROWS[row];
    let dir = crate::player2::direction_ck(
        i32::from(eye[0] as i16),
        i32::from(eye[2] as i16),
        i32::from(look[0] as i16),
        i32::from(look[2] as i16),
    );
    let turn = crate::player2::rot_matrix([0, dir.wrapping_add(0x400) as i16, 0]);
    let side_x = (i32::from(turn[0][0]) * width) >> 12;
    let side_z = (i32::from(turn[2][0]) * width) >> 12;
    let [x, y, z] = line.from;
    let near = [
        (eye[0] - x + side_x) >> 4,
        (width - below - y + eye[1]) >> 4,
        (side_z - z + eye[2]) >> 4,
    ];
    let far = [
        (eye[0] - side_x - x) >> 4,
        (eye[1] - width - below - y) >> 4,
        (eye[2] - side_z - z) >> 4,
    ];
    let along = line
        .matrix
        .map(|row| (i32::from(row[0]) * axis[0] + i32::from(row[1]) * axis[1]) >> 12);
    let near = outer_product0(along, near);
    let far = outer_product0(along, far);
    let between = near[1] < 0 && far[1] >= 0 && (far[0] ^ near[0]) < 0 && (far[2] ^ near[2]) < 0;
    if !between {
        return Vec::new();
    }
    let toward = vector_normal([look[0] - eye[0], look[1] - eye[1], look[2] - eye[2]])
        .map(|axis| (axis >> 3) as i16);
    let base: [i16; 3] = std::array::from_fn(|axis| toward[axis].wrapping_add(eye[axis] as i16));
    let mut calls = vec![std::array::from_fn(|axis| {
        base[axis]
            .wrapping_sub(i16::from(rnd.draw()))
            .wrapping_add(0x80)
    })];
    if row != 0 {
        for _ in 0..5 {
            calls.push(std::array::from_fn(|axis| {
                base[axis]
                    .wrapping_add(i16::from(rnd.draw()).wrapping_mul(-2))
                    .wrapping_add(0x100)
            }));
        }
    }
    calls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dog_takes_each_weapons_own_row() {
        let damage = |weapon| damage_table(DOG_ID, weapon).unwrap().damage & 0x3FF;
        assert_eq!(
            [2, 3, 4, 5, 6, 7, 8].map(damage),
            [18, 18, 18, 200, 300, 59, 300]
        );
    }

    /// `Wp_dmg_tbl`: every body takes the handgun's row from the Colt and
    /// the Beretta, but `Em_30`, whose Colt row does 17 where its handgun's
    /// does 16.
    #[test]
    fn the_colt_and_the_beretta_take_the_handguns_rows() {
        for id in [
            0x10,
            LICKER_ID,
            LICKER_24_ID,
            DOG_ID,
            CROW_ID,
            MRX_ID,
            G2_ID,
            SPIDER_ID,
        ] {
            let handgun = damage_table(id, 2).unwrap();
            for weapon in [COLT, BERETTA] {
                let row = damage_table(id, weapon).unwrap();
                assert_eq!(
                    (row.damage, row.stun, row.heights),
                    (handgun.damage, handgun.stun, handgun.heights),
                    "{id:#x} {weapon:#x}"
                );
            }
        }
        let colt = damage_table(G1_ID, COLT).unwrap();
        assert_eq!((colt.damage, colt.stun), (0x00F0_4011, 0x0285_0A14));
        assert_eq!(colt.damage & 0x3FF, 17);
        assert_eq!(damage_table(G1_ID, BERETTA).unwrap().damage, 0x00E0_3C10);
        for weapon in [COLT, BERETTA] {
            let records = shot_records(weapon).unwrap();
            for (record, handgun) in records.iter().zip(&HANDGUN) {
                assert_eq!((record.reach, record.rects), (handgun.reach, handgun.rects));
            }
        }
    }

    /// The spark shot's records (`0x539CB0`) have one rectangle a level, and
    /// its look reads each body's own fourteenth row's heights; the strike's
    /// damage comes through `heavy_rows`' row for 0xE.
    #[test]
    fn the_spark_shot_looks_with_its_own_records_and_heights() {
        let records = shot_records(SPARK).unwrap();
        for (record, reach) in records.iter().zip([4, 8, 0x10]) {
            assert_eq!(record.reach, [reach, 0, 0]);
            assert_eq!(record.rects, [[200, 0, 3500, 500], [0; 4], [0; 4]]);
        }
        let heights = |id| damage_table(id, SPARK).unwrap().heights;
        assert_eq!(heights(0x10), [-4000, -2000, -3000, 2000, -500, 3000]);
        assert_eq!(heights(0x10), heights(crate::zombie2::EMBRYO_ID));
        assert_eq!(heights(DOG_ID), heights(crate::zombie2::ROACH_ID));
        assert_eq!(heights(LICKER_ID), [-5000, -2000, -3500, 500, -500, 2000]);
        assert_eq!(heights(SPIDER_ID), heights(YOUNG_ID));
        assert_eq!(heights(MRX_ID)[1], 100);
        assert_eq!(heights(G2_ID)[2], -6000);
        assert_eq!(heights(CROW_ID)[0], -6000);
        assert_eq!(
            heavy_rows(0x1F).unwrap()[usize::from(SPARK) - 9],
            [0x03C0_F03C, 0x078F_1E0A]
        );
        // A zombie straight ahead in reach is found; one past it isn't.
        for (x, found) in [(3000, true), (16000, false)] {
            let shooter = Shooter {
                pos: [0, 0, 0],
                neck: [0, -1500, 0],
                angle: 0,
                weapon: SPARK,
                level: 1,
            };
            let mut body = zombie(4, x, 0);
            body.at_w = 500;
            let hit = weapon_at_ck(&shooter, &[body], &[]);
            assert_eq!(hit.map(|hit| hit.slot), found.then_some(4), "at {x}");
        }
    }

    /// `Wp_dmg_tbl`'s fifteenth rows: the sub-machine gun's, its own for
    /// each body, on the handguns' records.
    /// `Em_27`'s and `Em_28`'s rows (`0x535f50`, `0x5360d0`) are the same, with the
    /// handgun's heights for every weapon.
    #[test]
    fn the_g_embryo_and_the_adult_take_their_own_rows() {
        for id in [crate::zombie2::EMBRYO_ID, crate::zombie2::ADULT_ID] {
            let handgun = damage_table(id, 2).unwrap();
            assert_eq!((handgun.damage, handgun.stun), (0x0120_4C14, 0x0285_0A14));
            let magnum = damage_table(id, 5).unwrap();
            assert_eq!(magnum.damage, 0x05F1_9069);
            let shotgun = damage_table(id, 7).unwrap();
            assert_eq!((shotgun.damage, shotgun.heights), (0x0370_F041, HANDGUN_HEIGHTS));
        }
    }

    #[test]
    fn the_smg_takes_each_bodys_fifteenth_row() {
        let row = |id| {
            let row = damage_table(id, SMG).unwrap();
            (row.damage, row.stun, row.heights)
        };
        assert_eq!(row(0x10), (0x0040_1004, 0x0081_020A, SHOTGUN_HEIGHTS));
        assert_eq!(row(0x10).0 & 0x3FF, 4);
        assert_eq!(row(LICKER_ID), row(LICKER_24_ID));
        assert_eq!(row(LICKER_ID).0, 0x00B0_2C0B);
        assert_eq!(row(DOG_ID).1, 0x0183_060A);
        assert_eq!(row(CROW_ID).0, 0x00F0_3C0F);
        assert_eq!(row(MRX_ID).2, [-3200, 100, -1000, 1600, -1000, 2600]);
        assert_eq!(row(G1_ID).0, 0x0080_2008);
        assert_eq!(row(G2_ID).2[2], -6000);
        assert_eq!(row(G3_ID), (0x00C0_300C, 0x0408_100A, SHOTGUN_HEIGHTS));
        assert_eq!(row(SPIDER_ID), row(YOUNG_ID));
        assert_eq!(row(ALLIGATOR_ID).1, 0x050A_140A);
        assert_eq!(row(IVY_ID).0, 0x0050_1405);
        // The embryo's, the cockroach's and room 205's bodies' own rows,
        // not the zombie's: their heights decide what `Set_MinMax` lets hit.
        use crate::zombie2::{EM2D_ID, EMBRYO_ID, ROACH_ID};
        assert_eq!(
            row(EMBRYO_ID),
            (
                0x00A0_280A,
                0x0285_0A0A,
                [-5000, -2000, -3000, 2000, -500, 3000]
            )
        );
        assert_eq!(
            row(ROACH_ID),
            (
                0x0070_1C07,
                0x0081_020A,
                [-4000, -2800, -3100, -1200, -1200, 100]
            )
        );
        assert_eq!(
            row(EM2D_ID),
            (
                0x0070_1C07,
                0x0081_020A,
                [-3000, -2000, -3000, 2000, -500, 3000]
            )
        );
        assert_eq!(row(0x15), row(0x10));
        let records = shot_records(SMG).unwrap();
        for (record, handgun) in records.iter().zip(&HANDGUN) {
            assert_eq!((record.reach, record.rects), (handgun.reach, handgun.rects));
        }
        // The flamethrower's hits are its flames', not the shot's.
        assert!(shot_records(crate::player2::FLAMETHROWER).is_none());
    }

    #[test]
    fn g1_has_rows_of_its_own() {
        let damage = |weapon| damage_table(G1_ID, weapon).unwrap().damage & 0x3FF;
        assert_eq!(
            [2, 3, 4, 5, 6, 7, 8].map(damage),
            [16, 16, 16, 90, 100, 60, 90]
        );
        assert_eq!(damage_table(G1_ID, 7).unwrap().heights[1], 500);
        assert_eq!(damage_table(G1_ID, 5).unwrap().heights[1], -2000);
    }

    #[test]
    fn the_plants_share_one_row_for_each_weapon() {
        for id in [IVY_ID, VINE_ID, POISON_IVY_ID] {
            let damage = |weapon| damage_table(id, weapon).unwrap().damage & 0x3FF;
            assert_eq!(
                [2, 3, 4, 5, 6, 7, 8].map(damage),
                [7, 7, 7, 50, 100, 30, 60]
            );
            assert_eq!(damage_table(id, 7).unwrap().heights[1], 500);
        }
    }

    /// `Em_23_wp_tbl`: the handgun's 17, 16 and 15 a band, the magnums' 31
    /// and 50, the shotguns' 27 and 50 at their nearest.
    #[test]
    fn the_alligator_takes_a_few_units_a_shot() {
        let damage = |weapon| damage_table(ALLIGATOR_ID, weapon).unwrap().damage;
        assert_eq!(damage(2) & 0x3FF, 17);
        assert_eq!(damage(2) >> 10 & 0x3FF, 16);
        assert_eq!(damage(2) >> 20 & 0x3FF, 15);
        assert_eq!(
            [5, 6, 7, 8].map(|weapon| damage(weapon) & 0x3FF),
            [31, 50, 27, 50]
        );
        assert_eq!(damage_table(ALLIGATOR_ID, 7).unwrap().heights[1], 500);
    }

    #[test]
    fn a_spider_takes_each_weapons_own_row() {
        let row = |weapon| damage_table(SPIDER_ID, weapon).unwrap();
        assert_eq!(
            [2, 3, 4, 5, 6, 7, 8].map(|weapon| row(weapon).damage & 0x3FF),
            [17, 17, 17, 130, 150, 55, 130]
        );
        assert_eq!(row(4).stun, 0xA);
        assert_eq!(row(8).heights, [-7000, -2000, -4200, 500, -500, 2000]);
    }

    #[test]
    fn a_crow_takes_each_weapons_own_row() {
        let damage = |weapon| damage_table(CROW_ID, weapon).unwrap().damage & 0x3FF;
        assert_eq!(
            [2, 3, 4, 5, 6, 7, 8].map(damage),
            [15, 15, 15, 200, 300, 60, 120]
        );
    }

    #[test]
    fn shoot_camera_only_throws_when_the_gun_lines_up() {
        // Leon's hand firing from the street's start, along -X.
        let line = ShotLine {
            from: [18162, -2505, -2675],
            matrix: [[1, -4094, 11], [-4097, -4, 8], [-10, -10, -4094]],
        };
        let look = [22356, -2466, 522];
        let mut rnd = crate::zombie2::Rnd::default();
        let before = rnd;

        // The cut's own camera: the gun passes well beside it.
        let beside = shoot_camera(2, line, [11268, -2664, -3600], look, &mut rnd);
        assert!(beside.is_empty());
        assert_eq!(rnd, before);

        // A camera on the gun's line: a handgun throws one, three draws.
        let eye = [15164, -2388, -2682];
        assert_eq!(shoot_camera(2, line, eye, look, &mut rnd).len(), 1);
        let mut drawn = before;
        for _ in 0..3 {
            drawn.draw();
        }
        assert_eq!(rnd, drawn);

        // The shotgun throws six, and a knife none.
        assert_eq!(shoot_camera(7, line, eye, look, &mut rnd).len(), 6);
        assert!(shoot_camera(1, line, eye, look, &mut rnd).is_empty());
    }

    fn zombie(slot: usize, x: i32, z: i32) -> Target {
        Target {
            slot,
            id: 0x1F,
            be_flg: 1 | 0x400_0000 | 0x800_0000,
            life: 50,
            pos: [x, 0, z],
            at: [x, z],
            at_w: 500,
            neck_y: -1500,
            ..Target::default()
        }
    }

    #[test]
    fn a_licker_takes_its_own_damage_from_a_gun() {
        let licker = damage_table(LICKER_ID, 6).unwrap();
        let zombie = damage_table(0x10, 6).unwrap();
        assert_eq!(licker.damage, 0x12C4_B12C);
        assert_ne!(licker.damage, zombie.damage);
        assert!(damage_table(LICKER_ID, 1).is_none());
        // `Em_24` shares the handgun's rows but not the magnum's.
        let second = |weapon| damage_table(LICKER_24_ID, weapon).unwrap().damage;
        assert_eq!(second(2), 0x00C0_340E);
        assert_eq!(second(6), 0x0641_9064);
    }

    #[test]
    fn the_hand_picks_the_shots_level() {
        assert_eq!(hit_scan_ck(0, 2, 0, 0x1000, -2427), 1);
        assert_eq!(hit_scan_ck(0, 2, 0, 0x1000, -2700), 2);
        assert_eq!(hit_scan_ck(0, 2, 0, 0x1000, -2000), 0);
        assert_eq!(hit_scan_ck(1, 2, 0, 0x1000, -2120), 1);
        // The Colt's level is a lower and narrower band: 400 under Leon's
        // shoulder of -1850 to 30 over it, and Claire's at -1890.
        assert_eq!(hit_scan_ck(0, COLT, 0, 0x1000, -2250), 1);
        assert_eq!(hit_scan_ck(0, COLT, 0, 0x1000, -2251), 2);
        assert_eq!(hit_scan_ck(0, COLT, 0, 0x1000, -1820), 1);
        assert_eq!(hit_scan_ck(0, COLT, 0, 0x1000, -1819), 0);
        assert_eq!(hit_scan_ck(1, COLT, 0, 0x1000, -1859), 0);
        assert_eq!(hit_scan_ck(0, BERETTA, 0, 0x1000, -2000), 0);
        // The sub-machine gun's: 400 under Leon's shoulder of -1894 to 200
        // over it, and Claire's at -1809.
        assert_eq!(hit_scan_ck(0, SMG, 0, 0x1000, -2294), 1);
        assert_eq!(hit_scan_ck(0, SMG, 0, 0x1000, -2295), 2);
        assert_eq!(hit_scan_ck(0, SMG, 0, 0x1000, -1694), 1);
        assert_eq!(hit_scan_ck(0, SMG, 0, 0x1000, -1693), 0);
        assert_eq!(hit_scan_ck(1, SMG, 0, 0x1000, -1608), 0);
        // The shotguns aim from their own height.
        assert_eq!(hit_scan_ck(0, 7, 0, 0x1000, -1813), 1);
        assert_eq!(hit_scan_ck(0, 7, 0, 0x1000, -2400), 2);
    }

    #[test]
    fn aiming_prefers_who_stands_and_then_who_is_nearest() {
        let targets = [zombie(0, 0, 3000), zombie(1, 0, 2000)];
        assert_eq!(em_ck(&targets, &[], [0; 3], [0; 3], 0), Some(1));
        let mut floored = targets;
        floored[1].kind = TYPE_ON_FLOOR;
        assert_eq!(em_ck(&floored, &[], [0; 3], [0; 3], 0), Some(0));
        floored[0].life = -1;
        assert_eq!(em_ck(&floored, &[], [0; 3], [0; 3], 0), Some(1));
        assert_eq!(em_ck(&targets, &[], [0; 3], [0; 3], 1000), None);
    }

    #[test]
    fn a_shot_finds_the_nearest_in_front_and_skips_who_is_recovering() {
        let shooter = Shooter {
            pos: [0; 3],
            neck: [0, -2000, 0],
            // Facing +x.
            angle: 0,
            weapon: 2,
            level: 1,
        };
        let mut targets = [zombie(0, 1600, 200), zombie(1, 900, 200)];
        let hit = weapon_at_ck(&shooter, &targets, &[]).expect("a hit");
        assert_eq!(hit.slot, 1);
        targets[1].damage_cnt = 3;
        assert_eq!(
            weapon_at_ck(&shooter, &targets, &[]).map(|hit| hit.slot),
            Some(0)
        );
        targets[0].pos[0] = -1600;
        targets[0].at[0] = -1600;
        assert_eq!(weapon_at_ck(&shooter, &targets, &[]), None);
    }

    #[test]
    fn the_first_box_of_a_shot_is_wider_by_a_bodys_at_w1() {
        let shooter = Shooter {
            pos: [0; 3],
            neck: [0, -2000, 0],
            angle: 0,
            weapon: 5,
            level: 0,
        };
        let hits = |at_w1: u16| {
            (0..8000)
                .step_by(100)
                .flat_map(|x| (-4000..4000).step_by(50).map(move |z| (x, z)))
                .filter(|&(x, z)| {
                    let target = Target {
                        at_w1,
                        ..zombie(0, x, z)
                    };
                    weapon_at_ck(&shooter, &[target], &[]).is_some()
                })
                .count()
        };
        assert!(hits(500) > hits(0));
    }

    fn standing(slot: usize, x: i32, z: i32) -> Target {
        Target {
            ofs_y: -1000,
            at_h: 1000,
            ..zombie(slot, x, z)
        }
    }

    fn hits(effects: &[BlastEffect]) -> Vec<BlastHit> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                BlastEffect::Hit(hit) => Some(*hit),
                BlastEffect::Tested(_) => None,
            })
            .collect()
    }

    #[test]
    fn a_grenade_in_flight_hurts_every_body_in_reach() {
        let mut targets = [
            standing(1, -700, 0),
            standing(2, -600, 300),
            standing(3, 4000, 0),
        ];
        let blast = Blast {
            at: [0, -1000, 0],
            dir: 0,
            word: 0x3_0009,
        };
        let effects = blast_ck(&blast, &mut [-1400, 0, 350, 250], &mut targets);
        let hits = hits(&effects);
        assert_eq!(hits.iter().map(|hit| hit.slot).collect::<Vec<_>>(), [1, 2]);
        // `Em_10`'s row for weapon 9: 200 in the first field.
        assert!(hits.iter().all(|hit| hit.damage == 200 && hit.weapon == 9));
        assert_eq!(targets[0].life, 50 - 200);
        assert_ne!(targets[0].damage_cnt, 0);
        assert_eq!(targets[2].life, 50, "out of reach");
        // Each body it looked at starts its `Damage_flg` again.
        assert_eq!(effects.len(), 5);
    }

    #[test]
    fn a_burst_hurts_only_the_first_body_and_from_its_second_field() {
        let mut targets = [standing(1, -700, 0), standing(2, -600, 300)];
        let blast = Blast {
            at: [0, -1000, 0],
            dir: 0,
            word: 0x1002_0009,
        };
        let hits = hits(&blast_ck(&blast, &mut [-2000, 0, 1000, 500], &mut targets));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].slot, 1);
        assert_eq!(hits[0].damage, 50);
        // The second field's `Damage_no`s are 3 to 5.
        assert!((3..=5).contains(&hits[0].damage_no));
    }

    #[test]
    fn a_body_just_hit_is_passed_by_and_the_box_stays_wide() {
        let mut targets = [Target {
            at_w1: 400,
            ..standing(1, -700, 0)
        }];
        let blast = Blast {
            at: [0, -1000, 0],
            dir: 0,
            word: 0x3_0009,
        };
        let mut rect = [-1400, 0, 350, 250];
        assert_eq!(hits(&blast_ck(&blast, &mut rect, &mut targets)).len(), 1);
        assert_eq!(rect, [-1400, 0, 450, 350], "a hit leaves the box widened");
        assert!(blast_ck(&blast, &mut rect, &mut targets).is_empty());
    }

    #[test]
    fn a_shotgun_spreads_wider_than_a_magnum() {
        let hits = |weapon: u8, side: i32| {
            let shooter = Shooter {
                pos: [0; 3],
                neck: [0, -2000, 0],
                angle: 0,
                weapon,
                level: 1,
            };
            weapon_at_ck(&shooter, &[zombie(0, 1600, side)], &[]).is_some()
        };
        for weapon in [2, 5, 6, 7, 8] {
            assert!(hits(weapon, 0), "weapon {weapon} misses straight ahead");
        }
        assert!(!hits(5, 1500));
        assert!(hits(7, 1500));
        // The rest aren't ported.
        assert!(!hits(9, 0));
    }

    /// `Wp_dmg_tbl`'s row 0x12 (weapon 0x12 less one, at `+0x154` of each
    /// body's table) as the executable has it: the damage word, the stun
    /// word and six heights, little-endian. Each id arklay ports reads its own.
    #[test]
    fn the_gatling_guns_damage_is_the_executables_row() {
        const ROWS: [(u8, &str); 23] = [
            (0x10, "082080000a04020178ecf40148f4d0070cfeb80b"),
            (0x15, "082080000a04020178ecf40148f4d0070cfeb80b"),
            (0x1F, "082080000a04020178ecf40148f4d0070cfeb80b"),
            (0x20, "124820010a06830178ec320030f8d0070cfeb80b"),
            (0x21, "1e78e0010a1e8f0790e848f460f00cfe0cfeb80b"),
            (0x22, "145040010a0a850278ec30f854f2f4010cfed007"),
            (0x23, "124820010a140a0578ecf40148f4d0070cfeb80b"),
            (0x24, "145040010a0a850278ec30f854f2f4010cfed007"),
            (0x25, "124820010a0a850278ec30f898eff4010cfed007"),
            (0x26, "124820010a0a850278ec30f898eff4010cfed007"),
            (0x27, "114410010a0a850278ec30f848f4d0070cfeb80b"),
            (0x28, "114410010a0a850278ecf40148f4d0070cfeb80b"),
            (0x29, "071c70000a02810060f010f5e4f350fb50fb6400"),
            (0x2A, "186080010a12890480f3640018fc400618fc280a"),
            (0x2D, "0e38e0000a02810048f430f848f4d0070cfeb80b"),
            (0x2E, "0a28a0000a0c060348f4f40148f4d0070cfeb80b"),
            (0x2F, "0a28a0000a0c060348f4f40148f4d0070cfeb80b"),
            (0x30, "104000010a0a850278ecf40148f4d0070cfeb80b"),
            (0x31, "124820010a0a850278ecf40190e8d0070cfeb80b"),
            (0x33, "145040010a0e870378ecf40148f4d0070cfeb80b"),
            (0x39, "0a28a0000a0c060348f4f40148f4d0070cfeb80b"),
            (0x17, "082080000a04020178ecf40148f4d0070cfeb80b"),
            (0x1C, "082080000a04020178ecf40148f4d0070cfeb80b"),
        ];
        for (id, hex) in ROWS {
            let bytes: Vec<u8> = (0..hex.len())
                .step_by(2)
                .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
                .collect();
            let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
            let height =
                |n: usize| i32::from(i16::from_le_bytes([bytes[8 + n * 2], bytes[9 + n * 2]]));
            let row = gatling_damage(id).unwrap();
            assert_eq!(row.damage, word(0), "id {id:#x}'s damage");
            assert_eq!(row.stun, word(4), "id {id:#x}'s stun");
            assert_eq!(
                row.heights,
                std::array::from_fn(height),
                "id {id:#x}'s heights"
            );
            assert_eq!(damage_table(id, GATLING).unwrap().damage, word(0));
        }
    }

    /// `At_tbl_top`'s gatling records (`0x539D68`) as the executable has them:
    /// three levels of 28 bytes, a byte, the three reaches and three
    /// rectangles of four shorts each.
    #[test]
    fn the_gatling_guns_records_are_the_executables() {
        const BYTES: &str = concat!(
            "0004020164000000e803f40104100000e8037102a41f0000c4097102",
            "00080410c80000004c1df401c80000004c1df401c80000004c1df401",
            "0010204064000000e803770104100000e803f40108200000c4097102",
        );
        let bytes: Vec<u8> = (0..BYTES.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&BYTES[at..at + 2], 16).unwrap())
            .collect();
        let records = shot_records(GATLING).unwrap();
        for (level, record) in records.iter().enumerate() {
            let at = &bytes[level * 28..level * 28 + 28];
            assert_eq!(record.reach, [at[1], at[2], at[3]], "level {level}'s reach");
            let short = |n: usize| i16::from_le_bytes([at[4 + n * 2], at[5 + n * 2]]);
            let rects: [[i16; 4]; 3] =
                std::array::from_fn(|rect| std::array::from_fn(|n| short(rect * 4 + n)));
            assert_eq!(record.rects, rects, "level {level}'s rectangles");
        }
    }

    /// `Weapon_at_ck`'s end for a Beretta hit (#651): two `Rnd`s choose the
    /// magnum's row as weapon 6, when `(r2 >> (r1 & 3)) & 0xF` is 0, or else
    /// the handgun's as weapon 3, each read at the hit's band. Any other
    /// weapon's hit draws nothing and stays as it is.
    #[test]
    fn a_beretta_hit_lands_as_a_magnums_or_a_handguns() {
        use crate::zombie2::Rnd;
        let beretta = |id| Hit {
            slot: 1,
            id,
            weapon: BERETTA,
            high: false,
            band: 1,
            region: 2,
            damage: 0,
            stun: 0,
            count: 0,
            flags: 0x12,
        };
        let outcome = |seed: u16| {
            let mut replay = Rnd(seed);
            let shift = replay.draw() & 3;
            (replay.draw() >> shift) & 0xF == 0
        };
        // Literal draws: from 0x1234, 88 then 160 (0xA0 >> 0 & 0xF is 0), the
        // magnum's row as weapon 6; from 0x2679, 197 then 94 (0x5E >> 1 & 0xF
        // is 15), the handgun's as weapon 3. A zombie at band 1.
        for (seed, weapon, damage, stun, count, after) in [
            (0x1234, 6, 900, 1, 15, 0x48A0),
            (0x2679, 3, 15, 2, 5, 0x995E),
        ] {
            let mut rnd = Rnd(seed);
            let mut hit = beretta(0x10);
            beretta_hit(&mut hit, &mut rnd);
            assert_eq!(
                (hit.weapon, hit.damage, hit.stun, hit.count, rnd.0),
                (weapon, damage, stun, count, after),
                "from {seed:#x}"
            );
        }
        let magnum_seed = (0..=u16::MAX).find(|&seed| outcome(seed)).unwrap();
        let handgun_seed = (0..=u16::MAX).find(|&seed| !outcome(seed)).unwrap();
        for id in [0x10, LICKER_ID, G3_ID] {
            for (seed, row, weapon) in [(magnum_seed, 5, 6), (handgun_seed, 2, 3)] {
                let mut rnd = Rnd(seed);
                let mut hit = beretta(id);
                beretta_hit(&mut hit, &mut rnd);
                let mut want = Hit {
                    weapon,
                    ..beretta(id)
                };
                want.take_row(damage_table(id, row).unwrap());
                assert_eq!(hit, want, "id {id:#x}, row {row}");
                assert_ne!(hit.damage, 0);
                let mut two = Rnd(seed);
                two.draw();
                two.draw();
                assert_eq!(rnd.0, two.0, "two draws");
            }
        }
        let mut rnd = Rnd(magnum_seed);
        let mut hit = Hit {
            weapon: 2,
            ..beretta(0x10)
        };
        beretta_hit(&mut hit, &mut rnd);
        assert_eq!((hit.weapon, hit.damage, rnd.0), (2, 0, magnum_seed));
    }
}
