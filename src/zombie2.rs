//! RE2's zombies: `Em_Zombie`, which runs ids 0x10 to 0x1F in the enemy slots.
//!
//! This holds `Em10_Init`, `Em10_Move`'s states and the checks around them,
//! and `Em10_Damage` and `Em10_Die` for the handguns. The grab isn't ported yet
//! (`docs/re2-deviations.md`).

use crate::player2::{self, Bank, Mover, Tick, Work, direction_ck, goto};
use crate::scd2::host::Character;
use crate::state::{Guide, ScaRect};

/// `Rnd`, the stream everything random in the game shares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rnd(pub u16);

/// The low word of what `Player_set` seeds it with at every room's load.
pub const RND_SEED: u16 = 0x6CA4;

impl Default for Rnd {
    fn default() -> Self {
        Self(RND_SEED)
    }
}

impl Rnd {
    pub fn draw(&mut self) -> u8 {
        let high = ((u32::from(self.0) * 2) >> 8) as u8;
        let low = (self.0 as u8).wrapping_add(high);
        self.0 = u16::from(high) << 8 | u16::from(low);
        low
    }

    /// `Rnd() ; Rnd() >> (first & 3)`, the two draws most choices make.
    fn shifted(&mut self) -> u8 {
        let shift = self.draw() & 3;
        self.draw() >> shift
    }
}

/// `Em10_Life` and `Em10_Life_easy`.
const LIFE: [i16; 16] = [
    70, 84, 118, 65, 50, 85, 48, 65, 40, 73, 69, 56, 70, 55, 72, 55,
];
const LIFE_EASY: [i16; 16] = [
    39, 13, 27, 39, 54, 27, 39, 85, 27, 39, 27, 27, 39, 27, 39, 27,
];
/// `WORD_00526eb8`: a wander's stretches, whose top bit turns it away from the
/// player when they're far.
const STRETCHES: [u16; 32] = [
    0x00BE, 0x8032, 0x0096, 0x8040, 0x006E, 0x8040, 0x00B4, 0x8034, 0x008C, 0x8034, 0x0078, 0x803A,
    0x0096, 0x804E, 0x008C, 0x8032, 0x0096, 0x803E, 0x0096, 0x8048, 0x006E, 0x8046, 0x0078, 0x804A,
    0x00B4, 0x8022, 0x006E, 0x8034, 0x00C8, 0x8042, 0x00B4, 0x8042,
];
/// `Em10_Init`'s walks, by two of `Rnd`: clip 0 or clip 2.
const WALKS: [u8; 8] = [0, 2, 0, 2, 0, 2, 2, 0];
/// `FUN_00453e40`'s holds on the floor.
const FLOOR_HOLDS: [u16; 16] = [
    0x5A, 0x1E, 0xAA, 0xA0, 10, 0x28, 100, 0x32, 0x6E, 0x14, 0x23, 0x46, 0x1E, 0xB4, 0x1E, 0x3C,
];

/// The colour `Em10_Scenario` gives the shadow under a body.
pub const POOL_COLOR: u32 = 0x00BF_BF10;
/// `FUN_00456780`'s first part starts at `0x40`, and goes down to `0x10`.
const CHARRED_MAX: u8 = 0x30;
/// `Em10Dm00`'s `funcs_458795 + 0xb`: what a hit takes from `At_sce_no`, by weapon.
const STAGGER: [i8; 5] = [0, 9, 15, 15, 35];

/// `Type`'s bits.
const TYPE_BURNT: u16 = 0x80;
const TYPE_STRAIGHT: u16 = 0x1000;
const TYPE_ON_FLOOR: u16 = 0x2000;
pub const TYPE_HELD: u16 = 0x4000;
/// `Be_flg`'s bits: it's hit low, it's hit high.
pub const BE_LOW: u32 = 0x400_0000;
pub const BE_HIGH: u32 = 0x800_0000;
const BE_NO_SCA: u32 = 8;
/// The word at `+0x21a`.
/// `At_em_no` when nothing's touching, and the player's number.
pub const NO_BODY: u8 = 0xFF;
pub const PLAYER_BODY: u8 = 0xFE;
const FLAG_DOWN: u16 = 2;
const FLAG_FACE_DOWN: u16 = 4;
const FLAG_THROWN: u16 = 8;
const FLAG_RISING: u16 = 0x10;
const FLAG_LOST_ARM: u16 = 0x20;
const FLAG_LOST_OTHER_ARM: u16 = 0x40;
const FLAG_NECK_STILL: u16 = 0x200;
const FLAG_GUIDED: u16 = 0x400;
const FLAG_BURNING: u16 = 0x1000;
const FLAG_SMOKING: u16 = 0x8000;
/// `FUN_00453510`'s `|= 0x80` when the crawler keeps its head.
const FLAG_HEAD_LEFT: u16 = 0x80;
/// `Status_flg`'s bit: `Root_ck`'s line to the player is clear.
const STATUS_IN_SIGHT: u16 = 0x800;
/// `Now_seq`'s bits.
const SEQ_SOUND: u32 = 0x800_0000;
const SEQ_BLOOD: u32 = 0x1_0000;

/// A collision body, `AT_DATA`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Atd {
    pub ofs_y: i16,
    pub at_w: u16,
    pub at_h: u16,
    pub w: u16,
    /// `X`, `Y` and `Z`, which `Oba_set_ofs` leaves.
    pub at: [i32; 3],
}

/// What another body shows `Oba_ck_hit`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Oba {
    pub slot: u8,
    pub be_flg: u32,
    pub atd: Atd,
}

/// `Oba_ck_hit`: the push out of `other`. The original's extra push, when the
/// body comes from over or under the other, reads a height nothing sets: on
/// the level, which is what's ported, it never runs.
pub fn oba_hit(other: &Oba, be_flg: u32, atd: &mut Atd, pos: &mut [i32; 3]) -> bool {
    if (be_flg | other.be_flg) & 2 != 0 || be_flg & other.be_flg & 0x1000 != 0 || be_flg & 4 != 0 {
        return false;
    }
    let reach = i32::from(atd.at_w) + i32::from(other.atd.at_w);
    let (dx, dz) = (atd.at[0] - other.atd.at[0], atd.at[2] - other.atd.at[2]);
    if ((dx + reach) as u32) > (reach * 2) as u32 || ((dz + reach) as u32) > (reach * 2) as u32 {
        return false;
    }
    let distance = square_root0(dz * dz + dx * dx);
    let over = reach - distance;
    if over <= 0 {
        return false;
    }
    let height = i32::from(atd.at_h) + i32::from(other.atd.at_h);
    let dy = atd.at[1] - other.atd.at[1];
    if dy <= -height || dy >= height {
        return false;
    }
    let mut push_x = (dx * over) / (distance + 1);
    let mut push_z = (dz * over) / (distance + 1);
    if other.be_flg & 0x10_0000 != 0 {
        if push_x.abs() > 100 {
            if push_x < 0 {
                push_x = -100;
            } else {
                push_z = 100;
            }
        }
        if push_z.abs() > 100 {
            push_z = if push_z < 0 { -100 } else { 100 };
        }
    }
    pos[0] += push_x;
    pos[2] += push_z;
    atd.at[0] += push_x;
    atd.at[2] += push_z;
    true
}

fn square_root0(value: i32) -> i32 {
    crate::sca::square_root0(value)
}

/// What a zombie keeps beyond a character's [`Work`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Zombie {
    pub life: i16,
    at_em_no: u8,
    /// The first byte at `+0x218`: the walks' clip.
    walk: u8,
    /// `+0x21a`.
    pub flags: u16,
    /// `Timer0` to `Timer3`.
    timers: (u16, u16, u8, i8),
    /// `+0x23b`-ish countdowns: between moans, and between lunges.
    voice: u8,
    bite: u8,
    pub damage_cnt: u8,
    /// `D_life_u`, `D_life_c` and `D_life_d`: how much more each region takes.
    d_life: [u8; 3],
    /// `At_obj_no`: it has been staggered, and hits now knock it back.
    at_obj_no: u8,
    /// `At_sce_no`: what's left before it next staggers.
    at_sce_no: i8,
    /// `Damage_no`: the shot's band times three and its region.
    damage_no: u8,
    /// `Damage_flg`: the shot's record and side.
    damage_flg: u16,
    /// `+0x228` and `+0x22c`: last tick's routine, and what a hit while rising
    /// goes back to.
    last_routine: [u8; 4],
    rising_from: [u8; 4],
    /// `+0x230`: which way up it's lying, for the clip that gets it up.
    rise_side: u8,
    /// `Em_set_flg`: the flag that stops it coming back.
    pub dead_flag: u8,
    /// `Em_dead_flg_on` ran this tick.
    pub died: bool,
    /// How far the blood pool's `Half_x` and `Half_z` have grown.
    pub pool: u16,
    /// `+0x150`: `Root_ck`'s turn, one tick in eight.
    sight_turn: u8,
    /// `Status_flg`.
    status: u16,
    /// `Sca_info`.
    sca_info: u32,
    /// `Floor` at `sce_em_set`.
    pub floor: u8,
    pub atd: Atd,
    /// `Model_type`.
    pub model_type: u8,
    /// `Em10_Init`'s skin, from 0: `Texture_Move`'s page.
    pub skin: u8,
    /// `Em_Zombie`'s moans and the like this tick, as `Snd_se_enem` plays them.
    pub sounds: Vec<u8>,
    /// What a burnt zombie's `FUN_00456780` lit: the effect, the part it's on
    /// and where on that part.
    flames: Vec<(usize, u8, [i32; 3])>,
    burnt: bool,
    /// How far `Em10_Scenario` has taken a burnt body's colours down.
    charred: u8,
}

impl Zombie {
    /// `FUN_00456780`'s colours scale what lights the parts, around a half for
    /// most, and `Em10_Scenario` takes them all down together until the first
    /// part's reach `0x10`. They're a tint of the whole model here.
    pub fn tint(&self) -> [u8; 3] {
        const PARTS_MEAN: u32 = 0x4E;
        let burnt = 0xA0 * (PARTS_MEAN - u32::from(self.charred)) / PARTS_MEAN;
        [if self.burnt { burnt as u8 } else { 0xFF }; 3]
    }
}

/// `Weapon_at_ck`'s end: what a shot does to the zombie it found. `from` is the
/// shooter's position.
pub fn take_hit(
    character: &mut Character,
    zombie: &mut Zombie,
    hit: &crate::weapon2::Hit,
    from: [i32; 3],
) {
    zombie.life = zombie.life.wrapping_sub(hit.damage);
    let dead = zombie.life < 0;
    character.work.routine = [if dead { 3 } else { 2 }, hit.weapon, 0, 0];
    let region = usize::from(2 - hit.region.min(2));
    let left = zombie.d_life[region].wrapping_sub(hit.stun);
    zombie.d_life[region] = if (left as i8) < 0 { 0xFF } else { left };
    let body = &character.body;
    let direction = i32::from(direction_ck(from[0], from[2], body.pos[0], body.pos[2]));
    let side = direction - i32::from(body.angle as i16);
    let mut flags = hit.flags;
    if (side + 0x400) & 0xFFF < 0x800 {
        flags |= 0x20;
    }
    if (side + 0x600) & 0xFFF < 0x400 {
        flags |= 0x40;
    }
    if (side - 0x200) & 0xFFF < 0x400 {
        flags |= 0x80;
    }
    zombie.damage_flg = flags;
    zombie.damage_no = hit.band * 3 + hit.region;
    zombie.damage_cnt = (zombie.damage_cnt & 0x80) | hit.count;
}

/// The slot's first state: what `Sce_em_set` leaves.
pub fn spawned(slot: u8, floor: u8, model_type: u8) -> Zombie {
    Zombie {
        sight_turn: slot.wrapping_add(2) & 7,
        floor,
        model_type,
        ..Zombie::default()
    }
}

impl Zombie {
    /// What `FUN_00453510` and `FUN_0045a880` check of a zombie before they throw
    /// it back: alive, standing, not thrown already. `Type`'s `0x2000` is a
    /// zombie on the floor.
    pub fn can_be_knocked(&self, kind: u16) -> bool {
        self.life >= 0 && kind & 1 == 0 && kind & 0x2000 == 0 && self.flags & FLAG_THROWN == 0
    }

    /// `+0x16b`: which way the zombie goes.
    pub fn knocked(&mut self, back: bool) {
        self.timers.3 = i8::from(back);
    }
}

/// The rest of the game, as a zombie sees it.
pub struct World<'a> {
    pub player: [i32; 3],
    pub player_neck: Option<[i32; 3]>,
    pub player_floor: u8,
    /// The player's `Move_no`.
    pub player_clip: u8,
    /// The player's `+0x1d3`: an enemy has hold of them, or is about to.
    pub lock: &'a mut u8,
    pub slot: usize,
    pub pl: &'a mut Work,
    pub player_angle: u16,
    pub player_life: &'a mut i16,
    /// `Gacha_ch`: a button mashed this tick.
    pub gacha: bool,
    /// `Id`, which picks how a crawler's end looks.
    pub player_id: u8,
    /// The slots this zombie's stumble ran into.
    pub knocks: Vec<u8>,
    /// `WORD_00989eee`: the noises the player made.
    pub noise: u16,
    pub rnd: &'a mut Rnd,
    /// `splayer_work` is in play.
    pub partner: bool,
    /// `fg_system`'s easy bit.
    pub easy: bool,
    /// `BYTE_00989eeb`: how many enemies the room has.
    pub enemies: u8,
    pub stopped: bool,
    /// Every other body in play, `player_work`'s order.
    pub bodies: &'a [Oba],
    pub guides: &'a [Guide],
    pub sca: &'a [ScaRect],
    pub effects: &'a mut crate::esp2::Effects,
}

/// One tick's zombie: the state the original keeps in the work and globals.
struct Em<'a, 'b, 'c> {
    mover: Mover<'a, 'b>,
    z: &'a mut Zombie,
    be_flg: &'a mut u32,
    world: &'a mut World<'c>,
    distance: i32,
}

/// `Em_Zombie`. The caller has set `L_pl`'s position; `init` is `Em_init_move`'s
/// call, which comes before anyone has moved.
pub fn em_zombie(
    character: &mut Character,
    zombie: &mut Zombie,
    tick: &Tick<'_>,
    world: &mut World<'_>,
) {
    let Character {
        work, body, be_flg, ..
    } = character;
    let distance = square_root0(
        (body.pos[2] - world.player[2]).wrapping_mul(body.pos[2] - world.player[2])
            + (body.pos[0] - world.player[0]).wrapping_mul(body.pos[0] - world.player[0]),
    );
    let mut em = Em {
        mover: Mover::new(work, body, tick),
        z: zombie,
        be_flg,
        world,
        distance,
    };
    em.run();
}

impl Em<'_, '_, '_> {
    fn work(&mut self) -> &mut Work {
        self.mover.work
    }

    fn run(&mut self) {
        self.root_ck();
        if self.world.stopped {
            return;
        }
        if self.kind() & 1 != 0 || self.z.flags & FLAG_DOWN != 0 {
            *self.be_flg |= BE_LOW;
        }
        self.z.voice = self.z.voice.saturating_sub(1);
        self.z.bite = self.z.bite.saturating_sub(1);
        if self.z.damage_cnt & 0x7F != 0 {
            self.z.damage_cnt -= 1;
        }
        self.work().lean = 0;
        match self.work().routine[0] {
            0 => self.init(),
            1 => self.mv(),
            2 => self.em10_damage(),
            3 => self.em10_die(),
            7 => self.em10_corpse(),
            // `Spl_Scenario`, on the model's own bank.
            4 => {
                self.work().bank = Bank::Pld;
                self.mover.scenario();
            }
            _ => {}
        }
        let held = self.kind() & TYPE_HELD != 0 || self.z.flags & FLAG_NECK_STILL != 0;
        let neck = &mut self.work().neck;
        neck.flags = (neck.flags & !2) | if held { 2 } else { 0 };
        self.mover.rot_neck_em(self.world.player_neck);
        self.z.last_routine = self.work().routine;
        let seq = self.mover.now_seq();
        if seq & SEQ_SOUND != 0 {
            self.z.sounds.push((seq >> 28) as u8);
        }
        self.oba_ck_em();
        self.sca_ck_em();
        for (effect, part, point) in self.z.flames.clone() {
            if let Some(at) = self.mover.part_point(usize::from(part), point) {
                self.world
                    .effects
                    .move_to(effect, at.map(|axis| axis as i16));
            }
        }
    }

    /// `FUN_00456780`: fire on the body and darker colours.
    fn light_flames(&mut self) {
        const UP: [i32; 3] = [0, 200, 0];
        const DOWN: [i32; 3] = [0, -200, 0];
        const AT: [i32; 3] = [0, 0, 0];
        let flames = if self.rnd() & 1 == 0 {
            [(10000, 0, AT), (5000, 3, UP), (5096, 6, DOWN)]
        } else {
            [(10000, 0, UP), (5000, 8, AT), (5096, 3, UP)]
        };
        self.z.burnt = self.mover.tick.id != 0x10;
        let dir = self.mover.player.angle as i16;
        for (size, part, point) in flames {
            if let Some(effect) = self.world.effects.call(5, 3, size, dir, [0; 3], false) {
                self.z.flames.push((effect, part, point));
            }
        }
    }

    fn kind(&mut self) -> u16 {
        self.work().kind
    }

    /// `Esp_call` of the blood's type. The original's follows the part's matrix;
    /// here it stays where it starts.
    fn blood(&mut self, row: u8, size: u16, dir: i16, part: usize, point: [i32; 3]) {
        let at = self
            .mover
            .part_point(part, point)
            .unwrap_or(self.mover.player.pos);
        self.world
            .effects
            .call(0, row, size, dir, at.map(|axis| axis as i16), false);
    }

    /// A point on `pSin_parts_ptr[8]`, the head, for the blood it starts with.
    fn head_point(&self, x: i32, y: i32) -> [i32; 3] {
        let [hx, hy, hz] = self
            .mover
            .tick
            .skeleton
            .relative
            .get(8)
            .map_or([0; 3], |at| at.map(i32::from));
        [hx + x, hy + y, hz]
    }

    /// The first blood of `Em10Dm00`, `Em10Dm01` and `FUN_004587b0`: a shot at the
    /// head or the body, on the side it came from.
    fn hit_blood(&mut self, size: u16, offset: i32, dir: i16) {
        if self.z.damage_no.is_multiple_of(3) {
            self.blood(0, 0x17D0, dir, 1, [0, 800, 0]);
        } else {
            let point = self.head_point(offset, 300);
            self.blood(0, size, dir, 0, point);
        }
    }

    fn angle(&self) -> i16 {
        self.mover.player.angle as i16
    }

    fn player_held(&self) -> bool {
        (*self.world.lock as i8) < 0
    }

    /// `Em10_Damage`. Only the handguns reach here: the other weapons' branches,
    /// which light and freeze it, aren't ported.
    fn em10_damage(&mut self) {
        self.let_go();
        if self.kind() & 1 != 0 {
            self.dm_crawling();
            return;
        }
        if self.z.flags & FLAG_RISING != 0 {
            self.dm_rising();
            return;
        }
        let weapon = self.work().routine[1];
        let threshold = if self.kind() & 0x40 != 0 && weapon != 1 {
            0x17
        } else {
            0
        };
        if self.work().routine[2] == 0 && self.z.at_sce_no <= threshold {
            if self.z.at_obj_no == 1 || weapon == 1 {
                self.z.at_obj_no = 1;
                self.state(5, 0);
                self.z.at_sce_no = (self.rnd() & 0xF) as i8 + 0x10;
                let dir = self.angle();
                if !self.z.damage_no.is_multiple_of(3) {
                    let point = self.head_point(0, 500);
                    self.blood(0, 0x1000, dir, 0, point);
                } else {
                    self.blood(0, 0x17D0, dir, 1, [0, 800, 0]);
                }
                return;
            }
            self.z.at_obj_no = 1;
        }
        if self.z.flags & FLAG_DOWN != 0 {
            self.state(5, 6);
            self.z.timers.3 = 1;
            return;
        }
        if self.z.flags & (FLAG_LOST_ARM | FLAG_LOST_OTHER_ARM) == 0
            && (self.z.d_life[1] as i8) < 0
            && self.z.damage_flg & 0xC0 != 0
        {
            self.lose_arm();
        }
        self.dm00();
    }

    /// `Em10_Damage`'s arm coming off: which of them, by the side of the shot.
    /// What the original throws away as a part of its own is just no longer drawn.
    fn lose_arm(&mut self) {
        self.z.sounds.push(9);
        let part = if self.z.damage_flg & 0x80 != 0 {
            self.z.flags |= FLAG_LOST_OTHER_ARM;
            12
        } else {
            self.z.flags |= FLAG_LOST_ARM;
            9
        };
        // `Rnd() & 1` swaps which of the two ends bleed.
        self.rnd();
        let dir = u16::from(self.rnd()) << 4;
        self.blood(0, 8000, dir as i16, part, [0; 3]);
        let dir = u16::from(self.rnd()) << 4;
        self.blood(0, 7000, dir as i16, part + 1, [0; 3]);
        for hidden in part..part + 3 {
            player2::set_part_flags(self.work(), hidden as u8, 0);
        }
    }

    /// `Em10Dm00`.
    fn dm00(&mut self) {
        if self.z.at_obj_no == 1 {
            self.dm01();
            return;
        }
        let weapon = usize::from(self.work().routine[1]);
        let walk = self.z.walk;
        if self.work().routine[2] == 0 {
            if self.work().move_no != walk {
                self.clip(walk, 0, 0xF);
            }
            if self.z.voice == 0 {
                let slot = if self.rnd() & 1 == 0 { 0xC } else { 0xB };
                self.moan(slot);
            }
            self.z.timers.3 = 0x18;
            let count = self.work().move_cnt;
            self.work().move_cnt = 0;
            self.spd(Bank::Pld);
            self.work().move_cnt = count;
            self.spd(Bank::Pld);
            self.work().routine[2] = 1;
            self.z.timers.0 = 0;
            self.z.timers.1 = 1;
            let stagger = STAGGER.get(weapon).copied().unwrap_or(0);
            self.z.at_sce_no = self.z.at_sce_no.wrapping_sub(stagger);
            if self.kind() & 0x40 == 0 && self.z.at_sce_no < 1 && self.z.life > 0x50 {
                self.z.at_sce_no = (self.rnd() & 0xF) as i8 + 0x10;
            }
            let mut turn = 0;
            if self.z.damage_flg & 0x20 != 0 {
                self.z.timers.1 = 0xFFFF;
                turn = 0x800;
            }
            let dir = self.angle().wrapping_sub(turn);
            let side = self.z.timers.1 as i16;
            self.hit_blood(0x17D0, i32::from(side) * -100, dir);
            self.goto_dest(0x10);
        }
        match self.work().routine[2] {
            1 => {
                self.goto_dest(0x10);
                self.spd(Bank::Pld);
                let work = self.work();
                work.spd[0] >>= 1;
                work.spd[2] >>= 1;
                self.joint(Bank::Pld, 0x100);
                let lean = self.lean_now();
                self.work().lean = lean.wrapping_mul(-8);
                let old = self.z.timers.0 as i16;
                self.z.timers.0 = self.z.timers.0.wrapping_add(1);
                if old > 2 {
                    self.work().routine[2] = 2;
                    self.z.timers.0 = 0x10;
                }
            }
            2 => {
                self.goto_dest(0x10);
                self.spd(Bank::Pld);
                self.joint(Bank::Pld, 0x100);
                let lean = self.lean_now();
                self.work().lean = lean.wrapping_neg();
                let old = self.z.timers.0;
                self.z.timers.0 = old.wrapping_sub(1);
                if old == 0 {
                    self.work().routine[2] = 3;
                }
                if self.distance < 0x514
                    && !self.player_held()
                    && self.z.floor == self.world.player_floor
                    && !self.world.partner
                {
                    let angle = self.mover.player.angle;
                    let flags = self.z.flags;
                    if flags & FLAG_LOST_ARM == 0
                        && self.player_within(angle.wrapping_add(0x100), 0x120)
                    {
                        self.state(3, 0);
                    }
                    if flags & FLAG_LOST_OTHER_ARM == 0
                        && self.player_within(angle.wrapping_sub(0x100), 0x120)
                    {
                        self.state(3, 0);
                    }
                }
            }
            3 => {
                if self.turn_to_player(0x200) == 0
                    && self.distance < 4000
                    && !self.player_held()
                    && self.rnd() & 0xF == 0
                {
                    self.state(0xB, 0);
                    return;
                }
                if self.turn_to_player(0x400) != 0 && self.rnd() & 1 != 0 {
                    self.state(0xC, 0);
                    return;
                }
                if self.rnd() & 1 != 0 {
                    self.state(2, 0);
                    return;
                }
                self.state(1, 0);
                if self.work().move_no == walk {
                    self.state(1, 1);
                    let next = self.rnd();
                    self.z.timers.0 = 0x1E;
                    self.z.timers.3 = ((next & 0xF) * 2) as i8;
                    let next = self.rnd();
                    self.z.timers.2 = (next & 0x1F) + 0x1E;
                    let next = self.rnd();
                    self.z.timers.1 = u16::from(next) + 300;
                }
            }
            _ => {}
        }
        if self.work().move_no == walk {
            self.mover.add_speed_xz();
        }
    }

    /// `Timer3 * Timer1 * Timer0`: how far a hit has it leaning.
    fn lean_now(&self) -> i16 {
        i16::from(self.z.timers.3)
            .wrapping_mul(self.z.timers.1 as i16)
            .wrapping_mul(self.z.timers.0 as i16)
    }

    /// `Em10Dm01`: knocked back.
    fn dm01(&mut self) {
        match self.work().routine[2] {
            0 => {
                let back = u8::from(self.z.damage_flg & 0x20 != 0);
                self.clip(4 - back, 0, 3);
                self.work().routine[2] = 1;
                self.z.timers.1 = 10;
                self.work().spd = [if back == 0 { -450 } else { 0 }, 0, 0];
                self.mover.add_speed_xz();
                self.goto_dest(0x10);
                if self.z.voice == 0 {
                    let slot = if self.rnd() & 1 == 0 { 0xD } else { 0xC };
                    self.moan(slot);
                }
                let mut turn = 0;
                self.z.timers.2 = 1;
                if self.z.damage_flg & 0x20 != 0 {
                    turn = 0x800;
                    self.z.timers.2 = 0xFF;
                }
                let dir = self.angle().wrapping_sub(turn);
                let side = i32::from(self.z.timers.2 as i8);
                self.hit_blood(0x17D0, side * -100, dir);
                if self.z.damage_no.is_multiple_of(3) {
                    self.z.timers.2 = 0;
                }
            }
            1 => {}
            2 => {
                let at_sce = self.z.at_sce_no;
                self.state(1, 0);
                if at_sce < 1 {
                    let next = self.rnd();
                    self.z.at_obj_no = 0;
                    self.z.at_sce_no = (next & 0x10) as i8 + 0xF;
                }
                if self.turn_to_player(0x200) == 0
                    && self.distance < 4000
                    && !self.player_held()
                    && self.rnd() & 3 == 0
                {
                    self.state(0xB, 0);
                    return;
                }
                if self.distance < 3000 && self.turn_to_player(900) != 0 && self.rnd() & 1 != 0 {
                    self.state(0xC, 0);
                    return;
                }
                if self.rnd() & 1 != 0 {
                    self.state(2, 0);
                }
                return;
            }
            _ => return,
        }
        self.spd(Bank::Weapon);
        let ended = self.joint(Bank::Weapon, 0x400);
        self.step(ended);
        self.mover.add_speed_xz();
        if self.z.timers.1 != 0 && self.z.timers.2 != 0 {
            if self.z.timers.1 & 1 == 0 {
                let size = if self.z.at_obj_no == 0 {
                    8000
                } else {
                    self.work().spd[1] -= 500;
                    0x13E8
                };
                let (first, second) = (self.rnd(), self.rnd());
                let turn = (i16::from(first) - i16::from(second)).wrapping_mul(0x10);
                let dir = self.angle().wrapping_add(turn);
                self.blood(0, size, dir, 0, [0; 3]);
            }
            self.z.timers.1 -= 1;
        }
    }

    /// `FUN_00457d10`: a hit while crawling.
    fn dm_crawling(&mut self) {
        match self.work().routine[2] {
            0 => {
                let count = self.rnd() & 7;
                self.work().routine[2] = 1;
                self.clip(6, count, 3);
                self.blood(0, 6000, 0, 0, [0; 3]);
                if self.z.voice == 0 {
                    self.moan(0xC);
                }
            }
            1 => {}
            2 => {
                self.work().kind = 1;
                self.work().routine = [1, 0, 0, 0];
                return;
            }
            _ => return,
        }
        let ended = self.joint(Bank::Weapon, 0x400);
        self.step(ended);
    }

    /// `FUN_00457e60`: a hit while getting up, which only rocks it.
    fn dm_rising(&mut self) {
        if self.work().routine[2] == 0 {
            self.work().routine[2] = 1;
            self.z.timers.0 = 0;
            self.z.timers.1 = 1;
            self.z.timers.2 = 0;
            if self.z.damage_flg & 0x20 != 0 {
                self.z.timers.1 = 0xFFFF;
            }
            let dir = self.angle();
            self.blood(0, 0x17D0, dir, 8, [0, 300, 0]);
            if self.z.last_routine[0] == 1 {
                self.z.rising_from = self.z.last_routine;
            }
            self.work().hokan = 0xF;
            if self.z.voice == 0 {
                self.moan(0xC);
            }
            self.z.timers.3 = 0x18;
        }
        let ended = match self.work().routine[2] {
            1 => {
                let ended = self.joint(Bank::Weapon, 0x100);
                if !ended {
                    let lean = self.lean_now();
                    self.work().lean = lean.wrapping_mul(-8);
                    let old = self.z.timers.0 as i16;
                    self.z.timers.0 = self.z.timers.0.wrapping_add(1);
                    if old >= 3 {
                        self.work().routine[2] = 2;
                        self.z.timers.0 = 0x10;
                    }
                }
                ended
            }
            2 => {
                let ended = self.joint(Bank::Weapon, 0x100);
                if !ended {
                    let lean = self.lean_now();
                    self.work().lean = lean.wrapping_neg();
                    let old = self.z.timers.0;
                    self.z.timers.0 = old.wrapping_sub(1);
                    if old == 0 {
                        self.work().routine[2] = 3;
                    }
                }
                ended
            }
            3 => self.joint(Bank::Weapon, 0x100),
            _ => false,
        };
        if ended {
            self.z.flags &= !(FLAG_RISING | FLAG_DOWN);
            self.work().routine = self.z.rising_from;
            self.work().routine[2] = self.work().routine[2].wrapping_add(1);
            self.work().kind &= !TYPE_ON_FLOOR;
            *self.be_flg |= BE_LOW | BE_HIGH;
            self.z.atd.at_w = 500;
            self.z.atd.w = 500;
            self.z.atd.ofs_y = -0x5DC;
            self.z.atd.at_h = 0x5DC;
        }
    }

    /// `Em10_Die`.
    fn em10_die(&mut self) {
        self.let_go();
        self.z.flags &= !FLAG_NECK_STILL;
        if self.kind() & 1 != 0 {
            self.die_crawling();
        } else if self.z.flags & FLAG_RISING != 0 {
            self.die_rising();
        } else if self.z.flags & FLAG_DOWN == 0 {
            self.die_standing();
        } else {
            self.blood(0, 8000, 0, 0, [0; 3]);
            let slot = if self.rnd() & 1 == 0 { 0xB } else { 0xD };
            self.z.sounds.push(slot);
            self.work().routine = [7, 9, 0, 0];
        }
    }

    /// The end of a dying state's first tick: `Em_dead_flg_on` and the last moan.
    fn death_cry(&mut self) {
        let slot = if self.rnd() & 1 == 0 { 0xB } else { 0xD };
        self.z.sounds.push(slot);
        self.z.died = true;
    }

    /// `FUN_004587b0`.
    fn die_standing(&mut self) {
        if self.work().routine[2] == 0 {
            let fall = self.rnd() & 1;
            let fall = if self.z.flags & 0x2000 != 0 { 0 } else { fall };
            self.z.timers.2 = fall;
            self.z.flags &= !FLAG_FACE_DOWN;
            if fall != 0 {
                self.z.flags |= FLAG_FACE_DOWN;
            }
            self.work().routine[2] = 1;
            self.clip(1 + fall, fall * 10, 0xF);
            self.work().neck.flags |= 1;
            self.z.damage_cnt |= 0x80;
            let dir = self.angle();
            self.hit_blood(0x1FA0, 0, dir);
            self.death_cry();
            self.z.timers.3 = 0;
            self.work().spd[0] = 0xB;
        } else if self.work().routine[2] == 2 {
            self.work().routine = [7, 0, 0, 0];
            if self.rnd() & 3 != 0
                || self.z.flags & FLAG_FACE_DOWN != 0
                || self.z.timers.3 != 0
                || self.world.partner
            {
                return;
            }
            self.z.life = 1;
            self.z.atd.ofs_y = -0x15E;
            self.z.atd.at_h = 0x15E;
            self.z.atd.at_w = 200;
            self.z.atd.w = 200;
            *self.be_flg = (*self.be_flg & !BE_HIGH) | BE_LOW;
            self.work().kind = 0x2001;
            let next = self.rnd();
            self.work().routine = [1, if next & 7 != 0 { 2 } else { 0 }, 0, 0];
            self.z.damage_cnt &= 0x7F;
            return;
        } else if self.work().routine[2] != 1 {
            return;
        }
        if !self.z.damage_no.is_multiple_of(3) {
            let count = i32::from(self.work().move_cnt);
            let start = i32::from(self.z.timers.2) * 10;
            if start + 4 <= count && count <= start + 0x10 && count & 1 != 0 {
                let size = u16::from(self.rnd()) * 8 + 0xFD0;
                let dir = u16::from(self.rnd()) << 4;
                let point = self.head_point(0, 500);
                self.blood(0, size, dir as i16, 0, point);
            }
        }
        let planted = self.mover.foot_position();
        let ended = self.joint(Bank::Weapon, 0x100);
        self.step(ended);
        if self.work().hokan == 0 {
            self.mover.foot_set(planted);
        }
    }

    /// `FUN_00458bf0`.
    fn die_crawling(&mut self) {
        match self.work().routine[2] {
            0 => {
                self.z.flags &= !FLAG_FACE_DOWN;
                self.z.damage_cnt |= 0x80;
                self.clip(7, 0, 0xF);
                self.work().routine[2] = 1;
                self.work().neck.flags |= 1;
                let dir = self.angle();
                self.blood(0, 0x1FA0, dir, 0, [0, 300, 0]);
                self.death_cry();
            }
            1 => {}
            2 => {
                self.work().routine = [7, 0, 0, 0];
                return;
            }
            _ => return,
        }
        let ended = self.joint(Bank::Weapon, 0x100);
        self.step(ended);
    }

    /// `FUN_00459b60`: shot dead while getting up. The clip carries on from the
    /// rise's if it's early enough, by which way up it is.
    fn die_rising(&mut self) {
        /// The frame it's too late by, then the clips either side of it.
        const FALLS: [[u8; 3]; 3] = [[0x37, 0x16, 0x01], [0x28, 0x17, 0x01], [0x00, 0x01, 0x01]];
        match self.work().routine[2] {
            0 => {
                let side = usize::from(self.z.rise_side);
                self.z.flags |= FLAG_FACE_DOWN;
                if side == 1 {
                    self.z.flags &= !FLAG_FACE_DOWN;
                }
                let [limit, early, late] = FALLS[side.min(2)];
                if self.work().move_cnt < limit {
                    self.z.timers.3 = 0;
                    self.clip(early, 0, 0x1F);
                } else {
                    self.z.flags &= !FLAG_FACE_DOWN;
                    self.z.timers.3 = 1;
                    self.clip(late, 0x1E, 0x1F);
                }
                self.z.damage_cnt |= 0x80;
                self.work().routine[2] = 1;
                self.work().neck.flags |= 1;
                let dir = self.angle();
                let point = self.head_point(0, 300);
                self.blood(0, 0x1FA0, dir, 0, point);
                self.death_cry();
                self.work().spd[0] = 0xB;
            }
            1 => {}
            2 => {
                self.joint(Bank::Weapon, 0x80);
                if self.work().hokan == 0 {
                    self.work().routine[2] = 3;
                }
                return;
            }
            3 => {
                self.work().routine = [7, 0, 0, 0];
                return;
            }
            _ => return,
        }
        if self.joint(Bank::Weapon, 0x80) {
            self.work().routine[2] = 2 + u8::from(self.z.timers.3 != 0);
        }
    }

    /// `Em10_Scenario`, the state a body stays in: the pool under it spreads and
    /// a few of them keep twitching.
    fn em10_corpse(&mut self) {
        let mut joint = false;
        match self.work().routine[1] {
            0 => {
                let clip = if self.z.flags & FLAG_FACE_DOWN != 0 {
                    0x16
                } else {
                    0x17
                };
                self.clip(clip, 0, 7);
                self.work().neck.flags |= 1;
                self.z.life = -1;
                self.work().routine[1] = 1;
                *self.be_flg |= 2;
                self.z.timers.2 = 0x78;
                self.z.timers.0 = 0x28;
                self.work().spd[0] = 0;
                self.z.died = true;
                let next = self.rnd();
                if !next.is_multiple_of(3) {
                    self.work().routine[1] = 8;
                    return;
                }
                self.z.timers.1 = 0;
                self.spread_pool();
                joint = true;
            }
            1 => {
                self.spread_pool();
                joint = true;
            }
            5 | 10 => joint = true,
            2 => {
                let next = self.rnd();
                self.work().routine[1] = 3;
                self.z.timers.3 = (next & 0xF) as i8 + 1;
                let old = self.z.timers.0;
                self.z.timers.0 = old.wrapping_sub(1);
                if old != 0 {
                    self.corpse_wait();
                } else {
                    self.work().routine[1] = 4;
                    self.z.timers.0 = 0x1E;
                }
            }
            3 => self.corpse_wait(),
            4 => {
                let old = self.z.timers.3;
                self.z.timers.3 = old.wrapping_sub(1);
                if old == 0 {
                    self.work().routine[1] = 5;
                }
            }
            6 => {
                let next = self.rnd();
                self.work().routine[1] = 4;
                self.z.timers.3 = (next & 0x3F) as i8 + 0x3C;
                let old = self.z.timers.0;
                self.z.timers.0 = old.wrapping_sub(1);
                if old == 0 {
                    self.work().routine[1] = 7;
                }
            }
            8 => {
                self.spread_pool();
                if self.z.timers.2 == 0 {
                    self.work().routine[1] = 7;
                }
            }
            9 => {
                let clip = if self.z.flags & FLAG_FACE_DOWN != 0 {
                    0x16
                } else {
                    7
                };
                self.clip(clip, 0, 7);
                self.work().routine[1] = 10;
                joint = true;
            }
            0xB => self.work().routine[1] = 0,
            _ => {}
        }
        if joint {
            let ended = self.joint(Bank::Weapon, 0x200);
            self.work().routine[1] += u8::from(ended);
        }
        self.z.timers.1 = self.z.timers.1.wrapping_add(1);
        if self.kind() & TYPE_BURNT != 0 && self.z.charred < CHARRED_MAX && self.z.timers.1 & 3 == 0
        {
            self.z.charred += 1;
        }
    }

    fn corpse_wait(&mut self) {
        self.spread_pool();
        let old = self.z.timers.3;
        self.z.timers.3 = old.wrapping_sub(1);
        if old == 0 {
            self.work().routine[1] = 1;
        }
    }

    /// `Half_x` and `Half_z` grow while `Timer2` counts down.
    fn spread_pool(&mut self) {
        if self.z.timers.2 != 0 {
            self.z.timers.2 -= 1;
            self.z.pool += 5;
        }
    }

    /// `Oba_ck_em`.
    fn oba_ck_em(&mut self) {
        let pos = self.mover.player.pos;
        self.z.atd.at = [pos[0], pos[1] + i32::from(self.z.atd.ofs_y), pos[2]];
        self.z.at_em_no = NO_BODY;
        for other in self.world.bodies {
            let body = &mut self.mover.player.pos;
            if oba_hit(other, *self.be_flg, &mut self.z.atd, body) {
                self.z.at_em_no = other.slot;
            }
        }
    }

    /// `Sca_ck_em`, with the zombies' `0x400`.
    fn sca_ck_em(&mut self) {
        self.z.sca_info = 0;
        let radius = i32::from(self.z.atd.w);
        let room = self.mover.tick.room;
        let body = &mut *self.mover.player;
        if *self.be_flg & BE_NO_SCA == 0
            && let Some(sca) = &room.sca
        {
            let [old_x, old_z] = body.collision_mirror;
            let prev = [old_x, body.pos[1], old_z];
            let resolved = crate::sca::resolve_masked(sca, MASK, prev, body.pos, radius);
            // `Sca_ck_info`'s push moves what `Null_pos_set` adds to as well.
            let (dx, dz) = (resolved.pos[0] - body.pos[0], resolved.pos[2] - body.pos[2]);
            let base = &mut self.mover.work.base;
            base[0] = base[0].wrapping_add(dx as i16);
            base[2] = base[2].wrapping_add(dz as i16);
            // It moves the body through `Atd`, which others then push against.
            self.z.atd.at[0] += dx;
            self.z.atd.at[2] += dz;
            body.pos = resolved.pos;
            self.z.sca_info = u32::from(resolved.hit);
        }
        body.collision_mirror = [body.pos[0], body.pos[2]];
    }

    /// `Root_ck`, toward the player.
    fn root_ck(&mut self) {
        if self.z.sight_turn & 0x7F == 0 {
            self.z.sight_turn |= 7;
        }
        self.z.sight_turn = self.z.sight_turn.wrapping_sub(1);
        if self.z.sight_turn & 0x80 == 0 {
            self.z.status &= !STATUS_IN_SIGHT;
            let from = self.mover.work.neck.position;
            let to = self.world.player_neck;
            if let (Some(from), Some(to)) = (from, to)
                && !sca_ck_line(self.world.sca, from, to, 0x2000)
            {
                self.z.status |= STATUS_IN_SIGHT;
            }
        }
        let [target_x, _, target_z] = self.world.player;
        let kind = self.kind();
        if kind & TYPE_STRAIGHT != 0 {
            self.work().dest = [target_x as i16, target_z as i16];
            return;
        }
        if kind & TYPE_HELD != 0 || self.z.sight_turn & 0x80 != 0 {
            return;
        }
        let body = self.mover.player.pos;
        let (Some(here), Some(there)) = (
            guide_check(self.world.guides, body[0] as i16, body[2] as i16),
            guide_check(self.world.guides, target_x as i16, target_z as i16),
        ) else {
            return;
        };
        if here == there {
            self.work().dest = [target_x as i16, target_z as i16];
        } else if self.z.sight_turn == 0
            && let Some(dest) = kara_rootck2(
                self.world.guides,
                here,
                there,
                [body[0], body[2]],
                [target_x as i16, target_z as i16],
            )
        {
            self.work().dest = dest;
        }
    }

    fn joint(&mut self, bank: Bank, late: u16) -> bool {
        self.work().bank = bank;
        self.mover.joint_move_late(late)
    }

    fn spd(&mut self, bank: Bank) {
        self.work().bank = bank;
        self.mover.spd_get();
    }

    fn clip(&mut self, clip: u8, count: u8, hokan: u8) {
        let work = self.work();
        work.move_no = clip;
        work.move_cnt = count;
        work.hokan = hokan;
    }

    fn state(&mut self, one: u8, two: u8) {
        self.work().routine = [1, one, two, 0];
    }

    fn step(&mut self, ended: bool) {
        self.work().routine[2] += u8::from(ended);
    }

    /// `Snd_se_enem`, and the wait before the next.
    fn moan(&mut self, slot: u8) {
        self.z.sounds.push(slot);
        self.z.voice = 0x96;
    }

    fn rnd(&mut self) -> u8 {
        self.world.rnd.draw()
    }

    fn goto_dest(&mut self, rate: i32) {
        let [x, z] = self.work().dest;
        self.goto(i32::from(x), i32::from(z), rate);
    }

    fn goto_player(&mut self, rate: i32) {
        let [x, _, z] = self.world.player;
        self.goto(x, z, rate);
    }

    /// `Goto00`.
    fn goto(&mut self, x: i32, z: i32, rate: i32) {
        let body = &mut *self.mover.player;
        let target = direction_ck(body.pos[0], body.pos[2], x, z);
        body.angle = goto(body.angle, target, rate);
    }

    /// `Goto00_ck` toward the player: 0 once within `rate`, or the way to turn.
    fn turn_to_player(&mut self, rate: i32) -> i32 {
        let body = &*self.mover.player;
        let [x, _, z] = self.world.player;
        let target = direction_ck(body.pos[0], body.pos[2], x, z);
        turn_within(body.angle, target, rate)
    }

    /// `Dir_pos_ck`: the player lies within `0x100` of `angle`.
    fn player_at(&mut self, angle: u16) -> bool {
        self.player_within(angle, 0x100)
    }

    fn player_within(&self, angle: u16, within: i32) -> bool {
        let body = &*self.mover.player;
        let [x, _, z] = self.world.player;
        let direction = direction_ck(body.pos[0], body.pos[2], x, z);
        (i32::from(direction) + within - i32::from(angle)) & 0xFFF < within * 2
    }

    /// `Em10_Init`.
    fn init(&mut self) {
        self.work().routine = [1, 0, 0, 0];
        self.z.damage_cnt = 0;
        self.z.d_life = [0xD; 3];
        self.work().sce_flg = 0;
        let life = usize::from(self.world.rnd.shifted() & 0xF);
        self.z.life = LIFE[life];
        if self.world.easy {
            let life = usize::from(self.world.rnd.shifted() & 0xF);
            self.z.life = LIFE_EASY[life];
        } else if self.world.enemies < 4 {
            self.z.life += 0xF;
        }
        self.work().neck.flags = 0;
        let walk = self.world.rnd.shifted() & 7;
        self.z.walk = WALKS[usize::from(walk)];
        self.z.flags = 0;
        self.z.at_obj_no = 0;
        self.z.at_sce_no = (self.rnd() & 0xF) as i8 + 0x10;
        let id = self.mover.tick.id;
        if id == 0x11 {
            self.z.flags |= FLAG_SMOKING;
            self.z.life = 0xFA;
        }
        self.z.atd = Atd {
            ofs_y: -0x5DC,
            at_w: 500,
            at_h: 0x5DC,
            w: 500,
            at: [0; 3],
        };
        *self.be_flg |= BE_LOW | BE_HIGH;
        if matches!(id, 0x13 | 0x1E) {
            self.work().set_parts0_y(0xE66);
        }
        self.clip(1, 0, 0);
        let kind = self.kind() & 0x3F;
        if kind == 2 {
            self.z.flags |= FLAG_FACE_DOWN;
            self.lie_down(0x16);
        }
        if kind == 4 {
            self.lie_down(0x17);
        }
        if kind == 5 {
            self.z.flags |= FLAG_FACE_DOWN;
            self.lie_down(0x16);
            self.work().kind = 0x4002;
            self.z.life = -1;
        }
        if kind == 7 {
            self.lie_down(0x17);
            self.work().kind = 0x4004;
            self.z.life = -1;
        }
        let kind = self.kind();
        if kind & 0x3F == 6 {
            self.state(1, 0);
        }
        if kind & 0x3F == 8 {
            self.clip(0x12, 0, 0);
            self.state(8, 0);
        }
        if kind & 1 != 0 {
            self.clip(0x17, 0, 0);
            self.work().kind |= TYPE_ON_FLOOR;
            *self.be_flg = (*self.be_flg & !BE_HIGH) | BE_LOW;
            self.z.atd.ofs_y = -0x15E;
            self.z.atd.at_h = 0x15E;
            self.z.atd.at_w = 200;
            self.z.atd.w = 200;
            self.z.life /= 2;
        }
        match self.kind() & 0x3F {
            1 => self.state(2, 0),
            3 => self.state(0, 0),
            _ => {}
        }
        self.joint(Bank::Weapon, 0x100);
        if self.kind() & 0x3F == 10 {
            self.z.damage_cnt |= 0x80;
            self.z.flags = (self.z.flags & !FLAG_FACE_DOWN) | FLAG_DOWN;
            self.z.atd.at_w = 0;
            self.z.atd.w = 0;
            self.z.atd.ofs_y = -0x15E;
            self.z.atd.at_h = 0x15E;
            self.state(0xF, 0);
            *self.be_flg = (*self.be_flg & !BE_HIGH) | BE_LOW | BE_NO_SCA;
            self.clip(0, 0, 0);
            self.joint(Bank::Room, 0x100);
        }
        if id == 0x12 {
            self.rnd();
            self.rnd();
        }
        if id == 0x1F {
            let model_type = self.z.model_type;
            let mut skin = 0;
            if model_type & 0xC0 == 0 {
                let shift = self.rnd() & 3;
                let high = self.rnd();
                let low = self.rnd();
                skin = (u32::from(low) + (u32::from(high) << shift)) % 3 + 1;
            }
            if model_type & 0x3F != 0 {
                skin = u32::from(model_type & 0x3F);
            }
            self.z.skin = skin as u8;
        }
        if self.kind() & TYPE_BURNT != 0 {
            self.light_flames();
        }
        self.work().hidden_parts |= 1 << 0xF | 1 << 0x10;
    }

    fn lie_down(&mut self, clip: u8) {
        self.clip(clip, 0, 0);
        self.state(7, 0);
    }

    /// `Em10_Move`: what `Type` decides, then the state's motion.
    fn mv(&mut self) {
        let state = self.work().routine[1];
        if matches!(self.kind() & 0x3F, 1 | 3 | 5 | 7 | 9 | 0xB | 0xD) {
            match state {
                0 => self.look_crawl(),
                2 => self.look_flat(),
                _ => {}
            }
            let state = self.work().routine[1];
            match state {
                0 => self.mv_crawl(),
                1 => self.mv_grab(),
                2 => self.mv_flat(),
                _ => {}
            }
            return;
        }
        match state {
            0 => self.look_stand(),
            1 => self.look_walk(),
            2 => self.look_chase(),
            0xC => self.look_lunge(),
            _ => {}
        }
        let state = self.work().routine[1];
        match state {
            0 => self.mv_stand(),
            1 => self.mv_walk(),
            2 => self.mv_chase(),
            3 => self.mv_grab(),
            5 => self.mv_floor(),
            6 => self.mv_drop_body(),
            7 => self.mv_lying(),
            8 => self.mv_eat(),
            9 => self.mv_thrown(),
            0xA => self.mv_wall(),
            0xB => self.mv_fall(),
            0xC => self.mv_lunge(),
            0xD => self.mv_rise_again(),
            0xE => self.mv_vomit(),
            0xF => self.mv_room_clip(),
            _ => {}
        }
    }

    /// `FUN_00452630`.
    fn look_stand(&mut self) {
        let distance = self.distance;
        let wide = self.turn_to_player(0x400);
        let close = self.turn_to_player(800);
        let in_sight = self.z.status & STATUS_IN_SIGHT != 0;
        let noise = self.world.noise as u8;
        if distance < 5000 && wide == 0 && in_sight {
            self.state(1, 0);
        }
        if distance < 3000 && close != 0 && noise & 0x15 != 0 && in_sight && self.rnd() & 1 == 0 {
            self.state(0xC, 0);
        }
        if distance < 2000 && wide != 0 && in_sight && noise & 0x17 != 0 && self.rnd() & 1 != 0 {
            self.state(0xC, 0);
        }
    }

    /// `FUN_00452700`.
    fn mv_stand(&mut self) {
        if self.work().routine[2] == 0 {
            let count = self.rnd() & 0x1F;
            self.work().routine[2] = 1;
            self.clip(0, count, 0xF);
            let hold = self.rnd();
            self.z.timers.0 = u16::from(hold) + 0x96;
            let next = self.rnd();
            self.z.timers.1 = u16::from(next) + 300;
        }
        match self.work().routine[2] {
            1 => {
                let planted = self.mover.foot_position();
                self.joint(Bank::Weapon, 0x100);
                let timer = self.z.timers.1;
                self.z.timers.1 = timer.wrapping_sub(1);
                if timer == 0 {
                    if self.z.voice == 0 && self.rnd() & 1 != 0 {
                        self.moan(0xB);
                    }
                    let next = self.rnd();
                    self.z.timers.1 = u16::from(next) + 300;
                }
                if self.work().hokan == 0 {
                    self.mover.foot_set(planted);
                }
                if self.z.timers.1 == 0 && self.rnd() & 1 == 0 {
                    self.work().routine[2] = 2;
                }
                if self.z.timers.0 != 0 {
                    self.z.timers.0 -= 1;
                    return;
                }
                if self.distance < 0x1D4C && self.z.status & STATUS_IN_SIGHT != 0 {
                    self.state(1, 0);
                }
            }
            2 | 3 => {
                if self.work().routine[2] == 2 {
                    self.work().routine[2] = 3;
                    let walk = self.z.walk;
                    self.clip(walk, 0, 0xF);
                    let hold = self.rnd();
                    self.z.timers.0 = u16::from(hold) + 0x96;
                    // `Rnd_area`, whose choice nothing reads.
                    if !self.world.guides.is_empty() {
                        self.rnd();
                    }
                    self.z.flags |= FLAG_GUIDED;
                }
                self.goto_dest(0x10);
                self.spd(Bank::Pld);
                self.joint(Bank::Pld, 0x100);
                self.mover.add_speed_xz();
                self.z.timers.0 = self.z.timers.0.wrapping_sub(1);
                if self.z.timers.0 == 0 {
                    self.z.flags &= !FLAG_GUIDED;
                    self.work().routine[2] = 0;
                }
                let near = self.distance < 3000;
                if near || (self.distance < 0x1D4C && self.z.status & STATUS_IN_SIGHT != 0) {
                    self.z.flags &= !FLAG_GUIDED;
                    self.state(1, 0);
                }
            }
            _ => {}
        }
    }

    /// `FUN_00452930`.
    fn look_walk(&mut self) {
        let distance = self.distance;
        let wide = self.turn_to_player(0x400);
        let narrow = self.turn_to_player(0x200);
        let noise = self.world.noise as u8;
        if self.z.bite == 0 {
            let other_floor = self.z.floor != self.world.player_floor;
            if narrow == 0 && distance < 2000 && other_floor && *self.world.lock == 0 {
                self.state(0xE, 0);
            }
        }
        self.wall_check();
        if distance < 0xDAC && wide != 0 && noise & 0x15 != 0 && self.rnd() & 3 == 0 {
            self.state(0xC, 0);
        }
        if distance < 0x9C4 && wide != 0 && noise & 0x17 != 0 && self.rnd() & 1 == 0 {
            self.state(0xC, 0);
        }
        self.look_close(narrow);
    }

    /// The end of `FUN_00452930` and `FUN_00453010`.
    fn look_close(&mut self, narrow: i32) {
        let distance = self.distance;
        if self.world.player_clip == 0xF {
            if self.z.bite == 0 && narrow == 0 && distance < 2000 && *self.world.lock == 0 {
                self.state(0xE, 0);
            }
        } else if distance < 0x4B0
            && *self.world.lock & 0x80 == 0
            && self.z.floor == self.world.player_floor
        {
            self.reach_for_player(3);
        }
        // The player's body, once `Life` holds its `FUN_0045b070` mark.
        if *self.world.player_life == i16::MIN && narrow == 0 && distance < 1000 {
            self.work().kind |= TYPE_HELD;
            self.state(8, 6);
        }
    }

    /// The two checks of a reach, either side of the facing. Bits 0x20 and
    /// 0x40 at `+0x21a` are an arm the zombie has lost.
    fn reach_for_player(&mut self, state: u8) {
        let angle = self.mover.player.angle;
        if self.z.flags & 0x20 == 0 && self.player_at(angle.wrapping_add(0x100)) {
            self.state(state, 0);
            *self.world.lock |= 0x80;
        }
        if self.z.flags & 0x40 == 0 && self.player_at(angle.wrapping_sub(0x100)) {
            self.state(state, 0);
            *self.world.lock |= 0x80;
        }
    }

    /// `Sce_free1`'s scripted wall, once the zombie is against one.
    fn wall_check(&mut self) {
        if (self.work().sce_free[1] as u16) >> 8 & 0xC0 != 0 && self.z.sca_info & 1 != 0 {
            self.state(0xA, 0);
        }
    }

    /// `FUN_00452ba0`.
    fn mv_walk(&mut self) {
        if self.work().routine[2] == 0 {
            self.work().routine[2] = 1;
            let walk = self.z.walk;
            self.clip(walk, 0, 0xF);
            let stretch = self.rnd() & 0xF;
            self.z.timers.3 = (stretch * 2) as i8;
            let extra = self.rnd();
            self.z.timers.0 = u16::from(extra) + (self.stretch() & 0x7FFF);
            let moan = self.rnd();
            self.z.timers.2 = (moan & 0x1F) + 0x1E;
            self.spd(Bank::Pld);
            let count = self.rnd() & 0x1F;
            self.work().move_cnt = count;
            self.spd(Bank::Pld);
            let turn = self.rnd();
            self.z.timers.1 = u16::from(turn >> 3) + 100;
        } else if self.work().routine[2] != 1 {
            return;
        }
        let timer = self.z.timers.0;
        self.z.timers.0 = timer.wrapping_sub(1);
        if timer == 0 {
            self.z.timers.3 += 1;
            if self.z.timers.3 > 0x1F {
                self.z.timers.3 = 0;
            }
            let extra = self.rnd();
            self.z.timers.0 = u16::from(extra & 0x1F) + (self.stretch() & 0x7FFF);
            let lurch = self.rnd();
            let body = &mut *self.mover.player;
            body.angle = body
                .angle
                .wrapping_add(u16::from(lurch & 1) * 0x20)
                .wrapping_sub(0x10);
        }
        let rate = if self.distance < 0x1389 {
            8
        } else {
            8 - i32::from(self.stretch() >> 15) * 0x10
        };
        self.goto_dest(rate);
        let timer = self.z.timers.2;
        self.z.timers.2 = timer.wrapping_sub(1);
        if timer == 0 {
            let next = self.rnd();
            self.z.timers.2 = (next & 0xF) + 0x1E;
            self.goto_dest(0x10);
            if self.z.voice == 0 {
                if self.rnd() & 0x1F == 0 {
                    self.moan(10);
                } else if self.rnd() & 0x1F == 0 {
                    self.moan(0xB);
                }
            }
        }
        if self.distance < 3000 {
            self.goto_dest(0x10);
        }
        self.spd(Bank::Pld);
        self.joint(Bank::Pld, 0x100);
        if self.burnt_clip() {
            self.goto_dest(8);
            self.mover.skip_frame();
        }
        self.mover.add_speed_xz();
        self.z.timers.1 = self.z.timers.1.wrapping_sub(1);
        if self.z.timers.1 == 0 {
            let next = self.rnd();
            self.z.timers.1 = u16::from(next >> 3) + 100;
            if self.rnd() & 3 == 0 && self.distance < 5000 {
                self.state(2, 0);
            }
        }
        if self.burn_walk() {
            return;
        }
        if self.distance < 2000 && self.turn_to_player(0x200) == 0 && *self.world.lock & 0x80 == 0 {
            self.state(2, 0);
        }
        if self.world.noise & 4 != 0 && self.distance < 0x9C4 && self.turn_to_player(0x52C) == 0 {
            self.state(2, 0);
        }
    }

    fn stretch(&self) -> u16 {
        STRETCHES[usize::try_from(self.z.timers.3).unwrap_or(0) & 0x1F]
    }

    /// A burning zombie's walk runs on: `Type`'s `0x80` or the `0x8000` flag
    /// skips every third frame.
    fn burnt_clip(&mut self) -> bool {
        let burnt = self.kind() & TYPE_BURNT != 0 || self.z.flags & FLAG_SMOKING != 0;
        if burnt && u32::from(self.work().move_cnt) % 3 == 2 {
            let seq = self.mover.now_seq();
            if seq & SEQ_SOUND != 0 && seq >> 28 < 2 {
                self.z.sounds.push((seq >> 28) as u8);
            }
            return true;
        }
        false
    }

    /// The burns' end of `FUN_00452ba0`: true when they kill it. What that
    /// death does waits for `Em10_Die`.
    fn burn_walk(&mut self) -> bool {
        if self.kind() & TYPE_BURNT == 0 && self.z.flags & FLAG_BURNING == 0 {
            return false;
        }
        if self.z.flags & 0x7 == 0 && self.z.flags & 0x800 != 0 {
            let shift = self.rnd() & 3;
            let side = u16::from(self.rnd() >> shift & 1);
            let body = &mut *self.mover.player;
            body.angle = body.angle.wrapping_add(0x40).wrapping_sub(side * 0x80);
            self.z.life = self.z.life.wrapping_sub(1);
            if self.z.life < 0 {
                self.work().routine = [3, 10, 0, 0];
                return true;
            }
        }
        if self.z.flags & FLAG_BURNING != 0 {
            let lurch = self.rnd() >> 1;
            let body = &mut *self.mover.player;
            body.angle = body.angle.wrapping_add(0x40).wrapping_sub(u16::from(lurch));
        }
        false
    }

    /// `FUN_00453010`.
    fn look_chase(&mut self) {
        let distance = self.distance;
        let wide = self.turn_to_player(0x400);
        let narrow = self.turn_to_player(0x200);
        if self.z.bite == 0
            && narrow == 0
            && distance < 2000
            && self.z.floor != self.world.player_floor
            && *self.world.lock == 0
        {
            self.state(0xE, 0);
        }
        self.wall_check();
        if distance < 3000 && wide != 0 && self.world.noise as u8 & 0x17 != 0 && self.rnd() & 1 == 0
        {
            self.state(0xC, 0);
        }
        self.look_close(narrow);
    }

    /// `FUN_00453240`.
    fn mv_chase(&mut self) {
        if self.work().routine[2] == 0 {
            self.work().routine[2] = 1;
            let walk = self.z.walk;
            self.clip(walk + 4, 0, 0xF);
            let hold = self.rnd();
            self.z.timers.0 = u16::from(hold & 0x3F) + 0xB4;
            if self.z.voice == 0 {
                let slot = if self.rnd() & 1 == 0 { 10 } else { 0xB };
                self.moan(slot);
            }
            self.spd(Bank::Pld);
            let count = self.rnd() & 0x1F;
            self.work().move_cnt = count;
            self.spd(Bank::Pld);
            let turn = self.rnd();
            self.z.timers.2 = 0;
            self.z.timers.1 = u16::from(turn & 0x1F) + 0x2D;
        } else if self.work().routine[2] != 1 {
            return;
        }
        if self.z.timers.2 == 0 {
            self.goto_dest(0x10);
        } else {
            self.goto_dest(-0x10);
            self.z.timers.2 -= 1;
        }
        if self.z.sca_info & 1 != 0 {
            let timer = self.z.timers.1;
            self.z.timers.1 = timer.wrapping_sub(1);
            if timer == 0 {
                let turn = self.rnd();
                self.z.timers.1 = u16::from(turn & 0x1F) + 0x2D;
                let back = self.rnd();
                self.z.timers.2 = (back & 7) + 5;
            }
        }
        self.spd(Bank::Pld);
        self.joint(Bank::Pld, 0x100);
        if self.burnt_clip() {
            self.mover.skip_frame();
        }
        self.mover.add_speed_xz();
        if self.kind() & TYPE_BURNT != 0 || self.z.flags & FLAG_BURNING != 0 {
            if self.z.flags & 7 == 0 && self.z.flags & 0x800 != 0 {
                self.z.life = self.z.life.wrapping_sub(1);
                if self.z.life < 0 {
                    self.work().routine = [3, 10, 0, 0];
                    return;
                }
            }
            if self.z.flags & FLAG_BURNING != 0 {
                let lurch = self.rnd() >> 1;
                let body = &mut *self.mover.player;
                body.angle = body.angle.wrapping_add(0x40).wrapping_sub(u16::from(lurch));
            }
        }
        let timer = self.z.timers.0;
        self.z.timers.0 = timer.wrapping_sub(1);
        if timer == 0 || *self.world.lock & 0x80 != 0 {
            self.state(1, 0);
        }
        if self.distance < 0x1389 && self.turn_to_player(0x400) == 0 {
            return;
        }
        if (self.z.timers.0 as i16) < 0x78 {
            self.state(1, 0);
        }
    }

    /// `FUN_00453510`: the zombie takes hold of the player and bites.
    fn mv_grab(&mut self) {
        const CLIP: [u8; 2] = [0xB, 0xE];
        // The bite's frame and damage: standing, crawling; the second two rows
        // are ids 0x11 and 0x17.
        const BITE: [[(u8, i32); 2]; 2] = [[(0x10, 0x14), (1, 5)], [(0x10, 0x1E), (1, 10)]];
        let low = usize::from(self.kind() & 1 != 0);
        let tough = usize::from(matches!(self.mover.tick.id, 0x11 | 0x17));
        let (bite_frame, damage) = BITE[tough][low];
        let clip = CLIP[low];
        if self.work().routine[2] == 0 {
            self.work().routine[2] = 1;
            self.clip(clip, 0, 0xF);
            self.goto_player(0x800);
            self.work().bank = Bank::Weapon;
            self.world.pl.base = self.mover.set_base_pos();
            let behind = self.cdir_player();
            self.world.pl.routine = [player2::MODE_HELD, u8::from(behind) + 2 * low as u8, 0, 0];
            self.world.pl.held_by = Some(self.world.slot);
            *self.world.lock |= 0x80;
            self.z.damage_cnt = 0xF;
            *self.be_flg |= 0x1000;
            self.world.pl.be_flg |= 0x1000;
            if self.z.voice == 0 {
                let slot = if self.rnd() & 1 == 0 { 0xB } else { 0xA };
                self.moan(slot);
            }
        }
        match self.work().routine[2] {
            1 => {
                self.mover.null_pos_set(0);
                let ended = self.joint(Bank::Weapon, 0x100);
                self.step(ended);
            }
            2 | 3 => {
                if self.work().routine[2] == 2 {
                    self.work().routine[2] = 3;
                    self.z.timers.0 = 0x94;
                    self.clip(clip + 1, 0, 0);
                }
                self.mover.null_pos_set(0);
                self.joint(Bank::Weapon, 0x100);
                let shaken = if self.world.gacha { 5 } else { 0 };
                self.z.timers.0 = self.z.timers.0.wrapping_sub(shaken + 2);
                if (self.z.timers.0 as i16) < 0 {
                    self.work().routine[2] = 4;
                    self.world.pl.routine[2] = 4;
                }
                if self.work().move_cnt == bite_frame {
                    self.z.sounds.push(3);
                    match self.pl_life_down(damage, low != 0) {
                        1 => {
                            self.work().routine[2] = 4;
                            self.world.pl.routine[2] = 4;
                            // The player's `Timer2`: one more blood as they go.
                            self.world.pl.timers[2] = 1;
                        }
                        2 => {
                            self.state(6, 0);
                            let behind = self.cdir_player();
                            self.world.pl.routine =
                                [player2::MODE_HELD_DIE, u8::from(behind), 0, 0];
                        }
                        _ => {}
                    }
                }
            }
            4 | 5 => {
                if self.work().routine[2] == 4 {
                    self.work().routine[2] = 5;
                    self.z.timers.0 = 0;
                    self.z.timers.2 = 0;
                    self.clip(clip + 2, 0, 7);
                }
                if self.z.flags & (FLAG_LOST_ARM | FLAG_LOST_OTHER_ARM) == 0
                    && self.work().move_cnt == 3
                    && low == 0
                {
                    self.maybe_lose_arm();
                }
                self.mover.null_pos_set(0);
                let ended = self.joint(Bank::Weapon, 0x200);
                self.step(ended);
                if low != 0 {
                    self.stomped();
                } else if self.work().move_cnt == 7 {
                    self.work().routine[2] = 7;
                }
            }
            6 => {
                self.z.flags &= !FLAG_FACE_DOWN;
                self.work().routine = [7, 0, 0, 0];
                self.z.life = -1;
                *self.be_flg = (*self.be_flg & !0x800_0000) | 0x400_0000;
                self.z.damage_cnt |= 0x80;
            }
            7 | 8 => {
                if self.work().routine[2] == 7 {
                    self.spd(Bank::Weapon);
                    self.spd(Bank::Weapon);
                    self.work().routine[2] = 8;
                }
                self.spd(Bank::Weapon);
                let ended = self.joint(Bank::Weapon, 0x200);
                self.work().spd[0] = self.work().spd[0].wrapping_sub(0x1E);
                self.step(ended);
                self.mover.add_speed_xz();
                if self.work().move_cnt == 0x16 {
                    let shift = self.rnd() & 3;
                    let draw = self.rnd();
                    if (draw >> shift) & 0xF != 2 {
                        self.falls_after_shove();
                        return;
                    }
                }
                let count = self.work().move_cnt;
                if count > 6 && count < 0x19 && self.z.at_em_no != NO_BODY {
                    self.world.knocks.push(self.z.at_em_no);
                }
            }
            9 => self.state(0, 0),
            _ => {}
        }
    }

    /// `Cdir_ck` on the player: they face the same way as this body.
    fn cdir_player(&mut self) -> bool {
        let mine = self.mover.player.angle;
        self.world
            .player_angle
            .wrapping_sub(mine)
            .wrapping_add(0x400)
            & 0xFFF
            < 0x800
    }

    /// `Pl_life_down`. `hard_mode` and `censorship_off` are the title menu's,
    /// which leaves them 0.
    fn pl_life_down(&mut self, down: i32, low: bool) -> u8 {
        let (left, near_death, result) =
            life_down(*self.world.player_life, down, low, self.world.pl.near_death);
        *self.world.player_life = left;
        self.world.pl.near_death = near_death;
        if result == 2 {
            *self.world.lock |= 0x80;
        }
        result
    }

    /// `FUN_00453510`'s arm coming off in the shove: `Rnd`'s roll, and the side.
    fn maybe_lose_arm(&mut self) {
        let shift = self.rnd() & 3;
        let draw = self.rnd();
        if (draw >> shift) & 0xF != 0 {
            return;
        }
        let behind = self.cdir_player();
        let draw = self.rnd();
        let (flag, part) = if behind || draw & 2 != 0 {
            (FLAG_LOST_ARM, 9)
        } else {
            (FLAG_LOST_OTHER_ARM, 12)
        };
        self.z.flags |= flag;
        for hidden in part..part + 3 {
            player2::set_part_flags(self.work(), hidden as u8, 0);
        }
    }

    /// The shove's end for a crawler: which of two ends it gets goes by the
    /// player's `Id`, and it bleeds on odd frames either way.
    fn stomped(&mut self) {
        const LEFT_ITS_HEAD: u8 = 0x1B;
        let count = self.work().move_cnt;
        let id = self.world.player_id;
        if (id & 1 == 0 && id < 0xC) || matches!(id, 0xC | 0xD) {
            if count == 0x16 {
                player2::set_part_flags(self.work(), 8, 0);
                let dir = self.angle();
                self.blood(0, 0x1000, dir, 8, [0, -600, 0]);
                self.z.sounds.push(2);
            }
        } else if count == LEFT_ITS_HEAD {
            self.z.flags |= FLAG_HEAD_LEFT;
            self.z.sounds.push(8);
        }
        if count > 0x16 && count < 0x2A && count & 1 != 0 {
            let rnd = i32::from(self.rnd());
            let dir = (rnd * 4 - 0x200 + i32::from(self.angle())) as i16;
            self.blood(0, 0x1000, dir, 8, [0, 100, 0]);
        }
    }

    /// `FUN_00453510`'s stumble ending on the floor.
    fn falls_after_shove(&mut self) {
        self.work().routine = [1, 5, 1, 0];
        let sce = self.rnd() & 0xF;
        self.work().kind |= 0x2000;
        self.z.at_sce_no = sce as i8 + 0x10;
        self.z.timers.2 = 1;
        self.clip(2, 0x14, 10);
        self.z.flags = (self.z.flags & !FLAG_THROWN) | 0x206;
        *self.be_flg = (*self.be_flg & !0x800_0000) | 0x400_0000;
        self.z.damage_cnt |= 0x80;
        self.z.at_obj_no = 0;
        self.z.timers.3 = 0;
        let slot = if self.rnd() & 1 == 0 { 0xD } else { 0xC };
        self.z.sounds.push(slot);
        self.z.timers.0 = 1;
    }

    /// `Em10_Damage` and `Em10_Die`'s start: a shot lets go of the player.
    fn let_go(&mut self) {
        if self.world.pl.routine[0] == player2::MODE_HELD
            && self.world.pl.held_by == Some(self.world.slot)
        {
            *self.world.lock &= 0x7F;
            self.world.pl.neck.flags &= 0xED;
            self.world.pl.routine = [1, 0, 0, 0];
            self.world.pl.bank = Bank::Weapon;
            self.world.pl.be_flg &= !0x1004;
            *self.be_flg &= !0x1004;
        }
    }

    /// `FUN_00453c50`.
    fn look_crawl(&mut self) {
        if self.distance < 0x514
            && *self.world.lock & 0x80 == 0
            && self.z.floor == self.world.player_floor
        {
            self.reach_for_player(1);
        }
    }

    /// `FUN_00453d40`.
    fn mv_crawl(&mut self) {
        if self.work().routine[2] == 0 {
            self.clip(5, 0, 0xF);
            self.work().routine[2] = 1;
            let hold = self.rnd();
            self.z.timers.0 = u16::from(hold & 7) + 7;
            self.spd(Bank::Pld);
            let count = self.rnd() & 0xF;
            self.work().move_cnt = count;
            self.spd(Bank::Pld);
        } else if self.work().routine[2] != 1 {
            return;
        }
        if self.work().spd[0] > 0x14 {
            self.goto_dest(0x18);
            let timer = self.z.timers.0;
            self.z.timers.0 = timer.wrapping_sub(1);
            if timer == 0 {
                self.goto_dest(0x18);
                let hold = self.rnd();
                self.z.timers.0 = u16::from(hold & 7) + 7;
            }
        }
        self.spd(Bank::Pld);
        self.joint(Bank::Pld, 0x100);
        self.mover.add_speed_xz();
    }

    /// `FUN_004545c0`.
    fn look_flat(&mut self) {
        if self.turn_to_player(0x200) == 0
            && self.distance < 0x514
            && *self.world.lock & 0x80 == 0
            && self.z.status & STATUS_IN_SIGHT != 0
            && self.z.floor == self.world.player_floor
        {
            self.state(1, 0);
            *self.world.lock |= 0x80;
        }
    }

    /// `FUN_00454650`.
    fn mv_flat(&mut self) {
        if self.work().routine[2] == 0 {
            self.work().routine[2] = 1;
            self.work().move_no = 0x17;
        }
    }

    /// `FUN_00453e40`: on the floor, then back up.
    fn mv_floor(&mut self) {
        const DOWN: [u8; 2] = [1, 2];
        const LYING: [u8; 2] = [0x17, 0x16];
        const RISE: [u8; 2] = [8, 9];
        if self.work().routine[2] == 0 {
            let side = self.rnd() & 1;
            self.z.flags &= !FLAG_FACE_DOWN;
            self.z.timers.2 = side;
            if side != 0 {
                self.z.flags |= FLAG_FACE_DOWN;
            }
            self.z.flags |= FLAG_NECK_STILL | FLAG_DOWN;
            self.work().kind |= TYPE_ON_FLOOR;
            self.work().routine[2] = 1;
            *self.be_flg = (*self.be_flg & !BE_HIGH) | BE_LOW;
            self.clip(DOWN[usize::from(side)], side * 5 + 10, 0xF);
            self.z.damage_cnt |= 0x80;
            self.z.timers.3 = 0;
            if self.z.voice == 0 {
                let slot = if self.rnd() & 1 == 0 { 0xD } else { 0xC };
                self.moan(slot);
            }
            self.z.timers.0 = 0;
            self.work().spd[0] = 0xB;
        }
        match self.work().routine[2] {
            1 => {
                let planted = self.mover.foot_position();
                let ended = self.joint(Bank::Weapon, 0x100);
                self.step(ended);
                if self.z.atd.at_w > 0x14 {
                    self.z.atd.at_w -= 10;
                    if self.z.atd.w > 200 {
                        self.z.atd.w -= 10;
                    }
                }
                if self.work().hokan == 0 || self.z.timers.0 != 0 {
                    self.mover.foot_set(planted);
                }
            }
            2..=8 => self.mv_floor_rest(LYING, RISE),
            _ => {}
        }
    }

    fn mv_floor_rest(&mut self, lying: [u8; 2], rise: [u8; 2]) {
        let side = usize::from(self.z.timers.2 & 1);
        match self.work().routine[2] {
            2 => {
                self.work().routine[2] = 3;
                self.clip(lying[side], 0, 0xF);
                let hold = self.world.rnd.shifted() & 0xF;
                let extra = self.rnd();
                self.z.atd.ofs_y = -0x15E;
                self.z.timers.1 = FLOOR_HOLDS[usize::from(hold)] + u16::from(extra & 0xF);
                self.z.damage_cnt &= 0x7F;
                self.z.atd.at_w = 0;
                self.z.atd.w = 200;
                self.z.atd.at_h = 0x15E;
                let wait = self.rnd();
                self.work().spd[0] = i16::from(wait & 3);
                if self.kind() & TYPE_BURNT != 0 {
                    self.work().spd[0] = 0;
                    let hold = self.world.rnd.shifted() & 0xF;
                    self.z.timers.1 = FLOOR_HOLDS[usize::from(hold)] >> 1;
                }
                self.floor_hold();
            }
            3 => self.floor_hold(),
            4 | 5 => {
                if self.work().routine[2] == 4 {
                    let wait = self.rnd();
                    self.work().routine[2] = 5;
                    self.z.timers.0 = u16::from(wait & 0xF) + 3;
                }
                let timer = self.z.timers.0;
                self.z.timers.0 = timer.wrapping_sub(1);
                if timer == 0 {
                    self.work().routine[2] = 3;
                }
                self.floor_timer();
            }
            6 | 7 => {
                if self.work().routine[2] == 6 {
                    self.z.flags |= FLAG_RISING;
                    self.z.rise_side = u8::from(self.z.flags & FLAG_FACE_DOWN == 0);
                    self.clip(rise[side], 0, 0xF);
                    self.work().routine[2] = 7;
                    if self.z.voice == 0 && self.rnd() & 3 == 0 {
                        self.moan(0xB);
                    }
                    self.work().spd[0] = 0xB;
                }
                if self.z.atd.at_w < 500 {
                    self.z.atd.at_w += 10;
                    if self.z.atd.w < 500 {
                        self.z.atd.w += 10;
                    }
                }
                if self.z.atd.ofs_y > -0x5DC {
                    self.z.atd.at_h += 10;
                    self.z.atd.ofs_y -= 10;
                }
                if self.joint(Bank::Weapon, 0x100) {
                    self.z.flags &= !(FLAG_RISING | FLAG_DOWN);
                    self.work().routine[2] = 8;
                }
                if self.work().move_cnt == 0x37 {
                    *self.be_flg |= BE_LOW | BE_HIGH;
                }
            }
            8 => {
                self.work().kind &= !TYPE_ON_FLOOR;
                self.z.flags &= !(FLAG_RISING | FLAG_DOWN);
                self.z.damage_cnt &= 0x7F;
                self.state(1, 0);
                *self.be_flg |= BE_LOW | BE_HIGH;
                self.z.atd = Atd {
                    ofs_y: -0x5DC,
                    at_w: 500,
                    at_h: 0x5DC,
                    w: 500,
                    at: self.z.atd.at,
                };
            }
            _ => {}
        }
    }

    fn floor_hold(&mut self) {
        if self.work().spd[0] == 0 {
            let ended = self.joint(Bank::Weapon, 0x100);
            self.step(ended);
        }
        self.floor_timer();
    }

    fn floor_timer(&mut self) {
        let timer = self.z.timers.1;
        self.z.timers.1 = timer.wrapping_sub(1);
        if timer == 0 {
            self.work().routine[2] = 6;
        }
    }

    /// `FUN_004543c0`.
    fn mv_lying(&mut self) {
        let face_down = self.z.flags & FLAG_FACE_DOWN != 0;
        match self.work().routine[2] {
            0 => {
                self.clip(if face_down { 0x16 } else { 0x17 }, 0, 0);
                self.joint(Bank::Weapon, 0x100);
                self.work().routine[2] = 1;
                self.z.damage_cnt |= 0x80;
                self.work().neck.flags |= 1;
                self.lying_wait();
            }
            1 => self.lying_wait(),
            2 | 3 => {
                if self.work().routine[2] == 2 {
                    let count = self.rnd() & 0xF;
                    let clip = if face_down {
                        let count = self.rnd() & 0xF;
                        self.clip(9, count, 0xF);
                        9
                    } else {
                        self.clip(8, count, 0xF);
                        8
                    };
                    debug_assert!(clip >= 8);
                    self.work().routine[2] = 3;
                    if self.z.voice == 0 {
                        let slot = if self.rnd() & 1 != 0 {
                            0xC
                        } else if self.rnd() & 1 == 0 {
                            10
                        } else {
                            0xB
                        };
                        self.moan(slot);
                    }
                }
                let ended = self.joint(Bank::Weapon, 0x100);
                self.step(ended);
            }
            4 => {
                self.z.damage_cnt &= 0x7F;
                self.state(1, 0);
                *self.be_flg |= BE_LOW | BE_HIGH;
                self.work().neck.flags &= !1;
            }
            _ => {}
        }
    }

    fn lying_wait(&mut self) {
        if self.kind() & TYPE_HELD == 0 {
            self.work().routine[2] = 2;
        }
    }

    /// `FUN_00454520`: lets the dead player's body down, then eats it.
    fn mv_drop_body(&mut self) {
        match self.work().routine[2] {
            0 => {
                self.clip(0x18, 0, 0xF);
                self.work().routine[2] = 1;
                self.world.pl.base = self.mover.set_base_pos();
                let slot = if self.rnd() & 1 == 0 { 0xB } else { 0xA };
                self.moan(slot);
                self.z.damage_cnt |= 0x80;
            }
            1 => {}
            2 => {
                self.work().kind |= TYPE_HELD;
                self.state(8, 0);
                return;
            }
            _ => return,
        }
        self.mover.null_pos_set(0);
        let ended = self.joint(Bank::Weapon, 0x100);
        self.step(ended);
    }

    /// `FUN_00454670`: eating, or over the player's body.
    fn mv_eat(&mut self) {
        const CLIPS: [u8; 8] = [0x12, 0x13, 0x14, 0x12, 0x13, 0x14, 0x12, 0x13];
        match self.work().routine[2] {
            0 | 1 => {
                if self.work().routine[2] == 0 {
                    self.work().routine[2] = 1;
                    let clip = self.rnd() & 7;
                    self.clip(CLIPS[usize::from(clip)], 0, 7);
                    self.z.damage_cnt |= 0x80;
                }
                let planted = self.mover.foot_position();
                let ended = self.joint(Bank::Weapon, 0x200);
                self.step(ended);
                if self.work().hokan == 0 {
                    self.mover.foot_set(planted);
                }
                if self.mover.now_seq() & SEQ_BLOOD != 0 {
                    // `Esp_call`'s direction.
                    self.rnd();
                }
                if self.kind() & TYPE_HELD == 0 {
                    self.work().routine[2] = 3;
                }
            }
            2 => {
                let clip = self.rnd() & 7;
                self.work().routine[2] = 1;
                self.clip(CLIPS[usize::from(clip)], 3, 0);
            }
            3 | 4 => {
                if self.work().routine[2] == 3 {
                    self.z.flags |= FLAG_RISING;
                    self.z.rise_side = 2;
                    self.work().routine[2] = 4;
                    self.clip(0x15, 0, 0xF);
                    self.z.damage_cnt &= 0x7F;
                    if self.z.voice == 0 {
                        if self.rnd() & 1 == 0 {
                            if self.rnd() & 1 == 0 {
                                if self.rnd() & 1 != 0 {
                                    self.z.sounds.push(10);
                                }
                            } else {
                                self.z.sounds.push(0xB);
                            }
                        }
                        self.z.voice = 0x96;
                    }
                }
                if self.joint(Bank::Weapon, 0x100) {
                    self.z.flags &= !FLAG_RISING;
                    self.work().routine[2] = 5;
                }
            }
            5 => {
                self.state(1, 0);
                self.z.flags &= !FLAG_RISING;
                self.z.damage_cnt &= 0x7F;
            }
            6 | 7 => {
                if self.work().routine[2] == 6 {
                    self.work().routine[2] = 7;
                    let count = self.rnd() & 7;
                    self.clip(0x15, count, 0xF);
                }
                self.goto_player(0x10);
                self.work().bank = Bank::Weapon;
                let ended = self.mover.joint_move_late_reversed(0x100, true);
                self.step(ended);
            }
            8 => self.work().routine[2] = 0,
            _ => {}
        }
    }

    /// `FUN_00454900`: thrown off the player.
    fn mv_thrown(&mut self) {
        const CLIPS: [u8; 4] = [3, 3, 4, 0xD];
        const DOWN: [u8; 2] = [1, 2];
        match self.work().routine[2] {
            0 => {
                let side = self.rnd() & 1;
                let back = self.z.timers.3;
                self.z.timers.2 = side;
                let count = self.rnd() & 3;
                let index = usize::try_from(i32::from(back) * 2 + i32::from(side)).unwrap_or(0) & 3;
                self.clip(CLIPS[index], count, 0);
                if back != 0 && side != 0 {
                    let count = self.rnd() & 3;
                    self.work().move_cnt = count + 0x14;
                }
                self.z.timers.0 = (i32::from(back) << 11) as u16;
                self.work().routine[2] = 1;
                self.work().spd[0] = 400;
                self.work().spd[2] = 0;
                self.z.flags = (self.z.flags & !FLAG_RISING) | FLAG_THROWN;
                self.z.timers.1 = 0;
                if self.z.voice == 0 && self.rnd() & 1 != 0 {
                    self.moan(0xC);
                }
            }
            1 => {}
            2 => {
                self.z.flags &= !FLAG_THROWN;
                self.state(1, 0);
                return;
            }
            _ => return,
        }
        self.mover.add_speed_xz_at(self.z.timers.0);
        self.work().spd[0] -= 0x1E;
        if self.work().spd[0] < 0 {
            self.z.timers.1 += 1;
            self.work().spd[0] = 0;
        }
        if self.z.timers.1 == 1 && self.world.rnd.shifted() & 7 != 0 {
            self.work().routine = [1, 5, 1, 0];
            let wait = self.rnd();
            let _ = wait;
            self.z.flags &= !(FLAG_THROWN | FLAG_FACE_DOWN);
            let back = self.z.timers.3;
            self.z.timers.2 = back as u8;
            if back != 0 {
                self.z.flags |= FLAG_FACE_DOWN;
            }
            self.z.flags |= FLAG_NECK_STILL | FLAG_DOWN;
            self.work().kind |= TYPE_ON_FLOOR;
            *self.be_flg = (*self.be_flg & !BE_HIGH) | BE_LOW;
            let side = usize::try_from(back).unwrap_or(0) & 1;
            self.clip(DOWN[side], back as u8 * 5 + 10, 0xF);
            self.z.damage_cnt |= 0x80;
            self.z.timers.3 = 0;
            if self.z.voice == 0 {
                let slot = if self.rnd() & 1 == 0 { 0xD } else { 0xC };
                self.moan(slot);
            }
            self.z.timers.0 = 0;
            return;
        }
        let ended = self.joint(Bank::Weapon, 0x100);
        self.step(ended);
    }

    /// `FUN_00454b90`: at a wall a script chose, on the room's clips.
    fn mv_wall(&mut self) {
        const CLIPS: [u8; 3] = [0, 2, 4];
        let flag = 0x4000u16;
        self.work().sce_free[1] = (self.work().sce_free[1] as u16 | flag) as i16;
        match self.work().routine[2] {
            0 | 1 => {
                if self.work().routine[2] == 0 {
                    let count = self.rnd() & 3;
                    self.work().routine[2] = 1;
                    let walk = self.z.walk;
                    self.clip(walk, count, 0xF);
                    self.z.timers.2 = 0;
                    if self.work().sce_free[0] as u16 & 0x4000 != 0 {
                        let which = self.rnd() & 1;
                        self.z.timers.2 = which + 1;
                    }
                }
                self.joint(Bank::Pld, 0x100);
                let facing = (self.work().sce_free[0] as u16) & 0xFFF;
                let body = &mut *self.mover.player;
                let turn = turn_within(body.angle, facing, 0x20);
                body.angle = body.angle.wrapping_add(turn as u16);
                if turn == 0 {
                    self.work().routine[2] = 2;
                }
            }
            2 | 3 => {
                let table = CLIPS[usize::from(self.z.timers.2) % 3];
                if self.work().routine[2] == 2 {
                    let count = self.rnd() & 7;
                    let side = self.rnd() & 1;
                    self.work().routine[2] = 3;
                    self.clip(side + table, count, 0xF);
                    let hold = self.rnd();
                    self.z.timers.0 = u16::from(hold & 0x7F) + 0x3C;
                }
                if self.joint(Bank::Room, 0x100) {
                    let count = self.rnd() & 7;
                    let side = self.rnd() & 1;
                    self.clip(side + table, count, 0xF);
                }
                let seq = self.mover.now_seq();
                let free = self.work().sce_free[1] as u16 & 0x7FFF;
                let free = if seq & SEQ_BLOOD != 0 {
                    free | 0x8000
                } else {
                    free
                };
                self.work().sce_free[1] = free as i16;
                let timer = self.z.timers.0;
                self.z.timers.0 = timer.wrapping_sub(1);
                if timer == 0 {
                    self.work().routine[2] = 4;
                }
            }
            4 => self.state(1, 0),
            _ => {}
        }
    }

    /// `FUN_00454d60`: falling to the floor toward the player.
    fn mv_fall(&mut self) {
        match self.work().routine[2] {
            0 => {
                self.clip(10, 0, 0xF);
                self.work().routine[2] = 1;
                self.goto_dest(0x80);
                self.work().kind |= TYPE_ON_FLOOR;
                self.z.flags = (self.z.flags & !FLAG_FACE_DOWN) | FLAG_DOWN;
                *self.be_flg = (*self.be_flg & !BE_HIGH) | BE_LOW;
                self.z.timers.0 = 0;
                self.z.damage_cnt |= 0x80;
                self.z.timers.3 = 0;
                if self.z.voice == 0 {
                    let slot = if self.rnd() & 1 == 0 { 0xB } else { 10 };
                    self.moan(slot);
                }
            }
            1 => {}
            2 => {
                self.work().routine = [1, 5, 2, 0];
                self.rnd();
                self.z.timers.2 = 0;
                return;
            }
            _ => return,
        }
        self.z.timers.0 = self.z.timers.0.wrapping_add(1);
        if self.z.timers.0 & 1 == 0 {
            self.work().spd[0] >>= 2;
        } else {
            self.spd(Bank::Weapon);
        }
        let ended = self.joint(Bank::Weapon, 0x100);
        self.step(ended);
        self.mover.add_speed_xz();
        if self.work().move_cnt < 0x12 {
            self.goto_dest(0x10);
        } else if self.turn_to_player(0x100) == 0
            && self.distance < 0x708
            && *self.world.lock & 0x80 == 0
            && self.z.floor == self.world.player_floor
        {
            self.state(1, 0);
            let kind = self.kind();
            self.work().kind = (kind & 0xFFC1) | 1;
            *self.world.lock |= 0x80;
        }
    }

    /// `FUN_00454f30`.
    fn look_lunge(&mut self) {
        if self.work().move_no == 3
            && self.distance < 0x514
            && *self.world.lock & 0x80 == 0
            && self.work().move_cnt < 0xB
            && self.z.floor == self.world.player_floor
            && self.world.player_clip != 0xF
        {
            self.reach_for_player(3);
        }
    }

    /// `FUN_00455010`.
    fn mv_lunge(&mut self) {
        match self.work().routine[2] {
            0 | 1 => {
                if self.work().routine[2] == 0 {
                    self.clip(0x19, 0, 7);
                    self.work().routine[2] = 1;
                    self.goto_player(0x20);
                    self.z.timers.0 = self.turn_to_player(0xBE) as u16;
                    if self.z.voice == 0 {
                        self.moan(10);
                    }
                    self.work().spd[0] = 0xB;
                }
                if self.turn_to_player(0x140) == 0 {
                    self.z.timers.0 = 0;
                    self.work().routine[2] = 2;
                }
                let turn = self.z.timers.0;
                let body = &mut *self.mover.player;
                body.angle = body.angle.wrapping_add(turn);
                if self.joint(Bank::Weapon, 0x200) {
                    self.work().routine[2] = 2;
                }
            }
            2 | 3 => {
                if self.work().routine[2] == 2 {
                    self.clip(0x1B, 0, 7);
                    self.work().routine[2] = 3;
                }
                self.goto_player(0x20);
                self.spd(Bank::Weapon);
                if self.joint(Bank::Weapon, 0x200) {
                    self.state(1, 0);
                }
                self.mover.add_speed_xz();
                if self.work().move_cnt == 0x19 {
                    self.state(1, 0);
                }
            }
            _ => {}
        }
    }

    /// `FUN_00455170`: begun again, as a script's `Type` asks.
    fn mv_rise_again(&mut self) {
        self.work().routine = [1, 0, 0, 0];
        self.z.damage_cnt = 0;
        self.work().sce_flg = 0;
        let life = self.rnd() & 0xF;
        self.z.life = LIFE[usize::from(life)];
        let walk = self.rnd() & 7;
        self.z.walk = [0, 2][usize::from(walk & 1)];
        self.z.flags = 0;
        self.rnd();
        self.z.atd = Atd {
            ofs_y: -0x5DC,
            at_w: 500,
            at_h: 0x5DC,
            w: 500,
            at: self.z.atd.at,
        };
        *self.be_flg = BE_LOW | BE_HIGH | 1;
        self.clip(1, 0, 0);
        self.joint(Bank::Weapon, 0x100);
        if self.kind() as u8 & 0x3F == 6 {
            self.state(1, 0);
        }
        if self.mover.tick.id == 0x1F && self.z.model_type & 0xC0 == 0 {
            let skin = self.rnd() % 3 + 1;
            self.z.skin = skin;
        }
        self.work().hidden_parts = 1 << 0xF | 1 << 0x10;
    }

    /// `FUN_00455620`: `Esp_call`'s spit isn't ported.
    fn mv_vomit(&mut self) {
        if self.work().routine[2] == 0 {
            self.clip(0x11, 0, 0xF);
            self.work().routine[2] = 1;
            self.work().spd[0] = 0xB;
        } else if self.work().routine[2] != 1 {
            return;
        }
        if self.work().move_cnt == 10 {
            self.z.sounds.push(5);
        }
        if self.joint(Bank::Weapon, 0x100) {
            self.state(0, 0);
            self.z.bite = 0x3C;
        }
    }

    /// `FUN_004556b0`: the room's clip, at a script's word.
    fn mv_room_clip(&mut self) {
        match self.work().routine[2] {
            0 => {
                self.clip(0, 0, 0);
                self.joint(Bank::Room, 0x100);
                if self.work().sce_free[0] & 1 != 0 {
                    self.work().routine[2] = 1;
                    self.clip(0, 0, 0);
                }
            }
            1 => {
                self.spd(Bank::Room);
                if self.joint(Bank::Room, 0x100) {
                    self.work().kind &= !TYPE_HELD;
                    self.z.damage_cnt &= 0x7F;
                    self.z.timers.3 = 0;
                    self.z.timers.2 = 0;
                    self.work().routine = [1, 5, 6, 0];
                    *self.be_flg &= !BE_NO_SCA;
                }
                self.mover.add_speed_xz();
            }
            _ => {}
        }
    }
}

/// `Sca_ck_em`'s mask for a zombie.
pub const MASK: u16 = 0x400;

/// `Goto00_ck` and `Goto01_ck`: 0 once `angle` is within `rate` of `target`, or
/// the way to turn.
fn turn_within(angle: u16, target: u16, rate: i32) -> i32 {
    let error = (i32::from(target.wrapping_sub(angle)) + rate) & 0xFFF;
    if error < rate * 2 {
        0
    } else if error > 0x800 {
        -rate
    } else {
        rate
    }
}

/// `Guide_check`: the last area holding the point.
pub fn guide_check(guides: &[Guide], x: i16, z: i16) -> Option<u8> {
    guides
        .iter()
        .enumerate()
        .rev()
        .find(|(_, guide)| {
            (x.wrapping_sub(guide.x0) as u16) < (guide.x1.wrapping_sub(guide.x0) as u16)
                && (z.wrapping_sub(guide.z0) as u16) < (guide.z1.wrapping_sub(guide.z0) as u16)
        })
        .map(|(index, _)| index as u8)
}

/// The edge `kara_rootck2` crosses between two areas: its first corner, its
/// second, the point on it and whether it runs along z.
fn shared_edge(a: &Guide, b: &Guide) -> ([i16; 2], [i16; 2], [i16; 2], bool) {
    if a.z0 == b.z1 || a.z1 == b.z0 {
        let z = if a.z1 == b.z0 { a.z1 } else { a.z0 };
        let (x0, x1) = (a.x0.max(b.x0), a.x1.min(b.x1));
        let middle = ((i32::from(x0) + i32::from(x1)) >> 1) as i16;
        ([x0, z], [x1, z], [middle, z], false)
    } else {
        let x = if a.x1 == b.x0 { b.x0 } else { a.x0 };
        let (z0, z1) = (a.z0.max(b.z0), a.z1.min(b.z1));
        let middle = ((i32::from(z0) + i32::from(z1)) >> 1) as i16;
        ([x, z0], [x, z1], [x, middle], true)
    }
}

/// `kara_rootck2`: the way through the areas to the target. It searches every
/// route without crossing an area twice and heads for the best's first edge,
/// or straight for its second point if the first edge's corners allow.
pub fn kara_rootck2(
    guides: &[Guide],
    from: u8,
    to: u8,
    body: [i32; 2],
    target: [i16; 2],
) -> Option<[i16; 2]> {
    const DEPTH: usize = 16;
    let to_bit = 1u16 << (to & 31);
    let mut links = [0u16; DEPTH + 2];
    let mut visited = [0u16; DEPTH + 2];
    let mut cost = [0u16; DEPTH + 2];
    let mut nodes = [0u8; DEPTH + 2];
    let mut points = [[0i16; 2]; DEPTH + 3];
    let mut corners = [([0i16; 2], [0i16; 2]); DEPTH + 2];
    let mut along_z = [false; DEPTH + 2];
    let mut best = i32::MAX;
    let mut route = None;
    let mut level = 0usize;
    let mut current = from;
    links[0] = guides.get(usize::from(from))?.links;
    visited[0] = 1 << (from & 31);
    nodes[0] = from;
    points[0] = [body[0] as i16, body[1] as i16];
    while links[0] != 0 || level != 0 {
        let remaining = links[level];
        if remaining == 0 {
            level -= 1;
            current = nodes[level];
        } else if remaining & to_bit == 0 {
            let next = remaining.trailing_zeros() as u8;
            let bit = 1u16 << next;
            links[level] = remaining & !bit;
            if level >= DEPTH {
                continue;
            }
            let (a, b) = (
                guides.get(usize::from(current))?,
                guides.get(usize::from(next))?,
            );
            let (first, second, point, z) = shared_edge(a, b);
            corners[level + 1] = (first, second);
            along_z[level + 1] = z;
            points[level + 1] = point;
            let dx = (i32::from(points[level][0]) - i32::from(point[0])).abs();
            let dz = (i32::from(points[level][1]) - i32::from(point[1])).abs();
            cost[level + 1] = ((dx + dz) as u16).wrapping_add(cost[level]);
            nodes[level + 1] = next;
            visited[level + 1] = visited[level] | bit;
            links[level + 1] = b.links & !visited[level + 1];
            level += 1;
            current = next;
        } else {
            links[level] = remaining & !to_bit;
            let (a, b) = (
                guides.get(usize::from(current))?,
                guides.get(usize::from(to))?,
            );
            let (first, second, point, z) = shared_edge(a, b);
            corners[level + 1] = (first, second);
            along_z[level + 1] = z;
            points[level + 1] = point;
            points[level + 2] = target;
            let from_edge = |a: i16, b: i16| (i32::from(a) - i32::from(b)) >> 3;
            let (x, z) = (
                from_edge(points[level][0], point[0]),
                from_edge(points[level][1], point[1]),
            );
            let (tx, tz) = (
                from_edge(target[0], point[0]),
                from_edge(target[1], point[1]),
            );
            let total = crate::sca::square_root0(z * z + x * x)
                + crate::sca::square_root0(tz * tz + tx * tx)
                + i32::from(cost[level]);
            if total < best {
                best = total;
                route = Some((points[1], corners[1], points[2], along_z[1]));
                // The original's search carries on from the route's first area.
                current = nodes[1];
                if level == 0 {
                    current = to;
                }
            }
        }
    }
    let (mut edge, (mut first, mut second), next, along_z) = route?;
    let aim = |own: i16, other: i16, own_body: i32, other_body: i32| -> i16 {
        let delta = i32::from(own) - own_body;
        let square = delta != 0 && {
            let slope = (i32::from(other) - other_body).wrapping_mul(0x1000) / delta;
            slope.abs() <= 10000
        };
        if square {
            own
        } else if delta < 0 {
            own + 800
        } else {
            own - 800
        }
    };
    let (shift_x, shift_z) = if along_z {
        edge[0] = aim(edge[0], edge[1], body[0], body[1]);
        (0, 800)
    } else {
        edge[1] = aim(edge[1], edge[0], body[1], body[0]);
        (800, 0)
    };
    let way = [i32::from(next[0]) - body[0], i32::from(next[1]) - body[1]];
    first = [
        first[0].wrapping_add(shift_x),
        first[1].wrapping_add(shift_z),
    ];
    second = [
        second[0].wrapping_sub(shift_x),
        second[1].wrapping_sub(shift_z),
    ];
    let side = |corner: [i16; 2]| {
        (i32::from(corner[1]) - body[1])
            .wrapping_mul(way[0])
            .wrapping_sub((i32::from(corner[0]) - body[0]).wrapping_mul(way[1]))
    };
    Some(if side(first) ^ side(second) < 0 {
        next
    } else {
        edge
    })
}

/// `Sca_ck_line`, without its answers beyond a hit: whether a box of `mask`
/// stands between two points, in plan and from both sides.
pub fn sca_ck_line(sca: &[ScaRect], from: [i32; 3], to: [i32; 3], mask: u16) -> bool {
    /// `BYTE_0053ad38`: the shapes it doesn't test.
    const SKIPPED: [u8; 16] = [0, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0];
    sca.iter().any(|record| {
        record.id & mask != 0
            && SKIPPED[usize::from(record.id & 0xF)] == 0
            && line_hits(record, from, to)
    })
}

fn line_hits(record: &ScaRect, from: [i32; 3], to: [i32; 3]) -> bool {
    let (x, z) = (i32::from(record.x), i32::from(record.z));
    let (width, depth) = (i32::from(record.width), i32::from(record.depth));
    let within = |value: i32, base: i32, size: i32| (value.wrapping_sub(base) as u32) < size as u32;
    let mut sides = 3u32;
    let plan = if (within(from[0], x, width) && within(from[2], z, depth))
        || (within(to[0], x, width) && within(to[2], z, depth))
    {
        sides = 2;
        true
    } else {
        crosses(from[0], from[2], to[0], to[2], x, z, width, depth)
    };
    if !plan {
        return false;
    }
    let mut floor = 0i32;
    if record.floor != 0 {
        floor = -0x708 * record.floor.trailing_zeros() as i32;
    }
    let step = i32::from(record.kind >> 6 & 0x1F) * -0x12 - i32::from(record.kind >> 11);
    let top = floor.wrapping_add(step.wrapping_mul(-100));
    let lift = step.wrapping_mul(-100);
    let below = |y: i32| (y.wrapping_add(lift) as u32) < top as u32;
    if (below(from[1]) && within(from[2], z, depth)) || (below(to[1]) && within(to[2], z, depth)) {
        sides ^= 2;
    } else if !crosses_height(from[1], from[2], to[1], to[2], step * 100, floor, z, depth) {
        return false;
    }
    if (below(from[1]) && within(from[0], x, width)) || (below(to[1]) && within(to[0], x, width)) {
        return sides != 0;
    }
    crosses_height(from[1], from[0], to[1], to[0], step * 100, floor, x, width)
}

/// Whether the segment leaves a box's four sides on both tests the original
/// makes, which is the same sum in plan and against a height.
#[allow(clippy::too_many_arguments)]
fn crosses(ax: i32, az: i32, bx: i32, bz: i32, x: i32, z: i32, width: i32, depth: i32) -> bool {
    let (w, d) = (width, depth);
    let across = az.wrapping_sub(bz);
    let along = ax.wrapping_sub(bx);
    let a = (x - ax + w).wrapping_mul(across);
    let b = (z - az).wrapping_mul(along);
    let c = (d - az + z).wrapping_mul(along);
    let e = (x - ax).wrapping_mul(across);
    let g = (ax - x).wrapping_mul(d);
    let h = (bx - x).wrapping_mul(d);
    let first = c.wrapping_sub(a) ^ b.wrapping_sub(e);
    let second =
        (bz - z).wrapping_mul(w).wrapping_sub(h) ^ (az - z).wrapping_mul(w).wrapping_sub(g);
    let third = c.wrapping_sub(e) ^ b.wrapping_sub(a);
    let fourth = (bz - d - z).wrapping_mul(w).wrapping_add(h).wrapping_neg()
        ^ (az - d - z).wrapping_mul(w).wrapping_add(g).wrapping_neg();
    !(first & second >= 0 && third & fourth >= 0)
}

#[allow(clippy::too_many_arguments)]
fn crosses_height(
    ay: i32,
    aw: i32,
    by: i32,
    bw: i32,
    high: i32,
    low: i32,
    base: i32,
    size: i32,
) -> bool {
    let across = aw.wrapping_sub(bw);
    let along = ay.wrapping_sub(by);
    let span = low.wrapping_sub(high);
    let a = (low - ay).wrapping_mul(across);
    let b = (high - ay).wrapping_mul(across);
    let c = (size - aw + base).wrapping_mul(along);
    let e = (base - aw).wrapping_mul(along);
    let g = (aw - base).wrapping_mul(span);
    let h = (bw - base).wrapping_mul(span);
    let first = b.wrapping_sub(e) ^ a.wrapping_sub(c);
    let second = size.wrapping_mul(by.wrapping_sub(high)).wrapping_sub(h)
        ^ size.wrapping_mul(ay.wrapping_sub(high)).wrapping_sub(g);
    let third = b.wrapping_sub(c) ^ a.wrapping_sub(e);
    let fourth = (by - low).wrapping_mul(size).wrapping_add(h).wrapping_neg()
        ^ (ay - low).wrapping_mul(size).wrapping_add(g).wrapping_neg();
    !(first & second >= 0 && third & fourth >= 0)
}

/// `Pl_life_down` with `hard_mode` and `censorship_off` at 0: the life left,
/// the near-death bit, and 1 for the bite that leaves the player on their last
/// legs or 2 for the one that kills.
fn life_down(life: i16, down: i32, low: bool, near_death: bool) -> (i16, bool, u8) {
    let mut down = down;
    if life > 0x78 && down < 0x29 {
        down += down >> 1;
    }
    let left = life.wrapping_sub(down as i16);
    if left >= 0 {
        return (left, near_death, 0);
    }
    if low || (left >= -0xE && !near_death) {
        // Setting the bit writes `+0x154` as a whole word, over the life.
        return (0, true, 1);
    }
    (left, near_death, 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bite_takes_half_again_from_a_healthy_player() {
        assert_eq!(life_down(200, 20, false, false), (170, false, 0));
        assert_eq!(life_down(120, 20, false, false), (100, false, 0));
    }

    #[test]
    fn the_first_lethal_bite_leaves_the_player_at_nothing() {
        assert_eq!(life_down(10, 20, false, false), (0, true, 1));
    }

    #[test]
    fn a_bite_kills_once_the_player_is_near_death_or_it_goes_deep() {
        assert_eq!(life_down(0, 20, false, true), (-20, true, 2));
        assert_eq!(life_down(10, 30, false, false), (-20, false, 2));
    }

    #[test]
    fn a_crawlers_bite_never_kills() {
        assert_eq!(life_down(0, 30, true, true), (0, true, 1));
    }

    #[test]
    fn a_shot_takes_life_and_starts_the_damage_or_the_dying_state() {
        let mut character = Character {
            id: 0x10,
            be_flg: 1,
            work: Work::default(),
            body: crate::player::spawn(
                crate::state::RoomId::parse("1140").unwrap(),
                &Default::default(),
            ),
            kind: 0,
            start: (0, 0),
            with_room: false,
            zombie: None,
        };
        let mut zombie = Zombie {
            life: 20,
            d_life: [5, 9, 9],
            damage_cnt: 0x80,
            ..Zombie::default()
        };
        let hit = crate::weapon2::Hit {
            slot: 0,
            weapon: 2,
            band: 1,
            region: 2,
            damage: 15,
            stun: 3,
            count: 5,
            flags: 0,
        };
        let from = character.body.pos;
        take_hit(&mut character, &mut zombie, &hit, from);
        assert_eq!(zombie.life, 5);
        assert_eq!(character.work.routine, [2, 2, 0, 0]);
        assert_eq!(zombie.d_life, [2, 9, 9]);
        assert_eq!(zombie.damage_no, 5);
        assert_eq!(zombie.damage_cnt, 0x85);

        take_hit(&mut character, &mut zombie, &hit, from);
        assert_eq!(zombie.life, -10);
        assert_eq!(character.work.routine, [3, 2, 0, 0]);
        // A count below nothing stays at its floor.
        assert_eq!(zombie.d_life, [0xFF, 9, 9]);
    }

    #[test]
    fn a_burnt_body_chars_and_an_unburnt_one_doesnt() {
        let mut zombie = Zombie {
            burnt: true,
            ..Zombie::default()
        };
        assert_eq!(zombie.tint(), [0xA0; 3]);
        zombie.charred = CHARRED_MAX;
        assert_eq!(zombie.tint(), [0x3D; 3]);
        zombie.burnt = false;
        assert_eq!(zombie.tint(), [0xFF; 3]);
    }

    #[test]
    fn rnd_follows_the_originals_stream() {
        let mut rnd = Rnd::default();
        assert_eq!(rnd.draw(), 0x7D);
        assert_eq!(rnd.0, 0xD97D);
        // Room 100's load and first tick draw 87, recorded from the original.
        for _ in 1..87 {
            rnd.draw();
        }
        assert_eq!(rnd.0, 24679);
    }

    #[test]
    fn turning_stops_within_the_rate() {
        assert_eq!(turn_within(0x100, 0x180, 0x100), 0);
        assert_eq!(turn_within(0, 0x400, 0x100), 0x100);
        assert_eq!(turn_within(0x400, 0, 0x100), -0x100);
    }

    fn guide(x0: i16, z0: i16, x1: i16, z1: i16, links: u16) -> Guide {
        Guide {
            x0,
            z0,
            x1,
            z1,
            links,
        }
    }

    #[test]
    fn the_last_guide_holding_a_point_wins() {
        let guides = [guide(0, 0, 100, 100, 0), guide(50, 50, 100, 100, 0)];
        assert_eq!(guide_check(&guides, 10, 10), Some(0));
        assert_eq!(guide_check(&guides, 60, 60), Some(1));
        assert_eq!(guide_check(&guides, 200, 10), None);
    }

    #[test]
    fn a_route_heads_straight_on_when_the_edges_corners_allow() {
        let guides = [
            guide(0, 0, 2000, 2000, 0b010),
            guide(2000, 0, 4000, 2000, 0b101),
            guide(4000, 0, 6000, 2000, 0b010),
        ];
        let dest = kara_rootck2(&guides, 0, 2, [1000, 1000], [5000, 1000]).unwrap();
        assert_eq!(dest, [4000, 1000]);
    }

    #[test]
    fn a_route_heads_for_the_edge_when_its_corners_are_in_the_way() {
        let guides = [
            guide(0, 0, 2000, 2000, 0b010),
            guide(2000, 0, 4000, 6000, 0b101),
            guide(2000, 6000, 4000, 8000, 0b010),
        ];
        let dest = kara_rootck2(&guides, 0, 2, [1000, 200], [3000, 7000]).unwrap();
        assert_eq!(dest, [2000, 1000]);
    }

    #[test]
    fn a_body_is_pushed_out_of_another() {
        let other = Oba {
            atd: Atd {
                at_w: 500,
                at_h: 0x5DC,
                at: [0, -0x5DC, 0],
                ..Atd::default()
            },
            ..Oba::default()
        };
        let mut atd = Atd {
            at_w: 500,
            at_h: 0x5DC,
            at: [600, -0x5DC, 0],
            ..Atd::default()
        };
        let mut pos = [600, 0, 0];
        assert!(oba_hit(&other, 0, &mut atd, &mut pos));
        assert_eq!(pos, [1006, 0, 0]);
    }

    #[test]
    fn a_body_with_its_flag_off_isnt_pushed() {
        let other = Oba::default();
        let mut atd = Atd::default();
        assert!(!oba_hit(&other, 2, &mut atd, &mut [0; 3]));
    }

    #[test]
    fn a_wall_blocks_the_line_between_two_points() {
        let wall = ScaRect {
            x: 1000,
            z: -500,
            width: 100,
            depth: 1000,
            id: 0x2000,
            // 18 steps of 100 tall.
            kind: 0x40,
            floor: 1,
        };
        assert!(sca_ck_line(
            &[wall],
            [0, -1000, 0],
            [2000, -1000, 0],
            0x2000
        ));
        assert!(!sca_ck_line(
            &[wall],
            [0, -1000, 2000],
            [2000, -1000, 2000],
            0x2000
        ));
    }
}
