//! The four players on Extreme Battle's character choice (`ExBattle`,
//! `0x4EA320`): Ada, Leon, Claire and Chris in works 0 to 3, each its own
//! `PL<id>CH.PLD` with its weapon's texture pasted in (`Ex_load_pld`,
//! `Ex_load_plw`), its motions (`ExPl_animate`) and a light of its own that
//! comes up on the one the cursor is on (`Ex_set_light`). The choice's
//! script turns the stage ([`ExModel::place`]); the engine plays the rest.

use super::*;
use crate::model::Emd;

/// The works, in the choice's order.
pub const WORKS: usize = 4;
/// Each work's player (`0x53B54A`, 0x14 bytes apart).
pub const IDS: [u8; WORKS] = [0xE, 0, 1, 0xB];
/// The weapon whose texture each work's model takes (`Ex_load_plw`'s
/// `pl0ew0d`, `pl00w02`, `pl01w09` and `pl0bw08`).
const WEAPONS: [u8; WORKS] = [0xD, 2, 9, 8];
/// Each work's place and turn (`0x53B538`): x, z and `Cdir_y`, y 0.
const PLACES: [(i32, i32, u16); WORKS] = [
    (0, -2250, 0),
    (2250, 0, 3584),
    (0, 2250, 3904),
    (-2250, 0, 2048),
];
/// `Ex_set_view`: the eye and the point it looks at, and `SetGeomScreen`.
pub const VIEW: ([i32; 3], [i32; 3]) = ([0x1446, -0x960, 0x30C], [0, -0x7D0, 0x82]);
pub const FOV: i32 = 300;
/// `Set_clear_color(0x40, 0x40, 0x40)` in `Ex_Set_light`.
pub const AMBIENT: u8 = 0x40;
/// `PartsWork_link`'s `Poly_rgb` for every part, `0x808080`.
pub const POLY_RGB: u8 = 0x80;
/// `FUN_004eafc0`: the light stands 0x4B0 along x and 1000 above the
/// work, reaches 8000, and has a gain of 1.
const LIGHT_OFFSET: [i32; 3] = [0x4B0, -1000, 0];
const LIGHT_RANGE: u16 = 8000;
const LIGHT_GAIN: i32 = 1;
/// `ExBattle_mv01` and `ExBattle_mv03`: the colour's step a tick, and the
/// level under which it still comes up.
const LIGHT_STEP: u8 = 0x14;
const LIGHT_TOP: u8 = 0xDD;
/// `Joint_move`'s step in `ExPl_animate`.
const STEP: u16 = 0x200;

/// Claire's parts past her skeleton (`FUN_004eaae0`): the ponytail, a
/// chain of four from her head (part 8), and part 0x13 on her right hand
/// (part 11). Each is `(parent, place on it)`. `Op_PartsSort05` swings the
/// chain; here it hangs unturned, as the gallery hangs it.
const CLAIRE_PARTS: [(usize, [i32; 3]); 5] = [
    (8, [-40, -368, 0]),
    (15, [-48, -32, 0]),
    (16, [-91, 97, 0]),
    (17, [-1, 127, 0]),
    (11, [0x40, 0x3C0, 0]),
];

/// A work's model: its PLD with the weapon's texels, and an empty room for
/// the routines' [`Tick`].
pub struct ExAssets {
    pub body: std::sync::Arc<Emd>,
    room: RoomState,
}

impl ExAssets {
    /// `Ex_load_pld` and `Ex_load_plw` for work `slot`, from disc `disc`'s
    /// folder (`fg_status` bit 31) or else the other's.
    pub fn load(pack: &crate::pack::Pack, slot: usize, disc: u8) -> anyhow::Result<Self> {
        let id = IDS[slot];
        let weapon = WEAPONS[slot];
        let read = |path: &dyn Fn(u8) -> String| {
            [disc & 1, !disc & 1]
                .into_iter()
                .find_map(|disc| pack.read(&path(disc)).ok())
                .ok_or_else(|| anyhow::anyhow!("no {}", path(disc & 1)))
        };
        let pld = read(&|disc| format!("player/{id:02x}/model_ch_{disc}.pld"))?;
        let plw = read(&|disc| format!("player/{id:02x}/weapon/{weapon:02x}_{disc}.plw"))?;
        let mut body = crate::emd2::parse_pld(&pld)?;
        let plw = crate::emd2::parse_plw(&plw)?;
        // `Marni_merge_wp_texture`: only the weapon's texels; the model's
        // own mesh holds the weapon.
        body.texture = crate::emd2::weapon_texture(&body.texture, &plw.texture);
        Ok(Self::new(body))
    }

    pub fn new(body: Emd) -> Self {
        Self {
            body: std::sync::Arc::new(body),
            room: RoomState::default(),
        }
    }
}

/// A work's light (`0x694220`, 0x10 bytes a work): `ExBattle_mv00` to
/// `mv03`'s state and the colour it has reached.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExLight {
    pub state: u8,
    pub level: u8,
}

/// One of the four works.
#[derive(Debug, Clone)]
pub struct ExModel {
    pub work: Work,
    pub body: PlayerState,
    /// `Routine_0`: the work's number.
    pub slot: usize,
    pub light: ExLight,
    /// Part 0's world `Workm.t` as the last sort left it, which the next
    /// `Ex_set_light` lights from: it runs before the sort.
    drawn_root: Option<[i32; 3]>,
}

impl ExModel {
    /// `FUN_004ea990` and `FUN_004eafc0`: work `slot` in its place, its
    /// routines and motion at 0, its light off.
    pub fn new(slot: usize) -> Self {
        let room = RoomState::default();
        let mut body = player::spawn(crate::state::RoomId::parse("1000").unwrap(), &room);
        let (x, z, cdir) = PLACES[slot];
        body.pos = [x, 0, z];
        body.angle = cdir;
        let work = Work {
            routine: [slot as u8, 0, 0, 0],
            move_no: 0,
            move_cnt: 0,
            hokan: 0,
            first_tick_assumed: false,
            bank: Bank::Pld,
            ..Work::default()
        };
        Self {
            work,
            body,
            slot,
            light: ExLight::default(),
            drawn_root: None,
        }
    }

    pub fn id(&self) -> u8 {
        IDS[self.slot]
    }

    /// `FUN_004eba10`'s place and turn for the work, as the choice's script
    /// runs them.
    pub fn place(&mut self, x: i32, z: i32, cdir: u16) {
        self.body.pos = [x, 0, z];
        self.body.angle = cdir;
    }

    /// `ExBattle_03`'s first tick, on the chosen work: Ada's second motion
    /// played to its end and held there, another's third.
    pub fn confirm(&mut self) {
        if self.slot == 0 {
            self.work.move_no = 1;
            self.work.routine[2] = 1;
        } else {
            self.work.move_no = 2;
            self.work.routine[2] = 3;
        }
        self.work.move_cnt = 0;
        self.work.hokan = 0;
    }

    /// Run `f` on the work's [`Mover`] over `assets`' banks.
    fn with<R>(&mut self, assets: &ExAssets, f: impl FnOnce(&mut Mover<'_, '_>) -> R) -> R {
        let empty: (&[Clip], &[Keyframe]) = (&[], &[]);
        let bank = (
            assets.body.clips.as_slice(),
            assets.body.keyframes.as_slice(),
        );
        let tick = Tick {
            room: &assets.room,
            skeleton: &assets.body.skeleton,
            morphs: &assets.body.morphs,
            banks: Banks {
                pld: bank,
                weapon: empty,
                room: empty,
                held: empty,
            },
            input: Input::default(),
            options: Options::default(),
            id: self.id(),
            life: 200,
            poisoned: false,
            stopped: false,
            weapon: Weapon::default(),
            necks: [None; NECK_WORKS],
            targets: &[],
            neck_targets: &[],
            enemies: 0,
            pushing: false,
            objects: &[],
            key_trg: 0,
        };
        let mut mover = Mover::new(&mut self.work, &mut self.body, &tick);
        f(&mut mover)
    }

    /// `ExPl_animate` on the work: Leon, Claire and Chris play their first
    /// motion from its start, then loop their second; a confirm's motion
    /// plays to its end (`Routine_2` 3) and stops there (4). Ada loops the
    /// motion she has until the confirm's (`Routine_2` 1) ends (2). Without
    /// a model the work's motion bytes stand still.
    pub fn animate(&mut self, assets: Option<&ExAssets>) {
        let ada = self.id() == 0xE;
        let r2 = self.work.routine[2];
        if !ada && r2 == 0 {
            self.work.move_no = 0;
            self.work.move_cnt = 0;
            self.work.hokan = 0;
            self.work.routine[2] = 1;
        }
        let Some(assets) = assets else {
            return;
        };
        self.work.bank = Bank::Pld;
        let r2 = self.work.routine[2];
        match (ada, r2) {
            (false, 1) => {
                if self.with(assets, |mover| mover.joint_move_late(STEP)) {
                    self.work.move_no = 1;
                    self.work.move_cnt = 0;
                    self.work.hokan = 0;
                    self.work.routine[2] = 2;
                }
            }
            (false, 2) | (true, 0) => {
                self.with(assets, |mover| mover.joint_move_late(STEP));
            }
            // The confirm's motion, to its end.
            (false, 3) | (true, 1) if self.with(assets, |mover| mover.joint_move_late(STEP)) => {
                self.work.routine[2] = if ada { 2 } else { 4 };
            }
            _ => {}
        }
    }

    /// `Ex_set_light`'s `ExBattle_mv00` to `mv03`: the light comes up by
    /// 0x14 a tick on the work the cursor is on (`cursor`) and goes down on
    /// the others.
    pub fn light_step(&mut self, cursor: usize) {
        let on = self.slot == cursor;
        let light = &mut self.light;
        match light.state {
            0 => {
                if on {
                    light.state = 1;
                }
            }
            1 => {
                if !on {
                    light.state = 3;
                } else if light.level < LIGHT_TOP {
                    light.level = light.level.wrapping_add(LIGHT_STEP);
                } else {
                    light.state = 2;
                }
            }
            2 => {
                if !on {
                    light.state = 3;
                }
            }
            _ => {
                if on {
                    light.state = 1;
                } else if light.level == 0 {
                    light.state = 0;
                } else {
                    light.level = light.level.wrapping_sub(LIGHT_STEP);
                }
            }
        }
    }

    /// The parts' world matrices `ExPl_sort` draws by, Claire's parts past
    /// her skeleton hung on theirs, and part 0's place kept for the next
    /// tick's light.
    pub fn sort(&mut self, assets: &ExAssets) -> Vec<crate::anim::Mat4x3> {
        let mut joints: Vec<crate::anim::Mat4x3> = self
            .with(assets, |mover| mover.posed_parts())
            .into_iter()
            .map(|part| {
                part.map_or_else(crate::anim::Mat4x3::default, |part| crate::anim::Mat4x3 {
                    r: part.m.map(|row| row.map(i32::from)),
                    t: part.t,
                })
            })
            .collect();
        if self.id() == 1 {
            for (parent, at) in CLAIRE_PARTS {
                let Some(parent) = joints.get(parent).copied() else {
                    break;
                };
                let local = crate::anim::Mat4x3 {
                    r: crate::anim::IDENTITY_MATRIX,
                    t: at,
                };
                joints.push(crate::anim::compose(&parent, &local));
            }
        }
        // Ada's part 15 hangs from part 0 with `Be_flg` 0 (`FUN_004eac60`):
        // it isn't drawn, and the parts past the skeleton have no matrix.
        joints.truncate(assets.body.mesh.objects.len());
        if let Some(root) = joints.first() {
            self.drawn_root = Some(root.t);
        }
        joints
    }

    /// The root the light is taken at: part 0 as the last sort left it, or
    /// the origin before the first.
    pub fn light_root(&self) -> [i32; 3] {
        self.drawn_root.unwrap_or([0; 3])
    }

    /// `Ex_Set_light` and `Ex_calc_light` at `root`: one point light at
    /// `LIGHT_OFFSET` from the work, its colour the light's level for each
    /// channel, by how far it is across x and z (each `>> 4`) of its range,
    /// and the direction from it to `root`.
    pub fn lighting(&self, root: [i32; 3]) -> crate::render::EntityLight {
        let pos = self.body.pos;
        let light = [0, 1, 2].map(|axis| i32::from((pos[axis] + LIGHT_OFFSET[axis]) as i16));
        let offset = [0, 1, 2].map(|axis| (root[axis] - light[axis]) >> 4);
        let range = i64::from(LIGHT_RANGE >> 4);
        let distance = integer_sqrt(
            i64::from(offset[2]) * i64::from(offset[2])
                + i64::from(offset[0]) * i64::from(offset[0]),
        );
        let reach = (range - distance).max(0);
        let level = (i64::from(self.light.level) * reach / range) as u8;
        let length = offset
            .iter()
            .map(|v| f64::from(*v).powi(2))
            .sum::<f64>()
            .sqrt()
            .max(1.0);
        let gain = f64::from(LIGHT_GAIN);
        let source = crate::render::EntityLightSource {
            dir: [
                (f64::from(offset[0]) / length * 4096.0 * gain) as i32,
                (-f64::from(offset[1]) / length * 4096.0 * gain) as i32,
                (f64::from(offset[2]) / length * 4096.0 * gain) as i32,
            ],
            color: [level; 3],
        };
        let dark = crate::render::EntityLightSource {
            dir: [0; 3],
            color: [0; 3],
        };
        crate::render::EntityLight {
            ambient: [f64::from(AMBIENT); 3],
            sources: [source, dark, dark],
        }
    }

    /// The trace's `exbm` for the work: `Routine_2`, then `Move_no`,
    /// `Move_cnt`, `Hokan_flg` and `Mplay_flg` in hex.
    pub fn trace(&self) -> String {
        format!(
            "{},{:02x}{:02x}{:02x}00",
            self.work.routine[2], self.work.move_no, self.work.move_cnt, self.work.hokan
        )
    }
}

/// `SquareRoot0`: the integer root, truncated.
fn integer_sqrt(value: i64) -> i64 {
    if value <= 0 {
        return 0;
    }
    let mut root = (value as f64).sqrt() as i64;
    while root * root > value {
        root -= 1;
    }
    while (root + 1) * (root + 1) <= value {
        root += 1;
    }
    root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_works_start_in_their_places_with_their_lights_off() {
        let ada = ExModel::new(0);
        assert_eq!((ada.body.pos, ada.body.angle), ([0, 0, -2250], 0));
        let chris = ExModel::new(3);
        assert_eq!(
            (chris.body.pos, chris.body.angle, chris.id()),
            ([-2250, 0, 0], 2048, 0xB)
        );
        assert_eq!(chris.light, ExLight::default());
        assert_eq!(chris.trace(), "0,00000000");
    }

    /// `ExBattle_mv00` to `mv03`: up by 0x14 to 0xF0 (the last step from
    /// 0xDC), held, then down by 0x14 to 0 once the cursor leaves.
    #[test]
    fn the_light_comes_up_on_the_cursor_and_goes_down_off_it() {
        let mut leon = ExModel::new(1);
        leon.light_step(1);
        assert_eq!(leon.light, ExLight { state: 1, level: 0 });
        for _ in 0..12 {
            leon.light_step(1);
        }
        assert_eq!(leon.light.level, 0xF0);
        leon.light_step(1);
        assert_eq!(
            leon.light,
            ExLight {
                state: 2,
                level: 0xF0
            }
        );
        leon.light_step(2);
        assert_eq!(leon.light.state, 3);
        for _ in 0..12 {
            leon.light_step(2);
        }
        assert_eq!(leon.light, ExLight { state: 3, level: 0 });
        leon.light_step(2);
        assert_eq!(leon.light.state, 0);
    }

    /// Without a model, `ExPl_animate` still starts the first motion of
    /// every work but Ada's.
    #[test]
    fn the_first_pass_starts_the_first_motion_but_adas() {
        let mut ada = ExModel::new(0);
        let mut claire = ExModel::new(2);
        ada.animate(None);
        claire.animate(None);
        assert_eq!(ada.work.routine[2], 0);
        assert_eq!(claire.work.routine[2], 1);
        claire.confirm();
        assert_eq!(claire.trace(), "3,02000000");
        ada.confirm();
        assert_eq!(ada.trace(), "1,01000000");
    }

    /// `Ex_Set_light`: a light of level 0xF0 with the root 0x4B0 below it
    /// along x lights at (500 - 75) / 500 of its level, from straight
    /// along x but for its height.
    #[test]
    fn the_light_fades_with_distance_across_the_floor() {
        let mut leon = ExModel::new(1);
        leon.light.level = 0xF0;
        let light = leon.lighting([2250, -1000, 0]);
        let source = light.sources[0];
        assert_eq!(source.color, [(0xF0 * 425 / 500) as u8; 3]);
        assert_eq!(source.dir[1], 0);
        assert!(source.dir[0] < -4000);
        assert_eq!(light.ambient, [64.0; 3]);
    }
}
