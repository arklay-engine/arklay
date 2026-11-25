//! The player's model beside KEYCONFIG's list on RE 2's option screen
//! (`Config_main`'s states 2 and 9 to 0xF, and the draw `FUN_0044b180`).
//!
//! The page plays the player's own work (`pPl`) through copies of the
//! routines of play that differ from them in many small ways: the turns
//! aren't masked, the rates are the fine-health ones, the run and the back
//! step don't move the body, the lowering counts its own ticks, and the keys
//! are a pseudo key word (`0x689384`) whose low byte stands for `Key_trg`.
//! So they are their own functions here, not flags on play's `pl_br_*`.
//!
//! What the page keeps outside the work (the pseudo keys, the row's count
//! and steps, the row last shown, whether the model has turned and is
//! turning back) is [`Statics`], which outlives the page and the option
//! screen as the executable's statics do.

use super::*;

/// `Config_set_data`'s place, where the model stands beside the list, and
/// where `FUN_0044b3c0` puts it back every tick.
pub const PLACE: [i32; 3] = [-0x5FA, 0x7E4, 0xA8C];
/// `Config_set_data`'s turn, and the one the model faces the viewer at.
pub const FACING: u16 = 0xC00;
/// `FUN_0044d990`'s start: off to the left, facing +x.
const START_X: i32 = -0xF40;
/// Sherry's (`Id` 0xF) start.
const SMALL_START_X: i32 = -0xC0F;
/// The turn's step, `FUN_00449600`'s fourth argument.
const FACE_STEP: i32 = 0xD8;
/// The Colt's raise turns faster.
const COLT_FACE_STEP: i32 = 0x120;
/// The run's, the back step's and the turn's rates on the page: play's
/// fine-health ones, with no mask on the result.
const PAGE_RUN_TURN: i32 = 0x38;
const PAGE_BACK_TURN: i32 = 0x28;
const PAGE_ROTATE_TURN: i32 = 0x50;
/// `Joint_move`'s `0x10000`: the clip plays backward.
const REVERSED: u32 = 0x1_0000;
/// The page's `Joint_move` step, and `FUN_00447a20`'s.
const PAGE_STEP: u32 = 0x200;
const INIT_STEP: u32 = 0x100;
/// `pl_mv05`'s handgun stages' lowering, `0x10100`.
const LOWER_STEP: u32 = REVERSED | 0x100;
/// The row `0x525AB4` starts at: none.
const NO_ROW: i32 = 0x63;
/// The rows of the list (`FUN_0044b3c0`'s cases).
const ROW_FORWARD: i32 = 0;
const ROW_BACKWARD: i32 = 1;
const ROW_LEFT: i32 = 2;
const ROW_RIGHT: i32 = 3;
const ROW_AIM: i32 = 4;
const ROW_FIRE: i32 = 5;
const ROW_RUN: i32 = 6;
/// Sherry, who holds nothing: `Type` 0.
const SHERRY: u8 = 0xF;
const ADA: u8 = 0xE;

/// `FUN_0044b3c0`'s statics and the page's two flags. The executable keeps
/// them until it exits: they outlive the page and the option screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Statics {
    /// `0x6893D4`: state 10 has turned the model to face the viewer.
    pub turned: bool,
    /// `0x6893CC`: a move onto the back step's or the aim's row turns the
    /// model back to face the viewer first.
    pub returning: bool,
    /// `0x689384`: the pseudo keys.
    pub key: u16,
    /// `0x6893A0`: the step's count.
    pub count: i32,
    /// `0x6893D8` and `0x6893DC`: the step, and the one kept while the
    /// aim's row waits.
    pub step: i32,
    pub kept: i32,
    /// `0x525AB4`: the row last shown.
    pub row: i32,
    /// `0x99D010`: the last pseudo keys `FUN_00447df0` played.
    pub last_key: u16,
}

impl Default for Statics {
    fn default() -> Self {
        Self {
            turned: false,
            returning: false,
            key: 0,
            count: 0,
            step: 0,
            kept: 0,
            row: NO_ROW,
            last_key: 0,
        }
    }
}

impl Statics {
    /// The statics as two words, for a screen to keep past itself: the keys,
    /// the last keys, the two flags and the row; then the count, the step and
    /// the kept step.
    pub fn to_words(&self) -> [i64; 2] {
        let first = i64::from(self.key)
            | i64::from(self.last_key) << 16
            | i64::from(self.turned) << 32
            | i64::from(self.returning) << 33
            | i64::from(self.row as i16 as u16) << 40;
        let second = i64::from(self.count as u32)
            | i64::from(self.step as i16 as u16) << 32
            | i64::from(self.kept as i16 as u16) << 48;
        [first, second]
    }

    /// [`Statics::to_words`] read back.
    pub fn from_words([first, second]: [i64; 2]) -> Self {
        Self {
            key: first as u16,
            last_key: (first >> 16) as u16,
            turned: first >> 32 & 1 != 0,
            returning: first >> 33 & 1 != 0,
            row: i32::from((first >> 40) as u16 as i16),
            count: second as u32 as i32,
            step: i32::from((second >> 32) as u16 as i16),
            kept: i32::from((second >> 48) as u16 as i16),
        }
    }
}

/// The model's files: the character's PLD (the `Main` bank) and the held
/// weapon's PLW (`Sub0`), and an empty room for the routines' [`Tick`].
pub struct PageAssets {
    pub body: std::sync::Arc<crate::model::Emd>,
    pub weapon: crate::model::Emw,
    room: RoomState,
}

impl PageAssets {
    /// `Player_set` and `Pl_weapon_ch`'s files for character `id` holding
    /// weapon `weapon` (`Type & 0xFFF`), from disc `disc`'s folder
    /// (`fg_status` bit 31) or else the other's.
    pub fn load(pack: &crate::pack::Pack, id: u8, weapon: u8, disc: u8) -> anyhow::Result<Self> {
        let read = |path: &dyn Fn(u8) -> String| {
            [disc & 1, !disc & 1]
                .into_iter()
                .find_map(|disc| pack.read(&path(disc)).ok())
                .ok_or_else(|| anyhow::anyhow!("no {}", path(disc & 1)))
        };
        let pld = read(&|disc| format!("player/{id:02x}/model_{disc}.pld"))?;
        let holder = plw_holder(id, weapon);
        let plw = read(&|disc| format!("player/{holder:02x}/weapon/{weapon:02x}_{disc}.plw"))?;
        let pld = crate::emd2::parse_pld(&pld)?;
        let plw = crate::emd2::parse_plw(&plw)?;
        let (body, weapon) = crate::emd2::player_banks(&pld, &plw)
            .ok_or_else(|| anyhow::anyhow!("the weapon has no locomotion clips"))?;
        Ok(Self::new(body, weapon))
    }

    pub fn new(body: crate::model::Emd, weapon: crate::model::Emw) -> Self {
        Self {
            body: std::sync::Arc::new(body),
            weapon,
            room: RoomState::default(),
        }
    }
}

/// `DAT_0053A305`'s row for `Type & 0xFFF` `weapon`, the handgun's first
/// byte as the process's loads of it left it ([`HandgunFrame`]): 0x03 after
/// Leon's alternates 8 and 0xA, which plants the other foot and may tilt
/// down past frame 3, and 0x83 after anyone else loads it again.
pub fn page_frames(weapon: u8, handgun: HandgunFrame) -> [u8; 3] {
    let frames = Weapon {
        id: weapon,
        ..Weapon::default()
    }
    .frames();
    if weapon == 2 {
        [handgun.0, frames[1], frames[2]]
    } else {
        frames
    }
}

/// `Foot_set_pl`'s chain for the weapon's frames: the leg the byte of the
/// character's parity picks.
fn page_foot_chain(id: u8, frames: [u8; 3]) -> &'static [usize; 5] {
    if frames[usize::from(id & 1)] & 0x80 != 0 {
        &AIM_FOOT_CHAIN
    } else {
        &AIM_OTHER_FOOT_CHAIN
    }
}

/// The page's model: `pPl` as `FUN_0044b100` makes it on the title's way
/// in, and the page's statics.
#[derive(Debug, Clone)]
pub struct PageModel {
    pub work: Work,
    pub body: PlayerState,
    /// `Id`.
    pub id: u8,
    pub statics: Statics,
    /// The foot `Foot_set_pl` keeps planted, as the last draw left it (the
    /// first pass's `Workm`), by chain: the weapon's own, then the other.
    drawn_feet: Feet,
    /// Part 0's world `Workm.t` as the last pass computed it, which the
    /// next pass's lights are latched at (`FUN_0044d890`) before it
    /// computes its own.
    drawn_root: Option<[i32; 3]>,
    /// `DAT_0053A305`'s row for the weapon, as this visit's `Pl_weapon_ch`
    /// left it.
    frames: [u8; 3],
}

/// What the page's routines read beyond `pPl`: the feet the last draw left
/// and the weapon's row of `DAT_0053A305`.
#[derive(Debug, Clone, Copy)]
struct Page {
    feet: Feet,
    frames: [u8; 3],
}

/// The feet `Foot_set_pl` keeps planted, as the last draw left them: the
/// end of the weapon's chain, then of the other leg's.
type Feet = [Option<[i32; 3]>; 2];

impl PageModel {
    /// `FUN_0044b100` and `Config_set_data`: character `id` holding weapon
    /// `weapon` (`Type`), freshly set, standing at [`PLACE`] facing the
    /// viewer. The statics are the executable's, which no visit resets, and
    /// `handgun` is the handgun's first frame as this visit's `Pl_weapon_ch`
    /// left it ([`HandgunFrame::equip`]).
    pub fn new(id: u8, weapon: u16, statics: Statics, handgun: HandgunFrame) -> Self {
        let room = RoomState::default();
        let mut body = player::spawn(crate::state::RoomId::parse("1000").unwrap(), &room);
        body.pos = PLACE;
        body.angle = FACING;
        let work = Work {
            routine: [0; 4],
            move_no: 0,
            move_cnt: 0,
            hokan: 0,
            first_tick_assumed: false,
            kind: weapon,
            ..Work::default()
        };
        Self {
            work,
            body,
            id,
            statics,
            drawn_feet: [None; 2],
            drawn_root: None,
            frames: page_frames((weapon & 0xFFF) as u8, handgun),
        }
    }

    /// `FUN_0044b100` and `Config_set_data` again on a later OPTION visit:
    /// a fresh character and weapon in the place, but the routine and motion
    /// bytes of `pPl` as the last visit left them, which neither writes.
    pub fn again(&self, id: u8, weapon: u16, handgun: HandgunFrame) -> Self {
        let mut model = Self::new(id, weapon, self.statics, handgun);
        model.work.routine = self.work.routine;
        model.work.move_no = self.work.move_no;
        model.work.move_cnt = self.work.move_cnt;
        model.work.hokan = self.work.hokan;
        model
    }

    /// From play (`FUN_0044af00` in `FUN_0044b100`'s place): the page's
    /// model is `pPl` itself, so its routine and motion bytes, its
    /// `D_life_u` and its speed are the player's as play left them, which
    /// `Config_set_data` doesn't write either.
    pub fn from_play(
        id: u8,
        weapon: u16,
        statics: Statics,
        handgun: HandgunFrame,
        player: &Work,
    ) -> Self {
        let mut model = Self::new(id, weapon, statics, handgun);
        model.work.routine = player.routine;
        model.work.move_no = player.move_no;
        model.work.move_cnt = player.move_cnt;
        model.work.hokan = player.hokan;
        model.work.d_life_u = player.d_life_u;
        model.work.spd = player.spd;
        model
    }

    /// A pass computing part 0 at `root`: the root the pass's lights are
    /// latched at, the one the pass before it left (`root` itself for a
    /// visit's first pass, which no pass precedes).
    pub fn pass_root(&mut self, root: [i32; 3]) -> [i32; 3] {
        self.drawn_root.replace(root).unwrap_or(root)
    }

    /// Run `f` on the model's [`Mover`], then latch the feet the draw
    /// after it leaves.
    fn with<R>(
        &mut self,
        assets: &PageAssets,
        f: impl FnOnce(&mut Mover<'_, '_>, &mut Statics) -> R,
    ) -> R {
        let empty: (&[Clip], &[Keyframe]) = (&[], &[]);
        let tick = Tick {
            room: &assets.room,
            skeleton: &assets.body.skeleton,
            morphs: &assets.body.morphs,
            banks: Banks {
                pld: (&assets.body.clips, &assets.body.keyframes),
                weapon: (&assets.weapon.clips, &assets.weapon.keyframes),
                room: empty,
                held: empty,
            },
            input: Input::default(),
            options: Options::default(),
            id: self.id,
            life: 200,
            poisoned: false,
            stopped: false,
            weapon: Weapon {
                id: (self.work.kind & 0xFFF) as u8,
                ..Weapon::default()
            },
            necks: [None; NECK_WORKS],
            targets: &[],
            neck_targets: &[],
            enemies: 0,
            pushing: false,
            objects: &[],
            key_trg: 0,
        };
        let mut mover = Mover::new(&mut self.work, &mut self.body, &tick);
        let result = f(&mut mover, &mut self.statics);
        let chain = page_foot_chain(self.id, self.frames);
        let chains = [chain, foot_chain_other(chain)];
        self.drawn_feet = chains.map(|chain| mover.chain_position(chain));
        result
    }

    /// `FUN_0044d990(0)`, state 2's: the model starts again off to the left
    /// (`FUN_00447a20`), and takes its first step in.
    pub fn start(&mut self, assets: &PageAssets) {
        let id = self.id;
        self.with(assets, |mover, _| mover.page_walk_in(id, true));
    }

    /// State 9's draw: `0x6893D4` cleared and `FUN_0044d990(1)`, a step in.
    pub fn walk(&mut self, assets: &PageAssets) {
        let id = self.id;
        self.with(assets, |mover, statics| {
            statics.turned = false;
            mover.page_walk_in(id, false);
        });
    }

    /// State 9's `Routine_1 = 1`, every tick.
    pub fn walk_on(&mut self) {
        self.work.routine[1] = MOVE_FORWARD;
    }

    /// State 9's decide: the model at [`PLACE`] at once.
    pub fn place(&mut self) {
        self.body.pos = PLACE;
    }

    /// State 10's draw: turn to face the viewer, then idle.
    pub fn face(&mut self, assets: &PageAssets) {
        self.with(assets, |mover, statics| {
            if mover.player.angle == FACING {
                if !statics.turned {
                    mover.work.routine = [MODE_MOVE, MOVE_IDLE, 0, 0];
                    statics.turned = true;
                }
            } else {
                mover.page_turn(FACE_STEP);
            }
            mover.page_br_idle(0);
            mover.page_mv_idle();
        });
    }

    /// State 0xB's `0x6893CC = 1`, on a move onto row 1 or 4.
    pub fn turn_back(&mut self) {
        self.statics.returning = true;
    }

    /// `FUN_0044b3c0(row)`: the draw of states 0xB to 0xF.
    pub fn demonstrate(&mut self, assets: &PageAssets, row: i32) {
        let id = self.id;
        let page = Page {
            feet: self.drawn_feet,
            frames: self.frames,
        };
        self.with(assets, |mover, statics| {
            mover.page_demonstrate(id, row, statics, page);
        });
    }

    /// The parts' world matrices the draw poses the model by: `FUN_0044d890`'s
    /// `Workm`s, each part on its parent's from the place and the turn. Both
    /// passes draw these; only their views differ.
    pub fn joints(&self, assets: &PageAssets) -> Vec<crate::anim::Mat4x3> {
        let mut model = self.clone();
        model
            .with(assets, |mover, _| mover.posed_parts())
            .into_iter()
            .map(|part| {
                part.map_or_else(crate::anim::Mat4x3::default, |part| crate::anim::Mat4x3 {
                    r: part.m.map(|row| row.map(i32::from)),
                    t: part.t,
                })
            })
            .collect()
    }

    /// The reflection's shadow (`MulKage` then `FUN_00449c90`, through the
    /// second view): under part 0 (`joints[0]`) at the place's height,
    /// with its x negated, turned by `-Cdir_y`, at the sizes
    /// `FUN_00447a20` gives the character. `None` before the model is posed.
    pub fn shadow(&self, joints: &[crate::anim::Mat4x3]) -> Option<PageShadow> {
        let root = joints.first()?;
        Some(PageShadow {
            pos: [root.t[0].wrapping_neg(), self.body.pos[1], root.t[2]],
            angle: self.body.angle.wrapping_neg() & 0xFFF,
            half: shadow_half(self.id),
        })
    }

    /// The trace's `kcm`: the routines in hex, the place, the turn, the
    /// motion's four bytes in hex and `Type`, then the statics.
    pub fn trace(&self) -> String {
        let r = self.work.routine;
        let s = &self.statics;
        format!(
            "{:02x}{:02x}{:02x}{:02x},{},{},{},{},{:02x}{:02x}{:02x}00,{},{},{},{:x},{},{},{},{}",
            r[0],
            r[1],
            r[2],
            r[3],
            self.body.pos[0],
            self.body.pos[1],
            self.body.pos[2],
            self.body.angle,
            self.work.move_no,
            self.work.move_cnt,
            self.work.hokan,
            self.work.kind,
            u8::from(s.turned),
            u32::from(s.returning),
            s.key,
            s.count,
            s.step,
            s.kept,
            s.row,
        )
    }
}

/// Where the reflection's shadow goes ([`PageModel::shadow`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageShadow {
    pub pos: [i32; 3],
    pub angle: u16,
    /// The half sizes along x and z.
    pub half: (i32, i32),
}

/// `FUN_00447a20`'s `Kage_work9_set` sizes, by `Id`: Claire and the other
/// odd ids under 10 and Ada smaller, Sherry smaller still.
pub fn shadow_half(id: u8) -> (i32, i32) {
    match id {
        SHERRY => (0x172, 0x1CC),
        ADA => (0x1A4, 0x208),
        id if id < 10 && id & 1 != 0 => (0x1A4, 0x208),
        _ => (500, 600),
    }
}

/// The chain `Foot_set_pl`'s other leg takes.
fn foot_chain_other(chain: &[usize; 5]) -> &'static [usize; 5] {
    if chain == &AIM_FOOT_CHAIN {
        &AIM_OTHER_FOOT_CHAIN
    } else {
        &AIM_FOOT_CHAIN
    }
}

impl Mover<'_, '_> {
    /// `Joint_move` on `bank` with the page's step word: `late` in the low
    /// half, and `0x10000` backward.
    fn page_joint(&mut self, bank: Bank, step: u32) -> bool {
        self.work.bank = bank;
        self.joint_move_late_reversed((step & 0xFFFF) as u16, step & REVERSED != 0)
    }

    /// `Routine_0..3 = 1, routine, 0, 0`.
    fn page_routine(&mut self, routine: u8) {
        self.work.routine = [MODE_MOVE, routine, 0, 0];
    }

    /// The turn, a raw `s16` the original never masks.
    fn page_angle(&self) -> i32 {
        i32::from(self.player.angle as i16)
    }

    fn set_page_angle(&mut self, angle: i32) {
        self.player.angle = angle as i16 as u16;
    }

    /// `ang += rate` for a right turn (`2`) and `-= rate` for a left (`8`),
    /// unmasked.
    fn page_turn_keys(&mut self, key: u16, rate: i32) {
        let mut angle = self.page_angle();
        if key & KEY_RIGHT != 0 {
            angle += rate;
        }
        if key & KEY_LEFT != 0 {
            angle -= rate;
        }
        self.set_page_angle(angle);
    }

    /// `FUN_00449600(pl, _, _, step)` (0x449600): a step of `step` toward
    /// [`FACING`] (or 0x400 with the low word's bit 15, and the step
    /// negated), the nearer way, snapping onto it within two steps. Its
    /// second and third arguments are unused, and the turn is a raw `s16`.
    fn page_turn(&mut self, step: i32) {
        let (target, step) = if step & 0x8000 != 0 {
            (0x400, -step)
        } else {
            (i32::from(FACING), step)
        };
        let angle = self.page_angle();
        let d = (target - angle + step) & 0xFFF;
        let angle = if d < 2 * step {
            target
        } else if d <= 0x800 {
            angle + step
        } else {
            angle - step
        };
        self.set_page_angle(angle);
    }

    /// `FUN_0044d990(p)` (0x44D990): with `start`, the model back off to the
    /// left facing +x, reset (`FUN_00447a20`); then the walk's branch on a
    /// held forward (nothing) and its step.
    fn page_walk_in(&mut self, id: u8, start: bool) {
        if start {
            self.work.routine = [MODE_MOVE, MOVE_FORWARD, 0, 0];
            let x = if id == SHERRY { SMALL_START_X } else { START_X };
            self.player.pos = [x, PLACE[1], PLACE[2]];
            self.player.angle = 0;
            self.page_reset();
        }
        self.page_br_walk(KEY_FORWARD);
        self.page_mv_walk();
    }

    /// `FUN_00447a20` (0x447A20): the routines to the idle, the damage and
    /// the script's motion cleared, and the idle's clip snapped to.
    fn page_reset(&mut self) {
        self.work.routine = [MODE_MOVE, MOVE_IDLE, 0, 0];
        self.work.damage_cnt = 0;
        self.work.sce_flg = 0;
        self.work.aim_target = None;
        self.work.move_no = IDLE_CLIPS[0];
        self.work.move_cnt = 0;
        self.work.hokan = 0;
        self.page_joint(Bank::Weapon, INIT_STEP);
        self.work.neck.flags = 0;
        self.work.neck.joint = None;
    }

    /// `FUN_0044b3c0(row)` (0x44B3C0, read from its assembly): the model put
    /// back in its place, turned back first if it has to be, then the row's
    /// pseudo keys played through `FUN_00447df0`.
    fn page_demonstrate(&mut self, id: u8, row: i32, s: &mut Statics, page: Page) {
        let kind = self.work.kind;
        // The shotguns' kick and the magnums' walk the body on rows 5 and 7.
        if !(matches!(kind & 0xFFF, 6 | 8) && matches!(row, 5 | 7)) {
            self.player.pos = PLACE;
        }
        if s.returning {
            self.page_br_idle(0);
            self.page_mv_idle();
            if self.player.angle != FACING {
                self.page_turn(FACE_STEP);
                return;
            }
            s.returning = false;
        }
        match row {
            ROW_FORWARD => {
                s.key = KEY_FORWARD;
                s.row = ROW_FORWARD;
            }
            ROW_BACKWARD => {
                if s.row != ROW_BACKWARD {
                    s.row = ROW_BACKWARD;
                    s.step = 0;
                }
                match s.step {
                    0 => {
                        s.key = KEY_FORWARD;
                        if self.player.angle == FACING {
                            s.step = 1;
                        }
                        self.page_turn(FACE_STEP);
                    }
                    1 => s.key = KEY_BACKWARD,
                    _ => {}
                }
            }
            ROW_LEFT => {
                s.key = KEY_LEFT;
                s.row = ROW_LEFT;
            }
            ROW_RIGHT => {
                s.key = KEY_RIGHT;
                s.row = ROW_RIGHT;
            }
            ROW_AIM if id == SHERRY => {
                s.key = 0;
                s.row = ROW_AIM;
                self.page_turn(FACE_STEP);
            }
            ROW_AIM => {
                if s.row != ROW_AIM {
                    s.row = ROW_AIM;
                    s.step = 0;
                }
                match s.step {
                    // Level, up, then down, each for 61 ticks.
                    step @ 0..=2 => {
                        s.key = [KEY_AIM, KEY_AIM | KEY_TILT_UP, KEY_AIM | KEY_TILT_DOWN]
                            [step as usize];
                        s.count = 0x3C;
                        s.kept = step;
                        s.step = 3;
                    }
                    3 => {
                        s.count -= 1;
                        if s.count == 0 {
                            s.step = s.kept + 1;
                            if s.step >= 3 {
                                s.step = 0;
                            }
                        }
                    }
                    _ => {}
                }
            }
            ROW_FIRE => {
                s.key = KEY_ACTION;
                if s.row != ROW_FIRE {
                    s.row = ROW_FIRE;
                    s.step = 0;
                    s.count = 0x37;
                }
                if id != SHERRY {
                    self.page_fire_steps(id, kind, s);
                }
            }
            ROW_RUN => {
                s.key = KEY_RUN | KEY_FORWARD;
                s.row = ROW_RUN;
            }
            _ => s.key = 0,
        }
        self.page_move(s.key, s, page);
    }

    /// Row 5's steps: the action key held, let go, the aim raised, fired,
    /// and waits. Its `Type` tests are on the whole word.
    fn page_fire_steps(&mut self, id: u8, kind: u16, s: &mut Statics) {
        match s.step {
            0 => {
                s.count -= 1;
                s.key = if s.count >= 0x1E { KEY_ACTION } else { 0 };
                if s.count == 0 {
                    s.count = 0x14;
                    s.step = 1;
                }
            }
            1 => {
                s.count += 0x1D;
                s.step = 2;
                s.key = 0;
            }
            2 => {
                s.count -= 1;
                s.key = KEY_AIM;
                if s.count == 0 {
                    s.count = match kind {
                        0xE => 0x42,
                        0x12 => 0x78,
                        7 | 8 => 0x5E,
                        0x11 => 0x54,
                        _ => 0x4A,
                    };
                    s.step = 3;
                }
            }
            3 => {
                s.count -= 1;
                s.key = KEY_AIM | KEY_FIRE;
                if s.count == 0 {
                    s.count = 0x14;
                    s.step = 4;
                    if kind == 0xE {
                        s.count = 0x21;
                    }
                    if kind == 0x12 {
                        s.step = 6;
                    }
                    if matches!(kind, 0xC | 5 | 8) || id == ADA {
                        s.count = 0xC;
                    }
                }
            }
            4 | 5 => {
                s.count -= 1;
                s.key = 0;
                if s.count == 0 {
                    if s.step == 4 {
                        s.count = 0x14;
                        s.step = 5;
                    } else {
                        s.count = 0x37;
                        s.step = 0;
                    }
                }
            }
            6 => {
                s.count -= 1;
                s.key = KEY_AIM;
                if s.count == 0 {
                    s.count = 10;
                    s.step = 4;
                }
            }
            // 7 to 9 walk back to the place, but nothing sets them.
            _ => {}
        }
    }

    /// `FUN_00447df0(pl, key)` (0x447DF0): the routine's branch on `key`,
    /// with its low byte as `Key_trg`, then its move, by the routine the
    /// branch left.
    fn page_move(&mut self, key: u16, s: &mut Statics, page: Page) {
        let trg = key as u8 as i8;
        match self.work.routine[1] {
            MOVE_IDLE => self.page_br_idle(key),
            MOVE_FORWARD => self.page_br_walk(key),
            MOVE_RUN => self.page_br_run(key, trg),
            MOVE_BACKWARD => self.page_br_back(key, trg),
            MOVE_ROTATE => self.page_br_rotate(key, trg),
            MOVE_ITEM => self.page_br_crouch(key),
            _ => {}
        }
        s.last_key = key;
        match self.work.routine[1] {
            MOVE_IDLE => self.page_mv_idle(),
            MOVE_FORWARD => self.page_mv_walk(),
            MOVE_RUN => self.page_mv_run(),
            MOVE_BACKWARD => self.page_mv_back(),
            MOVE_ROTATE => self.page_mv_rotate(),
            MOVE_AIM => self.page_mv_weapon(key, page),
            MOVE_ITEM => self.page_mv_crouch(),
            _ => {}
        }
    }

    /// The end of a branch: the action key's low byte crouches, and the aim
    /// key raises a weapon.
    fn page_branch_end(&mut self, key: u16, trg: i8) {
        if trg < 0 {
            self.page_routine(MOVE_ITEM);
        }
        if key & KEY_AIM != 0 && self.work.kind & 0xFFF != 0 {
            self.page_routine(MOVE_AIM);
        }
    }

    /// `0x447E30`: the idle's branch. No life clips, stairs, climb, check or
    /// push.
    fn page_br_idle(&mut self, key: u16) {
        if key & KEY_TURN != 0 {
            self.page_routine(MOVE_ROTATE);
        }
        if key & KEY_BACKWARD != 0 {
            self.page_routine(MOVE_BACKWARD);
        }
        if key & KEY_FORWARD != 0 {
            self.page_routine(if key & KEY_RUN != 0 {
                MOVE_RUN
            } else {
                MOVE_FORWARD
            });
        }
        self.page_branch_end(key, key as u8 as i8);
    }

    /// `0x447E90`: the idle's move, on `Sub0`'s clip 2. A routine past its
    /// first two steps plays nothing; its first step only plays whatever
    /// motion is up.
    fn page_mv_idle(&mut self) {
        if self.work.routine[2] == 0 {
            self.work.move_no = IDLE_CLIPS[0];
            self.work.move_cnt = 0;
            self.work.hokan = FULL_BLEND;
            self.work.routine[2] = 1;
        }
        if self.work.routine[2] == 1 {
            self.page_joint(Bank::Weapon, PAGE_STEP);
        }
    }

    /// `0x447ED0`: the walk's branch. Forward let go stops it; nothing turns,
    /// runs or aims.
    fn page_br_walk(&mut self, key: u16) {
        if key & KEY_FORWARD == 0 {
            self.page_routine(MOVE_IDLE);
        }
    }

    /// `0x447EF0`: the walk on `Sub0`'s clip 0, which moves the body by its
    /// root's motion. No noise, footsteps or water.
    fn page_mv_walk(&mut self) {
        if self.work.routine[2] == 0 {
            self.start_motion(Bank::Weapon, WALK_CLIPS[0]);
        }
        self.work.bank = Bank::Weapon;
        self.spd_get();
        self.page_joint(Bank::Weapon, PAGE_STEP);
        self.add_speed_xz();
    }

    /// `0x447F60`: the run's branch.
    fn page_br_run(&mut self, key: u16, trg: i8) {
        if key & KEY_RUN == 0 || key & KEY_FORWARD == 0 {
            self.page_routine(MOVE_IDLE);
            if key & KEY_TURN != 0 {
                self.page_routine(MOVE_ROTATE);
            }
            if key & KEY_BACKWARD != 0 {
                self.page_routine(MOVE_BACKWARD);
            }
            if key & KEY_FORWARD != 0 {
                self.page_routine(MOVE_FORWARD);
            }
            return;
        }
        if self.work.spd[0] > RUN_TURN_MIN_SPEED {
            self.page_turn_keys(key, PAGE_RUN_TURN);
        }
        self.page_branch_end(key, trg);
    }

    /// `0x447FF0`: the run on `Sub0`'s clip 1, in place: no `Add_speedXZ`.
    /// Ada's smoke (`Id` 0xD's `Esp_call`) isn't ported.
    fn page_mv_run(&mut self) {
        if self.work.routine[2] == 0 {
            self.start_motion(Bank::Weapon, RUN_CLIPS[0]);
        }
        self.work.bank = Bank::Weapon;
        self.spd_get();
        self.page_joint(Bank::Weapon, PAGE_STEP);
        self.work.spd[0] = self.work.spd[0].wrapping_sub(RUN_SLOWDOWN);
    }

    /// `0x448100`: the back step's branch.
    fn page_br_back(&mut self, key: u16, trg: i8) {
        if key & KEY_BACKWARD == 0 {
            self.page_routine(MOVE_IDLE);
            if key & KEY_TURN != 0 {
                self.page_routine(MOVE_ROTATE);
            }
            if key & KEY_FORWARD != 0 {
                self.page_routine(MOVE_FORWARD);
            }
            return;
        }
        self.page_turn_keys(key, PAGE_BACK_TURN);
        self.page_branch_end(key, trg);
    }

    /// `0x448170`: the back step on `Main`'s clip 0, in place. Its look at
    /// an enemy (`Lim_dir_ck`) finds none on the page.
    fn page_mv_back(&mut self) {
        if self.work.routine[2] == 0 {
            self.start_motion(Bank::Pld, BACK_CLIPS[0]);
        }
        self.work.bank = Bank::Pld;
        self.spd_get();
        self.page_joint(Bank::Pld, PAGE_STEP);
    }

    /// `0x4482B0`: the turn's branch.
    fn page_br_rotate(&mut self, key: u16, trg: i8) {
        if key & KEY_TURN == 0 {
            self.page_routine(MOVE_IDLE);
        }
        if key & KEY_BACKWARD != 0 {
            self.page_routine(MOVE_BACKWARD);
        }
        if key & KEY_FORWARD != 0 {
            self.page_routine(if key & KEY_RUN != 0 {
                MOVE_RUN
            } else {
                MOVE_FORWARD
            });
        }
        self.page_turn_keys(key, PAGE_ROTATE_TURN);
        self.page_branch_end(key, trg);
    }

    /// `0x448330`: the turn on the spot, `Sub0`'s clip 0.
    fn page_mv_rotate(&mut self) {
        if self.work.routine[2] == 0 {
            self.start_motion(Bank::Weapon, ROTATE_CLIPS[0]);
            self.work.spd = [0; 3];
        }
        self.page_joint(Bank::Weapon, PAGE_STEP);
    }

    /// `0x448380`: the crouch's branch: the action key let go once it is
    /// down stands the model up again.
    fn page_br_crouch(&mut self, key: u16) {
        if self.work.routine[2] == 3 && key & KEY_ACTION == 0 {
            self.work.routine[2] = 4;
            self.work.routine[3] = 0;
        }
    }

    /// `0x4483A0`: the crouch on `Main`'s clip 6, down, held, and up again
    /// backward. Play's pick-up opens the item window where this waits.
    fn page_mv_crouch(&mut self) {
        let reverse = match self.work.routine[2] {
            0 => {
                self.start_motion(Bank::Pld, ITEM_CLIP);
                self.work.spd[..2].fill(0);
                false
            }
            1 => false,
            2 => {
                self.work.routine[2] = 3;
                return;
            }
            4 => {
                self.start_motion(Bank::Pld, ITEM_CLIP);
                self.work.routine[2] = 5;
                true
            }
            5 => true,
            6 => {
                self.page_routine(MOVE_IDLE);
                return;
            }
            _ => return,
        };
        let step = if reverse {
            REVERSED | PAGE_STEP
        } else {
            PAGE_STEP
        };
        if self.page_joint(Bank::Pld, step) {
            self.work.routine[2] += 1;
        }
    }

    /// `0x448B30`: the weapon's stage by `Type & 0xFFF` (table 0x525904),
    /// then `Foot_set_pl`.
    fn page_mv_weapon(&mut self, key: u16, page: Page) {
        let weapon = self.work.kind & 0xFFF;
        match weapon {
            0 => self.page_mv_crouch(),
            2 | 3 | 5..=0xD | 0x11 | 0x13 => self.page_handgun(key, page.frames),
            // The knife's, the custom handgun's, Ada's, the flamethrower's
            // and the gatling's tables aren't ported.
            _ => {}
        }
        let id = self.tick.id;
        let chain = if matches!(weapon, 6 | 8) && id != 0xB && self.work.routine[2] == 2 {
            Some(&AIM_OTHER_FOOT_CHAIN)
        } else if self.work.spd[0] == 0 && (self.work.routine[2] != 0 || self.work.hokan == 0) {
            Some(page_foot_chain(id, page.frames))
        } else {
            None
        };
        if let Some(chain) = chain {
            let planted = page.feet[usize::from(chain != page_foot_chain(id, page.frames))];
            if let (Some(planted), Some(foot)) = (planted, self.chain_position(chain)) {
                self.player.pos[0] += planted[0] - foot[0];
                self.player.pos[2] += planted[2] - foot[2];
            }
        }
    }

    /// `0x4486F0`: the handguns' and the rest's stages (table 0x525A10).
    fn page_handgun(&mut self, key: u16, frames: [u8; 3]) {
        match self.work.routine[2] {
            0 | 6 => self.page_raise(key, frames),
            1 => self.page_hold(key),
            2 => self.page_fire(key, frames),
            3 => self.page_lower(),
            // `0x448C80`, `0x448D50` and `0x448E20`: the hold never picks
            // them on the page.
            _ => {}
        }
    }

    /// `0x448710`: the raise, turning toward [`FACING`], which may tilt once
    /// it is far enough up.
    fn page_raise(&mut self, key: u16, frames: [u8; 3]) {
        let kind = self.work.kind & 0xFFF;
        match self.work.routine[3] {
            0 => {
                self.aim_clip(AIM_RAISE_CLIP);
                self.set_tilt(TILT_LEVEL);
                self.work.aim_target = None;
                self.work.neck.flags |= 0x12;
                self.work.aimed_again = false;
                self.work.spd[0] = 0;
            }
            1 => {}
            2 => {
                self.aim_stage(1);
                return;
            }
            _ => return,
        }
        self.page_turn(if kind == 0xD {
            COLT_FACE_STEP
        } else {
            FACE_STEP
        });
        if self.page_joint(Bank::Weapon, PAGE_STEP) {
            self.work.routine[3] += 1;
        }
        if matches!(kind, 0xE | 0x11) {
            return;
        }
        if self.work.move_cnt > frames[0] & 0x7F && key & KEY_TILT_DOWN != 0 {
            self.aim_stage(1);
            self.set_tilt(TILT_DOWN);
            return;
        }
        if self.work.move_cnt > frames[1] & 0x7F && key & KEY_TILT_UP != 0 {
            self.aim_stage(1);
            self.set_tilt(TILT_UP);
        }
    }

    /// `0x448870`: the hold, which always fires: no rounds, reload or empty
    /// click.
    fn page_hold(&mut self, key: u16) {
        if self.work.routine[3] == 0 {
            self.aim_clip(self.tilt_clip(AIM_HOLD_CLIPS));
        }
        if key & KEY_AIM == 0 {
            self.aim_stage(3);
            return;
        }
        if self.work.kind & 0xFFF != 0x11 {
            let tilt = if key & KEY_TILT_UP != 0 {
                TILT_UP
            } else if key & KEY_TILT_DOWN != 0 {
                TILT_DOWN
            } else {
                TILT_LEVEL
            };
            if self.work.status_flg & tilt == 0 {
                self.work.routine[3] = 0;
                self.set_tilt(tilt);
                return;
            }
        }
        if key & KEY_FIRE != 0 {
            self.aim_stage(2);
            return;
        }
        self.page_joint(Bank::Weapon, PAGE_STEP);
    }

    /// `0x4489A0`: the shot, with no flash, cartridge, sound or kick's move.
    /// Its `hit_scan_ck` reads and finds nobody on the page.
    fn page_fire(&mut self, key: u16, frames: [u8; 3]) {
        if self.work.routine[3] == 0 {
            self.aim_clip(self.tilt_clip(AIM_FIRE_CLIPS));
            self.work.spd = [0; 3];
        }
        if key & KEY_AIM == 0 && self.work.move_cnt > frames[2] & 0x7F {
            self.aim_stage(1);
            self.work.spd = [0; 3];
            return;
        }
        if matches!(self.work.kind & 0xFFF, 6 | 8) && self.tick.id != 0xB {
            self.work.bank = Bank::Weapon;
            self.spd_get();
        }
        if self.page_joint(Bank::Weapon, PAGE_STEP) {
            self.aim_stage(1);
            self.work.spd = [0; 3];
        }
    }

    /// `0x448C10`: the lowering, the raise backward. Past its tenth tick
    /// `Spd_x` is 1, which stops the foot's planting.
    fn page_lower(&mut self) {
        if self.work.routine[3] == 0 {
            self.aim_clip(AIM_RAISE_CLIP);
            self.work.move_cnt = 1;
            self.work.hokan = AIM_LOWER_BLEND;
        }
        let ticks = self.work.routine[3];
        self.work.routine[3] = ticks.wrapping_add(1);
        if ticks > 10 {
            self.work.spd[0] = 1;
        }
        if self.page_joint(Bank::Weapon, LOWER_STEP) {
            self.work.spd[0] = 0;
            self.work.neck.flags &= 0xED;
            self.page_routine(MOVE_IDLE);
        }
    }
}

#[cfg(test)]
mod tests;
