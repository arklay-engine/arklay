//! RE2's zombies: `Em_Zombie`, which runs ids 0x10 to 0x1F in the enemy slots.
//!
//! Their behaviour (`Em10_Init`, `Em10_Move`, `Em10_Damage`, `Em10_Die`) is the
//! script `lib/zombie.lua`, behind a thin `enemy/{id:02x}/enemy.lua` for each id.
//! This is what the scripts stand on: the routing and sight of `Root_ck`, the
//! pushes and walls at the end of a tick, and the parts that come off the body.

use crate::player2::{self, Bank, Mover, Tick, Work, direction_ck, goto};
use crate::scd2::host::Character;
use crate::state::{Guide, ScaRect};

/// `Rnd`, the stream everything random in the game shares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rnd(pub u16);

/// The low word of what `Player_set` seeds it with: at a new game, and at a door
/// only when the player's model changes.
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
}

/// The colour `Em10_Scenario` gives the shadow under a body.
pub const POOL_COLOR: u32 = 0x00BF_BF10;
/// `Type`'s bits.
const TYPE_STRAIGHT: u16 = 0x1000;
pub const TYPE_HELD: u16 = 0x4000;
/// `Be_flg`'s bits: it's hit low, it's hit high.
pub const BE_LOW: u32 = 0x400_0000;
pub const BE_HIGH: u32 = 0x800_0000;
const BE_NO_SCA: u32 = 8;
/// `Sca_ck_em` pushed the body along z.
const BE_PUSHED_Z: u32 = 0x4000;
/// `Be_flg`'s bit on both bodies of a grab.
pub const BE_HOLDING: u32 = 0x1000;
/// `At_em_no` when nothing's touching, and the player's number.
pub const NO_BODY: u8 = 0xFF;
pub const PLAYER_BODY: u8 = 0xFE;
/// The word at `+0x21a`.
const FLAG_DOWN: u16 = 2;
const FLAG_THROWN: u16 = 8;
const FLAG_NECK_STILL: u16 = 0x200;
/// `Status_flg`'s bit: `Root_ck`'s line to the player is clear.
const STATUS_IN_SIGHT: u16 = 0x800;
/// `Now_seq`'s bit.
const SEQ_SOUND: u32 = 0x800_0000;

pub mod licker;
mod script;
pub mod surface;

/// A collision body, `AT_DATA`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Atd {
    pub ofs_x: i16,
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
    pub pos: [i32; 3],
}

/// `Oba_ck_hit`: the push out of `other`.
///
/// `old` is the body's place before this tick. A body that comes down onto
/// another, from clear of its height, goes through to the far side of it.
pub fn oba_hit(
    other: &Oba,
    be_flg: u32,
    atd: &mut Atd,
    pos: &mut [i32; 3],
    old: Option<[i32; 3]>,
) -> bool {
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
    if let Some(old) = old {
        let was = i32::from(atd.ofs_y) + old[1] - other.atd.at[1];
        if was <= -height || was >= height {
            let through = i32::from(other.atd.at_w) * 2;
            let crossed =
                |old: i32, now: i32, at: i32| (old < at && at < now) || (at < old && now < at);
            let beyond = |push: i32| {
                if push >= 0 {
                    push - through
                } else {
                    push + through
                }
            };
            if crossed(old[0], pos[0], other.pos[0]) {
                push_x = beyond(push_x);
            }
            if crossed(old[2], pos[2], other.pos[2]) {
                push_z = beyond(push_z);
            }
        }
    }
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

/// A part off the body. `Joint_trans2` gives it to `Parts_down` every tick: it
/// takes its children with it, turns to lie flat, and bleeds where it lands. It
/// isn't drawn after its second landing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThrownPart {
    pub part: u8,
    /// `Workm`, no longer following the skeleton.
    pub(crate) matrix: player2::Matrix,
    velocity: [i16; 3],
    gravity: i16,
    /// `Timer3`.
    bounces: u8,
    /// The rotation it turns to, and `Hokan_flg`: how far it has to go.
    rest: [[i16; 3]; 3],
    turning: i8,
    /// `Be_flg`'s `0x4A`: it goes to `Parts_bomb` instead.
    blown: Option<BlownPart>,
    /// The parts under it, which `ko_joint_trans2` turns off when it has gone.
    pub(crate) below: Vec<u8>,
}

/// What `Parts_bomb` reads: `FUN_00461f40`'s fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BlownPart {
    /// `+0x98`.
    dir: u16,
    /// `+0x9c`, which `+0xa4` takes down to nothing.
    speed: i16,
    /// `+0xa0`.
    count: u16,
}

/// `FUN_0045bd20`'s arm: the player has pulled it off the zombie that was on
/// them. It stays in their hands, shaking and bleeding, until it's thrown down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PulledArm {
    part: u8,
    /// `+0x23d`: which of the arms, and whether it was pulled from behind.
    behind: bool,
    /// `+0x23b`.
    left: u8,
    /// `+0x23c`'s 3: it goes next.
    done: bool,
    /// `Workm`, and the hand's `Workm` as the pull found it.
    world: player2::Matrix,
    hand: player2::Matrix,
    /// The two parts below the arm's `M`, once the clip has let them go;
    /// until then `+0x23c` is still 1.
    below: Option<(player2::Matrix, player2::Matrix)>,
    /// `M` under the player's first part, and where the hand is kept there.
    local: player2::Matrix,
    hold: [i32; 3],
}

/// What `Sce_parts_down` asks of `Parts_down`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptDrop {
    pub part: u8,
    pub angle: i16,
    pub along: i16,
    pub up: i16,
    pub gravity: u8,
    pub rest: [i16; 3],
}

impl ThrownPart {
    fn new(
        part: u8,
        matrix: player2::Matrix,
        facing: u16,
        (along, up): (i16, i16),
        gravity: i16,
        rest: [i16; 3],
    ) -> Self {
        let (x, z) = player2::rotate(facing.wrapping_add(0x800), i32::from(along), 0);
        Self {
            part,
            matrix,
            velocity: [x as i16, up, z as i16],
            gravity,
            bounces: 3,
            rest: player2::rot_matrix(rest),
            turning: 0xF,
            blown: None,
            below: Vec::new(),
        }
    }

    /// `FUN_00461f40`: on its way to `Parts_bomb`.
    fn blown(part: u8, matrix: player2::Matrix, dir: u16, speed: i16, up: i16) -> Self {
        Self {
            part,
            matrix,
            velocity: [0, up, 0],
            gravity: 10,
            bounces: 0,
            rest: player2::rot_matrix([0; 3]),
            turning: -1,
            blown: Some(BlownPart {
                dir,
                speed,
                count: 0,
            }),
            below: Vec::new(),
        }
    }

    /// Whether the parts under it go when it does. `Parts_bomb` clears all of
    /// its own `Be_flg`, so `ko_joint_trans2` doesn't see its `0x20`.
    pub fn takes_below(&self) -> bool {
        self.blown.is_none()
    }

    /// `Parts_bomb`, and whether it's still there. It draws no `Rnd`.
    fn blow(&mut self, mut blown: BlownPart) -> bool {
        let there = blown.count < 0x1D;
        if there {
            blown.count += 1;
        }
        self.velocity[1] = self.velocity[1].wrapping_add(self.gravity);
        self.matrix.t[1] += i32::from(self.velocity[1]);
        blown.speed = (blown.speed - 100).max(0);
        let (x, z) = player2::rotate(blown.dir, i32::from(blown.speed), 0);
        self.matrix.t[0] += x;
        self.matrix.t[2] += z;
        self.blown = Some(blown);
        there
    }

    /// `Parts_down`, and whether it's still there.
    pub fn fall(
        &mut self,
        rnd: &mut Rnd,
        effects: &mut crate::esp2::Effects,
        sca: Option<&crate::state::Sca>,
    ) -> bool {
        if let Some(blown) = self.blown {
            return self.blow(blown);
        }
        let before = self.matrix.t;
        let ground = sca.map_or(0, |sca| {
            crate::sca::ground_at(sca, before, 100, PARTS_MASK).height
        });
        let [x, y, z] = &mut self.matrix.t;
        *x += i32::from(self.velocity[0]);
        self.velocity[1] = self.velocity[1].wrapping_add(self.gravity);
        *y += i32::from(self.velocity[1]);
        *z += i32::from(self.velocity[2]);
        if ground < *y {
            self.bounces -= 1;
            *y = ground;
            self.velocity[0] >>= 2;
            self.velocity[2] >>= 2;
            self.velocity[1] = (-self.velocity[1]) >> 2;
            let dir = u16::from(rnd.draw()) << 4;
            let at = self.matrix.t.map(|axis| axis as i16);
            effects.call(0, 0, 2000, dir as i16, at, false);
            if self.bounces == 1 {
                return false;
            }
        }
        if let Some(sca) = sca {
            let floor = (-self.matrix.t[1] / 1800).clamp(0, 31) as u8;
            let resolved =
                crate::sca::resolve_on_floor(sca, PARTS_MASK, before, self.matrix.t, 100, floor);
            self.matrix.t = resolved.pos;
            if resolved.hit {
                self.velocity[0] = -(self.velocity[0] >> 2);
                self.velocity[2] = -(self.velocity[2] >> 2);
            }
        }
        if self.turning >= 0 {
            let weight = 0x1000 - i32::from(self.turning) * 0x100;
            self.matrix.m = player2::blend_matrix(&self.matrix.m, &self.rest, weight);
            self.turning -= 1;
        }
        true
    }
}

fn joint_matrix(matrix: &player2::Matrix) -> crate::anim::Mat4x3 {
    crate::anim::Mat4x3 {
        r: matrix.m.map(|row| row.map(i32::from)),
        t: matrix.t,
    }
}

/// What `Parts_down` looks for in the room's collision records.
const PARTS_MASK: u16 = 0x2000;

/// How many of the work's fields the script keeps for itself.
const VARS: usize = 34;
/// The dog, whose script is `enemy/20/enemy.lua`.
pub const DOG_ID: u8 = 0x20;
/// The crow, whose script is `enemy/21/enemy.lua`.
pub const CROW_ID: u8 = 0x21;
/// Mr. X, Tyrant 1, whose script is `enemy/2a/enemy.lua`.
pub const MRX_ID: u8 = 0x2A;
/// Where `Game_loop`'s `Joint_trans` left Mr. X's parts 12 and 15 between
/// ticks: x, y and z of each.
pub(crate) const MRX_HANDS: usize = 14;
/// `+0x230`, which `Em_20` counts up while the dog is stuck.
const DOG_STUCK: usize = 18;
/// `Ground`, which `Game_loop` finds under each body between ticks.
pub(crate) const DOG_GROUND: usize = 20;
/// Part 4's place, which `Game_loop`'s `Joint_trans2` left there between ticks.
pub(crate) const DOG_JAW: usize = 21;
/// The alligator, `Em_23`, whose script is `enemy/23/enemy.lua`.
pub const ALLIGATOR_ID: u8 = 0x23;
/// The part `Em_23`'s boxes follow (`pSuper`, `+0x4fc` of its parts).
const ALLIGATOR_HEAD: usize = 7;
/// `FUN_00468a10`'s `Atd[0]` to `Atd[2]`: `Ofs_x` and `At_w` (each `W` and
/// `At_d` the same, `At_h` 2000).
const ALLIGATOR_BOXES: [(i16, u16); 3] = [(3000, 600), (0x1C2, 0x5AA), (0x76C, 0x3B6)];
/// The giant spider, `Em_25`, whose script is `enemy/25/enemy.lua`.
pub const SPIDER_ID: u8 = 0x25;
/// G1, William Birkin's first form, `Em_30`, whose script is
/// `enemy/30/enemy.lua`.
pub const G1_ID: u8 = 0x30;
/// Where leg 1's foot, part 12, was as the last tick ended: the `+0x5C`
/// that `Game_loop`'s `Joint_trans` left there, x and z.
pub(crate) const SPIDER_FOOT: usize = 32;
/// `Ground`, which `Game_loop` finds under the spider between ticks.
pub(crate) const SPIDER_GROUND: usize = 30;
/// `Em_25`'s `Sca_ck_em` mask.
const SPIDER_MASK: u16 = 0x600;
/// The young spiders, `Em_26`, whose script is `enemy/26/enemy.lua`.
pub const YOUNG_ID: u8 = 0x26;
/// `Em_26`'s `Sca_ck_em` mask.
const YOUNG_MASK: u16 = 0x400;
/// `Water` (`+0x10c`), which `Set_enemy_work` copies with the work.
pub(crate) const YOUNG_WATER: usize = 5;
/// `Ground`, which `Game_loop`'s pass after the moves finds under it.
pub(crate) const YOUNG_GROUND: usize = 30;
/// The ivy, `Em_2E`, whose script is `enemy/2e/enemy.lua`.
pub const IVY_ID: u8 = 0x2E;
/// The vines, `Em_2F` (`enemy/2f/enemy.lua`): the ivy's, which it grows from a
/// room's template, and those a room places alone.
pub const VINE_ID: u8 = 0x2F;
/// The poison ivy, which runs `Em_2E`'s code.
pub const POISON_IVY_ID: u8 = 0x39;
/// `Em_2E`'s `Neck_no`, whose yaw its `Rot_neck_em` turns.
const IVY_NECK: usize = 9;
/// `+0x22d`: the ticks `Em_2E` has ended where it began.
pub(crate) const PLANT_STUCK: usize = 31;
/// `+0x21a`: `Em_2E` checks the others with `FUN_00488980`, not `Oba_ck_em`.
pub(crate) const PLANT_OBA: usize = 30;

/// `+0x231`: which side of its clips the ivy is on, 0 or 1.
pub(crate) const PLANT_SIDE: usize = 29;

/// `Em_25`'s and `Em_26`'s `nFloor`: the height over -1800, toward 0.
fn spider_floor(y: i32) -> u8 {
    let high = ((i64::from(y) * 0x6E5D_4C3B) >> 32) as i32 - y;
    ((high >> 10) - (high >> 31)) as u8
}

/// The ivy and the vines.
pub fn is_plant(id: u8) -> bool {
    matches!(id, IVY_ID | VINE_ID | POISON_IVY_ID)
}

/// `FUN_004887d0`'s turn for an ivy: `Cdir_y`, its neck's turn and its pose
/// there, less its side.
pub fn plant_aim(character: &Character) -> u16 {
    let Some(zombie) = character.zombie.as_ref() else {
        return 0;
    };
    let pose = zombie
        .rotations
        .get(IVY_NECK)
        .map_or(0, |rotation| rotation[1]);
    let side = (zombie.vars[PLANT_SIDE] as i16).wrapping_mul(0x800);
    (character.work.neck.turn[0])
        .wrapping_sub(side)
        .wrapping_add(pose)
        .wrapping_add(character.body.angle as i16) as u16
        & 0xFFF
}

/// The script's fields: an array too long to derive `Default`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Vars([i32; VARS]);

impl Default for Vars {
    fn default() -> Self {
        Self([0; VARS])
    }
}

impl std::ops::Deref for Vars {
    type Target = [i32; VARS];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Vars {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// What a zombie keeps beyond a character's [`Work`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Zombie {
    /// `Em_22`'s state: this isn't a zombie but a licker.
    pub licker: Option<Box<licker::Licker>>,
    /// `Atd[1]` and up, which the others are pushed out of after `Atd[0]`:
    /// the alligator's.
    pub more_atd: Vec<Atd>,
    /// Parts it has thrown that are still falling.
    pub thrown: Vec<ThrownPart>,
    /// `Sce_parts_down`s since it last ran.
    pub script_drops: Vec<ScriptDrop>,
    pulled: Option<PulledArm>,
    pub life: i16,
    /// The last shot was aimed up, which `Em10Dm02` and `FUN_00458d40` read
    /// off the player.
    shot_high: bool,
    /// The script's fields (`e:var`), which only it reads.
    vars: Vars,
    at_em_no: u8,
    /// `FUN_00459ef0` took hold of its body this tick, from this base.
    pub grabbed: Option<[i16; 3]>,
    /// `+0x21a`.
    pub flags: u16,
    /// `Timer0` to `Timer3`.
    timers: (u16, u16, u8, i8),
    pub damage_cnt: u8,
    /// `D_life_u`, `D_life_c` and `D_life_d`: how much more each region takes.
    d_life: [u8; 3],
    /// `Damage_no`: the shot's band times three and its region.
    damage_no: u8,
    /// `Damage_flg`: the shot's record and side.
    damage_flg: u16,
    /// `+0x228`: last tick's routine.
    last_routine: [u8; 4],
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
    /// What `pSca_hit_data` points at: the first record's place and size.
    sca_hit: Option<[i32; 4]>,
    /// `Cdir_x` and `Cdir_z`, which only a crow turns.
    pub tilt: [u16; 2],
    /// `pKage_work`'s `Half_x` and `Half_z` for a body that sizes its own
    /// shadow, 0 being `Be_flg`'s off.
    pub shadow_half: Option<i32>,
    /// `Floor` at `sce_em_set`.
    pub floor: u8,
    pub atd: Atd,
    /// `Model_type`.
    pub model_type: u8,
    /// `Em10_Init`'s skin, from 0: `Texture_Move`'s page.
    pub skin: u8,
    /// `Em_Zombie`'s moans and the like this tick, as `Snd_se_enem` plays them:
    /// each slot with the body's position at the call, since `Snd_se_3D` pans
    /// it there and then, before `Oba_ck_em` and `Sca_ck_em` push the body.
    pub sounds: Vec<(u8, [i32; 3])>,
    /// What a burnt zombie's `FUN_00456780` lit: the effect, the part it's on
    /// and where on that part.
    flames: Vec<(usize, u8, [i32; 3])>,
    burnt: bool,
    /// How far `Em10_Scenario` has taken a burnt body's colours down.
    charred: u8,
    /// `pEnemy_ptr`: the slot of the ivy a vine grew from.
    pub parent: Option<u8>,
    /// The parts' `Workm` that the ivy's `FUN_004886f0` left, for its vines.
    pub(crate) parts: Vec<Option<player2::Matrix>>,
    /// The parts' own rotations on that pose (`+0x68`).
    pub rotations: Vec<[i16; 3]>,
}

impl Zombie {
    /// Whether `Parts_down` has this part.
    pub fn flung(&self, part: u8) -> bool {
        self.thrown
            .iter()
            .any(|thrown| thrown.part == part && thrown.takes_below())
    }

    /// The parts no longer on the skeleton, and where they are.
    pub fn detached(&self) -> impl Iterator<Item = (u8, crate::anim::Mat4x3)> + '_ {
        let pulled = self
            .pulled
            .iter()
            .map(|arm| (arm.part, joint_matrix(&arm.world)));
        let thrown = self
            .thrown
            .iter()
            .map(|part| (part.part, joint_matrix(&part.matrix)));
        pulled.chain(thrown)
    }

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
    zombie.shot_high = hit.high;
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

impl Zombie {
    /// The script's fields, for another work that reads them.
    pub fn vars(&self) -> [i32; VARS] {
        self.vars.0
    }

    /// Set one of the script's fields from outside its run.
    pub fn set_var(&mut self, index: usize, value: i32) {
        if let Some(var) = self.vars.get_mut(index) {
            *var = value;
        }
    }

    /// `+0x150`, which `Set_enemy_work` starts at the room's enemy count.
    pub fn set_sight_turn(&mut self, turn: u8) {
        self.sight_turn = turn;
    }
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

/// What a script called to the others of its id.
#[derive(Debug, Clone, Copy)]
pub struct KinCall {
    /// The field to read and to add to, and the bits there that keep one out.
    pub flags: usize,
    pub skip: i32,
    pub bits: i32,
    /// Fields that go to 0.
    pub clear: [usize; 2],
}

impl Zombie {
    /// Hear what another of its id called.
    pub fn hear(&mut self, call: &KinCall) {
        if self.vars[call.flags] & call.skip == 0 {
            self.vars[call.flags] |= call.bits;
            for field in call.clear {
                self.vars[field] = 0;
            }
        }
    }
}

/// The ivy a vine grew from, as it stood after its own run this tick.
#[derive(Debug, Clone)]
pub struct ParentView {
    pub vars: [i32; VARS],
    pub pos: [i32; 3],
    pub angle: u16,
    pub life: i16,
    pub kind: u16,
    pub routine: [u8; 4],
    pub(crate) parts: Vec<Option<player2::Matrix>>,
    /// Each part's own rotation this tick (`+0x68`).
    pub rotations: Vec<[i16; 3]>,
    pub be_flg: u32,
}

/// A work `Set_enemy_work` gave a script: its slot, and what the script set
/// in it before its first run.
#[derive(Debug, Clone)]
pub struct Spawn {
    pub slot: usize,
    pub id: u8,
    pub kind: u16,
    pub vars: Vec<(usize, i32)>,
    /// A dead work of the id taken back, rather than a copy of the last.
    pub reused: bool,
    /// Where it goes and which way it faces, if the script said.
    pub place: Option<([i32; 3], u16)>,
    /// The id's routine runs on it at once, before the rest of the tick:
    /// `FUN_004732a0`'s call of `Em_26` on each young spider it sends.
    pub run_now: bool,
}

/// The rest of the game, as a zombie sees it.
pub struct World<'a> {
    /// `Ck_em_id`: how many others of this id are in the room.
    pub same_id: u8,
    pub room_enemy: &'a mut u8,
    /// Where a script faced the player.
    pub turn_player: Option<u16>,
    pub player: [i32; 3],
    pub player_neck: Option<[i32; 3]>,
    pub player_floor: u8,
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
    /// `DAT_00989ffe`: the player's weapon.
    pub weapon: u8,
    /// `IsScenarioClaire`.
    pub claire: bool,
    /// What this tick's script told the others of its id.
    pub kin: Vec<KinCall>,
    /// `DWORD_006894f8`: the `Work_no` of the licker whose tongue is out.
    pub tongue_owner: &'a mut u8,
    /// `fg_common`'s `0xa4`, which takes some life off a licker.
    pub common_a4: bool,
    /// `DWORD_009888d8`'s low byte: which draw buffer this frame fills.
    pub draw_buffer: u8,
    /// `(scd_var_stage + 1) * 0x100 + scd_var_room`.
    pub room_code: u16,
    /// A script moved `player` along the floor.
    pub player_moved: bool,
    /// Bits a script set in `fg_status`'s first word.
    pub status_or: u32,
    /// Where the body a script's `Sce_free3` names stands.
    pub scene_target: [i32; 3],
    /// The pack the behaviour scripts come from, and their VM. `None` parks them.
    pub pack: &'a crate::pack::Pack,
    pub lua: Option<&'a mut crate::enemy::LuaEnemyHost>,
    /// The ivy this vine grew from.
    pub parent: Option<ParentView>,
    /// The works `Set_enemy_work` walks, in `player_work`'s order of the enemy
    /// slots: each one's id and whether it's in play.
    pub works: Vec<(usize, Option<u8>, bool)>,
    /// What `Set_enemy_work` gave out this tick.
    pub spawns: Vec<Spawn>,
    /// The script took its own work out of play (`Kill_enemy_work`).
    pub killed: bool,
    /// The room's water areas, for `Water_ck`. Only a spider's world has them.
    pub water: Vec<WaterArea>,
    /// The flag banks as the tick began. Only the alligator's world has them.
    pub banks: Vec<[u32; 8]>,
    /// Flags a script set or cleared: bank, index and `set`'s operation.
    pub flag_ops: Vec<(u8, u8, u8)>,
    /// Bits a script cleared in `fg_status`'s first word.
    pub status_and_not: u32,
    /// `Cut_change`: the camera cut a script switched to.
    pub cut: Option<u8>,
    /// The room's objects' places (`obj_model_set`). Only the alligator's
    /// world has them.
    pub objects: Vec<[i32; 3]>,
    /// A script pointed the player's `pSub1_kan_t_ptr` at this body's
    /// model's clips (its second bank), for `Pl_damage`'s falls.
    pub sub1: bool,
    /// `scd_var_cut`, the camera's cut as the tick began.
    pub camera_cut: u8,
    /// Room objects a script took away.
    pub hidden_objects: Vec<usize>,
}

/// A water area (`aot_set` kind 7) as `Water_ck` reads it from `aot_table`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaterArea {
    /// The box: `x`, `z`, `width`, `depth`.
    pub zone: [i16; 4],
    /// A four-point area's corners, which replace the box.
    pub quad: Option<[[i16; 2]; 4]>,
    /// The water's level, the area's first word.
    pub level: i16,
}

/// `Water_ck`: the level of the first water area over `(x, z)`, or 0. A box
/// is `Hit_ck_box`'s unsigned test, a four-point area `Hit_ck_point4`'s.
pub fn water_ck(areas: &[WaterArea], x: i32, z: i32) -> i16 {
    for area in areas {
        let inside = match area.quad {
            Some(corners) => crate::state::Zone {
                corners,
                ..crate::state::Zone::default()
            }
            .contains_signed(x, z),
            None => {
                let [left, near, width, depth] = area.zone;
                (x.wrapping_sub(i32::from(left)) as u32) <= u32::from(width as u16)
                    && (z.wrapping_sub(i32::from(near)) as u32) <= u32::from(depth as u16)
            }
        };
        if inside {
            return area.level;
        }
    }
    0
}

impl World<'_> {
    /// `Set_enemy_work(id)`: a dead work of the id, else a new one past the last
    /// in play, copied from the last of the id. `None` with neither.
    pub fn set_enemy_work(&mut self, id: u8) -> Option<(usize, bool)> {
        let mut template = false;
        for entry in &mut self.works {
            if entry.1 == Some(id) {
                template = true;
                if !entry.2 {
                    entry.2 = true;
                    return Some((entry.0, true));
                }
            }
        }
        if !template {
            return None;
        }
        let entry = self.works.iter_mut().find(|entry| !entry.2)?;
        entry.1 = Some(id);
        entry.2 = true;
        Some((entry.0, false))
    }
}

/// One tick's zombie: the state the original keeps in the work and globals.
struct Em<'a, 'b, 'c> {
    mover: Mover<'a, 'b>,
    z: &'a mut Zombie,
    be_flg: &'a mut u32,
    world: &'a mut World<'c>,
    distance: i32,
    /// `Old_x`: where the body was when the tick began.
    old_pos: [i32; 3],
    /// `FUN_00458d40`'s two arms, as they stood before its clip.
    kept_arms: [Option<player2::Matrix>; 2],
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
    let old_pos = body.pos;
    let mut em = Em {
        mover: Mover::new(work, body, tick),
        z: zombie,
        be_flg,
        world,
        distance,
        kept_arms: [None; 2],
        old_pos,
    };
    if let Some(mut body) = em.z.licker.take() {
        licker::run(&mut em, &mut body);
        em.z.licker = Some(body);
        return;
    }
    em.run();
}

impl Em<'_, '_, '_> {
    fn work(&mut self) -> &mut Work {
        self.mover.work
    }

    /// `Em_20`, which leaves the rest to its script.
    fn run_dog(&mut self) {
        if self.world.stopped {
            return;
        }
        if self.z.damage_cnt & 0x7F != 0 {
            self.z.damage_cnt -= 1;
        }
        let before = self.mover.player.pos;
        self.old_pos = before;
        self.z.vars[DOG_GROUND] = self
            .mover
            .tick
            .room
            .sca
            .as_ref()
            .map_or(0, |sca| crate::sca::ground_at(sca, before, 0, MASK).height);
        if let Some([x, _, z]) = self.mover.part_point(4, [0, 0, 0]) {
            self.z.vars[DOG_JAW] = x;
            self.z.vars[DOG_JAW + 1] = z;
        }
        self.script();
        self.oba_ck_em();
        self.sca_ck_em();
        let body = self.mover.player.pos;
        if (body[0] == before[0] && body[2] == before[2]) || self.z.at_em_no != NO_BODY {
            self.z.vars[DOG_STUCK] = (self.z.vars[DOG_STUCK] + 1) & 0xFF;
        } else {
            self.z.vars[DOG_STUCK] = 0;
        }
    }

    /// `Em_21`, which leaves the rest to its script.
    fn run_crow(&mut self) {
        if self.world.stopped {
            return;
        }
        if self.z.damage_cnt & 0x7F != 0 {
            self.z.damage_cnt -= 1;
        }
        let before = self.mover.player.pos;
        self.old_pos = before;
        self.z.vars[DOG_GROUND] = self
            .mover
            .tick
            .room
            .sca
            .as_ref()
            .map_or(0, |sca| crate::sca::ground_at(sca, before, 0, MASK).height);
        self.script();
        self.oba_ck_em();
        self.sca_ck_em();
    }

    /// `Em_2A`, which leaves the rest to its script.
    fn run_mrx(&mut self) {
        self.root_ck();
        if self.world.stopped {
            return;
        }
        if self.z.damage_cnt & 0x7F != 0 {
            self.z.damage_cnt -= 1;
        }
        let before = self.mover.player.pos;
        self.old_pos = before;
        self.z.vars[DOG_GROUND] = self
            .mover
            .tick
            .room
            .sca
            .as_ref()
            .map_or(0, |sca| crate::sca::ground_at(sca, before, 0, MASK).height);
        for (hand, part) in [12, 15].into_iter().enumerate() {
            if let Some(point) = self.mover.part_point(part, [0, 0, 0]) {
                let at = MRX_HANDS + hand * 3;
                self.z.vars[at..at + 3].copy_from_slice(&point);
            }
        }
        self.script();
        self.oba_ck_em();
        self.sca_ck_em();
    }

    /// `Em_25`, which leaves the rest to its script, then finds `nFloor` from
    /// its height and checks the others and the room with `0x600`.
    fn run_spider(&mut self) {
        if self.world.stopped {
            return;
        }
        if self.z.damage_cnt & 0x7F != 0 {
            self.z.damage_cnt -= 1;
        }
        self.old_pos = self.mover.player.pos;
        if let Some([x, _, z]) = self.mover.part_point(12, [0, 0, 0]) {
            self.z.vars[SPIDER_FOOT] = x;
            self.z.vars[SPIDER_FOOT + 1] = z;
        }
        self.z.vars[SPIDER_GROUND] = self.mover.tick.room.sca.as_ref().map_or(0, |sca| {
            crate::sca::ground_at(sca, self.old_pos, 0, MASK).height
        });
        // The parts' `+0x5C` that `Game_loop`'s `Joint_trans` left, which a
        // hit's and a death's blood (`FUN_00472df0`) and the ceiling's spit
        // (`FUN_00472f70`) start from.
        self.z.parts = self.mover.posed_parts();
        self.script();
        self.z.floor = spider_floor(self.mover.player.pos[1]);
        self.oba_ck_em();
        self.sca_ck_masked(SPIDER_MASK);
    }

    /// `Em_26`, which leaves the rest to its script, then finds `nFloor` from
    /// its height and, unless the script took the work out of play
    /// (`Kill_enemy_work`), checks the others and the room with `0x400`.
    fn run_young(&mut self) {
        if self.world.stopped {
            return;
        }
        if self.z.damage_cnt & 0x7F != 0 {
            self.z.damage_cnt -= 1;
        }
        self.old_pos = self.mover.player.pos;
        self.z.vars[YOUNG_GROUND] = self.mover.tick.room.sca.as_ref().map_or(0, |sca| {
            crate::sca::ground_at(sca, self.old_pos, 0, MASK).height
        });
        // Part 0's `+0x5C` as `Game_loop`'s `Joint_trans` left it, which
        // `FUN_00474110`'s effects start from.
        self.z.parts = self.mover.posed_parts();
        self.script();
        self.z.floor = spider_floor(self.mover.player.pos[1]);
        if *self.be_flg != 0 {
            self.oba_ck_em();
            self.sca_ck_masked(YOUNG_MASK);
        }
    }

    /// `Em_23`, which leaves the rest to its script. It doesn't run
    /// `Sca_ck_em` or `Oba_ck_em`: its boxes push the others instead
    /// (`Oba_ck_hit` of each work against it), which isn't ported yet.
    fn run_alligator(&mut self) {
        if self.world.stopped {
            return;
        }
        if self.z.damage_cnt & 0x7F != 0 {
            self.z.damage_cnt -= 1;
        }
        self.old_pos = self.mover.player.pos;
        // The parts as the last draw left them, which its boxes, its neck and
        // its reach (`FUN_00469750`) start from: a script's `pos_set` since
        // then doesn't move them.
        if self.z.parts.is_empty() {
            self.z.parts = self.mover.posed_parts();
        }
        self.script();
        // What a hit that it shrugs off (`FUN_00469e20`) goes back to: the
        // original's `+0x1fc`, the routines as a hit finds them.
        self.z.last_routine = self.work().routine;
        self.alligator_atd();
        self.z.parts = self.mover.posed_parts();
    }

    /// `Em_23`'s `Oba_set_ofs`: its three boxes, each `Ofs_x` ahead along
    /// part 7 as the last draw left it (`Be_flg`'s `0x2000000`, with `Cdir`
    /// cleared). The init gives them their sizes.
    fn alligator_atd(&mut self) {
        let Some(Some(part)) = self.z.parts.get(ALLIGATOR_HEAD).copied() else {
            return;
        };
        let boxes = ALLIGATOR_BOXES.map(|(ofs_x, at_w)| {
            let ofs = [i32::from(ofs_x), 0, 0];
            let at = std::array::from_fn(|axis| {
                let turned: i32 = (0..3).map(|i| i32::from(part.m[axis][i]) * ofs[i]).sum();
                part.t[axis] + (turned >> 12)
            });
            Atd {
                ofs_x,
                ofs_y: 0,
                at_w,
                at_h: 2000,
                w: at_w,
                at,
            }
        });
        let [first, rest @ ..] = boxes;
        self.z.atd = first;
        self.z.more_atd = rest.to_vec();
    }

    /// `Em_30`, which leaves the rest to its script: `Root_ck` to the player
    /// on every tick, then, unless the room is stopped, `Damage_cnt`, the
    /// move, `Oba_ck_em` and `Sca_ck_em` (`0x400`).
    fn run_g1(&mut self) {
        self.root_ck();
        if self.world.stopped {
            return;
        }
        if self.z.damage_cnt & 0x7F != 0 {
            self.z.damage_cnt -= 1;
        }
        self.old_pos = self.mover.player.pos;
        self.script();
        self.oba_ck_em();
        self.sca_ck_em();
        // The parts as this tick's draw leaves them, which the step in's hit
        // (`Workm` of part 6) reads on the next.
        self.z.parts = self.mover.posed_parts();
    }

    /// `Em_2E` and `Em_2F`, which leave the rest to their scripts. The ivy's
    /// `FUN_004886f0` composes its parts for its vines afterwards, and it
    /// counts the ticks it ends where it began.
    fn run_plant(&mut self) {
        if self.world.stopped {
            return;
        }
        let before = self.mover.player.pos;
        self.old_pos = before;
        self.script();
        self.oba_ck(self.z.vars[PLANT_OBA] == 0);
        self.sca_ck_em();
        if self.mover.tick.id == VINE_ID {
            // A vine falling with its dead ivy reads its own part 2, turned
            // as it's drawn.
            let [pitch, roll] = self.z.tilt;
            let angle = self.mover.player.angle;
            self.z.parts = self.mover.posed_parts_turned(player2::rot_matrix([
                pitch as i16,
                angle as i16,
                roll as i16,
            ]));
            return;
        }
        self.z.parts = self.mover.posed_parts();
        self.z.rotations = self.mover.posed_rotations();
        self.work().neck.joint = Some(IVY_NECK as u8);
        self.work().neck.position = self.z.parts.get(IVY_NECK).copied().flatten().map(|m| m.t);
        let after = self.mover.player.pos;
        if after[0] == before[0] && after[2] == before[2] {
            self.z.vars[PLANT_STUCK] = (self.z.vars[PLANT_STUCK] + 1) & 0xFF;
        } else {
            self.z.vars[PLANT_STUCK] = 0;
        }
    }

    /// `Root_ck` with its own target: the point, or with `by_area` the middle of
    /// a guide area. 0 finds no way, 1 has its target in this area and 2 is a
    /// tick without a look. The player's sight is still checked.
    fn root_ck_to(&mut self, point: [i16; 2], area: Option<u8>) -> u8 {
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
        let kind = self.kind();
        if kind & TYPE_STRAIGHT != 0 {
            self.work().dest = point;
            return 2;
        }
        if kind & TYPE_HELD != 0 || self.z.sight_turn & 0x80 != 0 {
            return 2;
        }
        let body = self.mover.player.pos;
        let here = guide_check(self.world.guides, body[0] as i16, body[2] as i16);
        let middle = area
            .and_then(|area| self.world.guides.get(usize::from(area)))
            .map(|guide| {
                [
                    ((i32::from(guide.x1) + i32::from(guide.x0)) >> 1) as i16,
                    ((i32::from(guide.z1) + i32::from(guide.z0)) >> 1) as i16,
                ]
            });
        let (there, target) = match area {
            Some(area) => (Some(area), middle.unwrap_or(point)),
            None => (guide_check(self.world.guides, point[0], point[1]), point),
        };
        let (Some(here), Some(there)) = (here, there) else {
            if area.is_none() {
                return 0;
            }
            // Off the guides, heading for an area: `kara_rootck2` searches
            // from a row past the table, finds no route and writes what its
            // stack holds. The dog keeps its way here instead.
            return if self.z.sight_turn == 0 { 1 } else { 2 };
        };
        if here == there {
            self.work().dest = target;
            return 1;
        }
        if self.z.sight_turn == 0 {
            if let Some(dest) =
                kara_rootck2(self.world.guides, here, there, [body[0], body[2]], target)
            {
                self.work().dest = dest;
            }
            return 1;
        }
        2
    }

    fn run(&mut self) {
        if self.mover.tick.id == DOG_ID {
            self.run_dog();
            return;
        }
        if self.mover.tick.id == CROW_ID {
            self.run_crow();
            return;
        }
        if self.mover.tick.id == MRX_ID {
            self.run_mrx();
            return;
        }
        if self.mover.tick.id == G1_ID {
            self.run_g1();
            return;
        }
        if is_plant(self.mover.tick.id) {
            self.run_plant();
            return;
        }
        if self.mover.tick.id == SPIDER_ID {
            self.run_spider();
            return;
        }
        if self.mover.tick.id == YOUNG_ID {
            self.run_young();
            return;
        }
        if self.mover.tick.id == ALLIGATOR_ID {
            self.run_alligator();
            return;
        }
        // The scripts run first, and `Workm` stops following the skeleton from
        // what the last tick left there.
        for drop in std::mem::take(&mut self.z.script_drops) {
            if let Some(matrix) = self.mover.part_matrix(usize::from(drop.part)) {
                self.push_thrown(ThrownPart::new(
                    drop.part,
                    matrix,
                    (drop.angle as u16).wrapping_sub(0x800),
                    (drop.along, drop.up),
                    i16::from(drop.gravity as i8),
                    drop.rest,
                ));
            }
        }
        self.root_ck();
        if self.world.stopped {
            return;
        }
        if self.kind() & 1 != 0 || self.z.flags & FLAG_DOWN != 0 {
            *self.be_flg |= BE_LOW;
        }
        if self.z.damage_cnt & 0x7F != 0 {
            self.z.damage_cnt -= 1;
        }
        self.work().lean = 0;
        self.script();
        self.pull_arm();
        let held = self.kind() & TYPE_HELD != 0 || self.z.flags & FLAG_NECK_STILL != 0;
        let neck = &mut self.work().neck;
        neck.flags = (neck.flags & !2) | if held { 2 } else { 0 };
        self.mover.rot_neck_em(self.world.player_neck);
        self.z.last_routine = self.work().routine;
        let seq = self.mover.now_seq();
        if seq & SEQ_SOUND != 0 {
            self.z
                .sounds
                .push(((seq >> 28) as u8, self.mover.player.pos));
        }
        self.oba_ck_em();
        self.sca_ck_em();
        self.mover.place_neck();
        for (effect, part, point) in self.z.flames.clone() {
            if let Some(at) = self.mover.part_point(usize::from(part), point) {
                self.world
                    .effects
                    .move_to(effect, at.map(|axis| axis as i16));
            }
        }
    }

    /// `update` of the id's script. A pack without one leaves the zombie
    /// standing.
    fn script(&mut self) {
        let Some(lua) = self.world.lua.take() else {
            return;
        };
        let pack = self.world.pack;
        let id = self.mover.tick.id;
        let mut api = script::Api::zombie(self);
        lua.update_re2(pack, id, &mut api);
        self.world.lua = Some(lua);
    }

    fn kind(&mut self) -> u16 {
        self.work().kind
    }

    /// `Esp_call` at a point on a part.
    fn effect(&mut self, kind: u8, row: u8, size: u16, dir: i16, part: usize, point: [i32; 3]) {
        let at = self
            .mover
            .part_point(part, point)
            .unwrap_or(self.mover.player.pos);
        self.world
            .effects
            .call(kind, row, size, dir, at.map(|axis| axis as i16), false);
    }

    fn angle(&self) -> i16 {
        self.mover.player.angle as i16
    }

    /// `FUN_004e2ec0`: which of the sides of `pSca_hit_data`'s record the body
    /// is on. The original hands it a `SVECTOR` where it reads two `VECTOR`
    /// fields, so its x is the short `y` and `x` together and its z is whatever
    /// the stack holds past them: the caller's `return_to`.
    fn wall_side(&self, return_to: i32) -> i32 {
        let Some([x, z, width, depth]) = self.z.sca_hit else {
            return 0;
        };
        if width == 0 {
            return 0;
        }
        let [at_x, at_y, _] = self.mover.player.pos;
        let first = (((at_y as u32 & 0xFFFF) << 16) | (at_x as u32 & 0xFFFF)) as i32;
        let across = first
            .wrapping_sub(x)
            .wrapping_mul(depth)
            .wrapping_div(width);
        let along = return_to.wrapping_sub(z);
        let beyond = along.wrapping_sub(depth);
        if along <= across {
            if beyond.wrapping_neg() == across || across.wrapping_neg() < beyond {
                0
            } else {
                1
            }
        } else if across.wrapping_neg() < beyond {
            3
        } else {
            2
        }
    }

    /// `Be_flg |= 0x1062`: what `Parts_down` then does with the part, moving
    /// `along` the way the body faces away and `up`.
    fn throw_part(&mut self, part: usize, along: i16, up: i16, gravity: i16) {
        let matrix = self.mover.part_matrix(part);
        self.throw_matrix(part, matrix, along, up, gravity);
    }

    fn throw_matrix(
        &mut self,
        part: usize,
        matrix: Option<player2::Matrix>,
        along: i16,
        up: i16,
        gravity: i16,
    ) {
        let facing = self.mover.player.angle;
        if let Some(matrix) = matrix {
            self.push_thrown(ThrownPart::new(
                part as u8,
                matrix,
                facing,
                (along, up),
                gravity,
                [0, facing as i16, 0x400],
            ));
        }
    }

    fn push_thrown(&mut self, mut thrown: ThrownPart) {
        let children = &self.mover.tick.skeleton.children;
        let mut next = vec![usize::from(thrown.part)];
        while let Some(part) = next.pop() {
            for &child in children.get(part).into_iter().flatten() {
                thrown.below.push(child);
                next.push(usize::from(child));
            }
        }
        self.z.thrown.push(thrown);
    }

    /// `FUN_00461f40` sends it at 800 and level, `Em_2E`'s `FUN_00487800`
    /// at 300 and up at 150.
    /// `Em_25`'s `FUN_00472450` lets its body fall at `0xF`.
    fn blow_part(&mut self, part: usize, dir: u16, speed: i16, up: i16, gravity: i16) {
        if let Some(matrix) = self.mover.part_matrix(part) {
            let mut thrown = ThrownPart::blown(part as u8, matrix, dir, speed, up);
            thrown.gravity = gravity;
            self.push_thrown(thrown);
        }
    }

    /// `FUN_00461f90`: what `Parts_down` does with `+0x38` on its first tick.
    fn fling_part(&mut self, part: usize, dir: u16, along: i16, gravity: i16, rest: i16) {
        if let Some(matrix) = self.mover.part_matrix(part) {
            self.push_thrown(ThrownPart::new(
                part as u8,
                matrix,
                dir.wrapping_add(0x800),
                (along, 0),
                gravity,
                [0, rest, 0],
            ));
        }
    }

    /// `FUN_00456780`'s fire on a part: what the walk then carries with it.
    fn light_flame(&mut self, size: u16, part: u8, point: [i32; 3]) {
        let dir = self.mover.player.angle as i16;
        if let Some(effect) = self.world.effects.call(5, 3, size, dir, [0; 3], false) {
            self.z.flames.push((effect, part, point));
        }
    }

    /// `FUN_00453510`'s arm coming off in the shove.
    fn start_pull(&mut self, part: usize, behind: bool) {
        let world = self.mover.part_matrix(part);
        let hand = self.mover.part_matrix(part + 2);
        if let (Some(world), Some(hand)) = (world, hand) {
            self.z.pulled = Some(PulledArm {
                part: part as u8,
                behind,
                left: 0x3C,
                done: false,
                world,
                hand,
                below: None,
                local: world,
                hold: [0; 3],
            });
        }
    }

    /// `Oba_ck_em`.
    fn oba_ck_em(&mut self) {
        self.oba_ck(true);
    }

    /// `Oba_ck_em`, or with `player` false the ivy's `FUN_00488980`, which
    /// checks only the enemy slots.
    fn oba_ck(&mut self, player: bool) {
        let pos = self.mover.player.pos;
        let [ahead_x, ahead_z] = self.atd_ahead();
        self.z.atd.at = [
            pos[0] + ahead_x,
            pos[1] + i32::from(self.z.atd.ofs_y),
            pos[2] + ahead_z,
        ];
        self.z.at_em_no = NO_BODY;
        let old = Some(self.old_pos);
        for other in self.world.bodies {
            let body = &mut self.mover.player.pos;
            let mut other = *other;
            if other.slot == PLAYER_BODY {
                if !player {
                    continue;
                }
                other.be_flg = self.world.pl.be_flg;
            }
            if oba_hit(&other, *self.be_flg, &mut self.z.atd, body, old) {
                self.z.at_em_no = other.slot;
            }
        }
    }

    /// `Oba_set_ofs`: `Ofs_x` turned with the body. Only the dog has one.
    fn atd_ahead(&self) -> [i32; 2] {
        let along = i32::from(self.z.atd.ofs_x);
        let angle = i32::from(self.mover.player.angle);
        [
            (along * player2::rcos(angle)) >> 12,
            (-along * player2::rsin(angle)) >> 12,
        ]
    }

    /// `Sca_ck_em`, with the zombies' `0x400`. It checks `Atd`, not `Pos`.
    fn sca_ck_em(&mut self) {
        self.sca_ck_masked(MASK);
    }

    /// `Sca_ck_em` with the records of `mask`.
    fn sca_ck_masked(&mut self, mask: u16) {
        self.z.sca_info = 0;
        let radius = i32::from(self.z.atd.w);
        let [ahead_x, ahead_z] = self.atd_ahead();
        let room = self.mover.tick.room;
        let keeps_hit = matches!(self.mover.tick.id, CROW_ID | MRX_ID);
        let body = &mut *self.mover.player;
        if *self.be_flg & BE_NO_SCA == 0
            && let Some(sca) = &room.sca
        {
            *self.be_flg &= !BE_PUSHED_Z;
            let [old_x, old_z] = body.collision_mirror;
            let prev = [old_x, body.pos[1], old_z];
            let at = [body.pos[0] + ahead_x, body.pos[1], body.pos[2] + ahead_z];
            let floor = self.z.floor;
            if keeps_hit {
                self.z.sca_hit =
                    crate::sca::first_hit(sca, mask, at, radius, floor).map(|record| {
                        [
                            i32::from(record.x),
                            i32::from(record.z),
                            i32::from(record.width),
                            i32::from(record.depth),
                        ]
                    });
            }
            let resolved = crate::sca::resolve_with_offset(
                sca,
                mask,
                prev,
                at,
                radius,
                floor,
                [ahead_x, ahead_z],
            );
            // `Sca_ck_info`'s push moves what `Null_pos_set` adds to as well.
            let (dx, dz) = (resolved.pos[0] - at[0], resolved.pos[2] - at[2]);
            let base = &mut self.mover.work.base;
            base[0] = base[0].wrapping_add(dx as i16);
            base[2] = base[2].wrapping_add(dz as i16);
            // It moves the body through `Atd`, which others then push against.
            self.z.atd.at[0] += dx;
            self.z.atd.at[2] += dz;
            if dz != 0 {
                *self.be_flg |= BE_PUSHED_Z;
            }
            body.pos[0] += dx;
            body.pos[1] = resolved.pos[1];
            body.pos[2] += dz;
            self.z.sca_info = u32::from(resolved.hit);
        }
        body.collision_mirror = [body.pos[0] + ahead_x, body.pos[2] + ahead_z];
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

    fn player_within(&self, angle: u16, within: i32) -> bool {
        let body = &*self.mover.player;
        let [x, _, z] = self.world.player;
        let direction = direction_ck(body.pos[0], body.pos[2], x, z);
        (i32::from(direction) + within - i32::from(angle)) & 0xFFF < within * 2
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
            self.world.pl.damage_cnt |= 0x80;
        }
        result
    }

    /// `FUN_0045bd20`, the arm the shove's end took off.
    fn pull_arm(&mut self) {
        let Some(mut arm) = self.z.pulled.take() else {
            return;
        };
        let Some(root) = self.world.pl.root else {
            return;
        };
        if arm.done {
            let turn = if arm.behind { 0 } else { 0x800u16 };
            let rest = [0, self.world.player_angle.wrapping_add(turn) as i16, -0x400];
            let facing = self.mover.player.angle;
            let world = root.comp(&arm.local);
            self.push_thrown(ThrownPart::new(
                arm.part,
                world,
                facing,
                (0, -20),
                0x1E,
                rest,
            ));
            return;
        }
        let hand = match arm.below {
            // The first tick reads the last tick's `Workm`, from before the
            // clip stopped writing the hand.
            None => arm.hand.relative_to(&root),
            Some((forearm, hand)) => root
                .comp(&arm.local)
                .comp(&forearm)
                .comp(&hand)
                .relative_to(&root),
        };
        if arm.below.is_none() {
            arm.local = arm.world.relative_to(&root);
            let (x, z) = (
                i32::from((hand.t[0] >> 1) as i16),
                i32::from((hand.t[2] >> 1) as i16),
            );
            arm.hold = [x, i32::from((hand.t[1] as i16).wrapping_add(200)), z];
            self.z.sounds.push((9, self.mover.player.pos));
            // `Be_flg`'s 0x200 stops the clip writing the rest of the arm from
            // the next tick on, so it stays where this tick's `Joint_move` put it.
            let part = usize::from(arm.part);
            arm.below = self
                .mover
                .part_local(part + 1)
                .zip(self.mover.part_local(part + 2));
            if square_root0(x.wrapping_mul(x).wrapping_add(z.wrapping_mul(z))) > 800 {
                arm.done = true;
                self.z.pulled = Some(arm);
                return;
            }
        }
        let left = arm.left;
        arm.left = left.wrapping_sub(1);
        arm.done = left == 0;
        for axis in 0..3 {
            arm.local.t[axis] += arm.hold[axis] - hand.t[axis];
        }
        let shake = i16::from(self.rnd() & 0xF) * (1 - 2 * i16::from(arm.left & 1)) - 8;
        arm.local = arm.local.turned([0, 0, shake]);
        if arm.left & 3 == 0 {
            let dir = self.angle();
            let row = self.rnd() & 1;
            let size = u16::from(self.rnd()) + 0xC18;
            let at = arm.world.t.map(|axis| axis as i16);
            self.world.effects.call(0, row, size, dir, at, false);
            let size = u16::from(self.rnd() & 1) + 0xC18;
            let side = (i32::from(arm.part) / 3 - 3) * 500 - 0xFA;
            self.effect(0, 0, size, dir, 0, [0, -500, side]);
        }
        arm.world = root.comp(&arm.local);
        self.z.pulled = Some(arm);
    }

    /// `Em10_Damage` and `Em10_Die`'s start: a shot lets go of the player.
    fn let_go(&mut self) {
        if self.world.pl.routine[0] == player2::MODE_HELD
            && self.world.pl.held_by == Some(self.world.slot)
        {
            self.world.pl.damage_cnt &= 0x7F;
            self.world.pl.neck.flags &= 0xED;
            self.world.pl.routine = [1, 0, 0, 0];
            self.world.pl.bank = Bank::Weapon;
            self.world.pl.be_flg &= !0x1004;
            *self.be_flg &= !0x1004;
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

/// `BYTE_0053ad38`: the shapes `Sca_ck_line` doesn't test.
const LINE_SKIPPED: [u8; 16] = [0, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0];

/// `Sca_ck_line`, without its answers beyond a hit: whether a box of `mask`
/// stands between two points, in plan and from both sides.
pub fn sca_ck_line(sca: &[ScaRect], from: [i32; 3], to: [i32; 3], mask: u16) -> bool {
    sca.iter().any(|record| {
        record.id & mask != 0
            && LINE_SKIPPED[usize::from(record.id & 0xF)] == 0
            && line_hits(record, from, to)
    })
}

/// [`sca_ck_line`] with `param_4` zero: the line seen from above, at any height.
pub fn sca_ck_line_plan(sca: &[ScaRect], from: [i32; 3], to: [i32; 3], mask: u16) -> bool {
    sca.iter().any(|record| {
        record.id & mask != 0
            && LINE_SKIPPED[usize::from(record.id & 0xF)] == 0
            && line_sides(record, from, to).is_some()
    })
}

/// Whether the line crosses a record from above, and if so `DWORD_00689c88`:
/// 2 if an end is inside it, otherwise 3.
fn line_sides(record: &ScaRect, from: [i32; 3], to: [i32; 3]) -> Option<u32> {
    let (x, z) = (i32::from(record.x), i32::from(record.z));
    let (width, depth) = (i32::from(record.width), i32::from(record.depth));
    let within = |value: i32, base: i32, size: i32| (value.wrapping_sub(base) as u32) < size as u32;
    if (within(from[0], x, width) && within(from[2], z, depth))
        || (within(to[0], x, width) && within(to[2], z, depth))
    {
        Some(2)
    } else {
        crosses(from[0], from[2], to[0], to[2], x, z, width, depth).then_some(3)
    }
}

fn line_hits(record: &ScaRect, from: [i32; 3], to: [i32; 3]) -> bool {
    let (x, z) = (i32::from(record.x), i32::from(record.z));
    let (width, depth) = (i32::from(record.width), i32::from(record.depth));
    let within = |value: i32, base: i32, size: i32| (value.wrapping_sub(base) as u32) < size as u32;
    let Some(mut sides) = line_sides(record, from, to) else {
        return false;
    };
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
pub(crate) fn life_down(life: i16, down: i32, low: bool, near_death: bool) -> (i16, bool, u8) {
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

    fn thrown(y: i32, up: i16) -> ThrownPart {
        let matrix = player2::Matrix {
            m: player2::rot_matrix([0, 0, 0]),
            t: [1000, y, 2000],
        };
        ThrownPart::new(10, matrix, 0, (0, up), 0x32, [0, 0x400, 0])
    }

    #[test]
    fn a_thrown_part_goes_at_its_second_landing_and_each_draws_rnd() {
        let mut part = thrown(-100, 0);
        let mut effects = crate::esp2::Effects::default();
        let mut rnd = Rnd::default();
        let start = rnd;
        let mut landings = 0;
        let mut alive = true;
        for _ in 0..200 {
            alive = part.fall(&mut rnd, &mut effects, None);
            if !alive {
                break;
            }
            if part.matrix.t[1] == 0 && landings == 0 {
                landings = 1;
                assert!(part.velocity[1] < 0, "it bounces");
                let mut once = start;
                once.draw();
                assert_eq!(rnd, once);
            }
        }
        assert!(!alive);
        assert_eq!(landings, 1);
        let mut twice = start;
        twice.draw();
        twice.draw();
        assert_eq!(rnd, twice);
    }

    /// `Parts_bomb` runs out by count, wherever the ground is, and draws nothing.
    #[test]
    fn a_blown_part_slows_falls_and_goes_without_rnd() {
        let matrix = player2::Matrix {
            m: player2::rot_matrix([0, 0, 0]),
            t: [0, 1000, 0],
        };
        let mut part = ThrownPart::blown(3, matrix, 0, 800, 0);
        let mut effects = crate::esp2::Effects::default();
        let mut rnd = Rnd::default();
        let start = rnd;
        assert!(part.fall(&mut rnd, &mut effects, None));
        assert_eq!(part.matrix.t, [700, 1010, 0]);
        let mut ticks = 1;
        while part.fall(&mut rnd, &mut effects, None) {
            ticks += 1;
        }
        assert_eq!(ticks, 0x1D);
        assert_eq!(part.matrix.t[0], 700 + 600 + 500 + 400 + 300 + 200 + 100);
        assert_eq!(rnd, start);
        assert!(!part.takes_below());
    }

    /// `Em_25`'s `nFloor`: a floor for each 1800 units up, from where it
    /// stands.
    #[test]
    fn a_spiders_floor_is_its_height_over_1800() {
        for (y, floor) in [
            (0, 0),
            (100, 0),
            (-100, 0),
            (-1799, 0),
            (-1800, 1),
            (-3600, 2),
            (-7200, 4),
        ] {
            assert_eq!(spider_floor(y), floor, "{y}");
        }
    }

    /// `FUN_00487800`: the ivy's parts go slower than `FUN_00461f40`'s, and up.
    #[test]
    fn an_ivys_blown_part_starts_slow_and_rises() {
        let matrix = player2::Matrix {
            m: player2::rot_matrix([0, 0, 0]),
            t: [0, 1000, 0],
        };
        let mut part = ThrownPart::blown(3, matrix, 0, 300, -150);
        let mut effects = crate::esp2::Effects::default();
        let mut rnd = Rnd::default();
        assert!(part.fall(&mut rnd, &mut effects, None));
        assert_eq!(part.matrix.t, [200, 860, 0]);
        assert!(part.fall(&mut rnd, &mut effects, None));
        assert_eq!(part.matrix.t, [300, 730, 0]);
    }

    #[test]
    fn a_thrown_part_turns_to_its_rest_as_it_falls() {
        let mut part = thrown(-100_000, 0);
        let rest = part.rest;
        let mut effects = crate::esp2::Effects::default();
        let mut rnd = Rnd::default();
        for _ in 0..16 {
            assert!(part.fall(&mut rnd, &mut effects, None));
        }
        let flat = |m: [[i16; 3]; 3]| m.into_iter().flatten().map(i32::from);
        for (got, want) in flat(part.matrix.m).zip(flat(rest)) {
            assert!((got - want).abs() <= 2, "{got} {want}");
        }
    }

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
            model: 0x10,
            em_set: (0, 0, 0xFF),
            arms: Default::default(),
            kage: None,
            be_flg: 1,
            work: Work::default(),
            body: crate::player::spawn(
                crate::state::RoomId::parse("1140").unwrap(),
                &Default::default(),
            ),
            kind: 0,
            start: (0, 0),
            sound_type: 0,
            distance: 0,
            area: crate::scd2::host::NO_AREA,
            with_room: false,
            held_bank: None,
            zombie: None,
        };
        let mut zombie = Zombie {
            life: 20,
            d_life: [5, 9, 9],
            damage_cnt: 0x80,
            ..Zombie::default()
        };
        let hit = crate::weapon2::Hit {
            high: false,
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
        zombie.charred = 0x30;
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
            pos: [0; 3],
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
        assert!(oba_hit(&other, 0, &mut atd, &mut pos, None));
        assert_eq!(pos, [1006, 0, 0]);
    }

    #[test]
    fn a_body_with_its_flag_off_isnt_pushed() {
        let other = Oba::default();
        let mut atd = Atd::default();
        assert!(!oba_hit(&other, 2, &mut atd, &mut [0; 3], None));
    }

    /// The crow's sight passes no height: a wall counts however far below it is.
    #[test]
    fn the_plan_only_line_ignores_a_walls_height() {
        let wall = ScaRect {
            x: 1000,
            z: -500,
            width: 100,
            depth: 1000,
            id: 0x2000,
            kind: 0x40,
            floor: 1,
        };
        let (from, to) = ([0, -9000, 0], [2000, -9000, 0]);
        assert!(!sca_ck_line(&[wall], from, to, 0x2000));
        assert!(sca_ck_line_plan(&[wall], from, to, 0x2000));
        assert!(!sca_ck_line_plan(&[wall], from, to, 0x4000));
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
