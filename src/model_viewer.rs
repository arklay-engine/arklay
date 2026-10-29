//! The model viewer: any character, enemy or weapon model of the pack,
//! posed and animated in a room with no AI (`--model`).
//!
//! A model is named by its pack entry and loaded by [`load`], whatever game
//! it is from: an RE1 EMD, an RE2 enemy EMD (with the texture page and CLUT swatches beside it), an
//! RE2 player PLD, or an RE2 weapon PLW drawn in its character's hands. The
//! [`Viewer`] stands the model on the floor of the camera's view and plays
//! one clip of one animation bank on a loop, one frame per 30 Hz tick
//! through the same [`AnimPlayer`] clock and joint maths the player uses.

use std::sync::Arc;

use anyhow::{Context, Result, bail};

use crate::anim::{self, AnimPlayer, Mat4x3};
use crate::format::{self, Dialect};
use crate::model::{Clip, Emd, Keyframe};
use crate::pack::Pack;
use crate::state::Cut;
use crate::{emd, emd2};

/// The model a pack entry names; see the module docs for the kinds.
pub fn load(pack: &Pack, entry: &str) -> Result<Emd> {
    load_with(pack, entry, &pack.read(entry)?)
}

/// [`load`] with the entry's bytes already read; `pack` supplies the files
/// beside it (an enemy's texture, a weapon's character).
pub fn load_with(pack: &Pack, entry: &str, data: &[u8]) -> Result<Emd> {
    let lower = entry.to_ascii_lowercase();
    let model = match lower.rsplit_once('.').map(|(_, extension)| extension) {
        Some("emd") if format::emd(data).dialect() == Some(Dialect::Re2) => {
            let texture =
                enemy_texture(pack, entry).with_context(|| format!("the texture of {entry}"))?;
            emd2::parse_emd_with(data, texture)
        }
        Some("emd") => emd::parse(data),
        Some("pld") => emd2::parse_pld(data),
        Some("plw") => {
            let character = character_entry(entry)
                .with_context(|| format!("{entry} is not a player/<id>/weapon/<nn>_<s>.plw"))?;
            let player = emd2::parse_pld(&pack.read(&character)?)
                .with_context(|| format!("invalid player model {character}"))?;
            emd2::parse_plw(data).map(|weapon| emd2::with_weapon(&player, &weapon))
        }
        _ => bail!("{entry} is not a model (.emd, .pld or .plw)"),
    };
    model.with_context(|| format!("invalid model {entry}"))
}

/// An RE2 enemy EMD's texture: the `<stem>.bmp` page beside it with its
/// `<stem>_clut_<nn>.bmp` swatches ([`emd2::texture_from_bmps`]), or a raw
/// `<stem>.tim` a mod ships in its place.
fn enemy_texture(pack: &Pack, entry: &str) -> Result<crate::model::Texture8> {
    let stem = entry.rsplit_once('.').map_or(entry, |(stem, _)| stem);
    let page = format!("{stem}.bmp");
    let tim = format!("{stem}.tim");
    if !pack.contains(&page) && pack.contains(&tim) {
        return crate::tim::decode_8bpp(&pack.read(&tim)?);
    }
    let mut swatches = Vec::new();
    for row in 1.. {
        let swatch = emd2::clut_entry(stem, row);
        if !pack.contains(&swatch) {
            break;
        }
        swatches.push(pack.read(&swatch)?);
    }
    emd2::texture_from_bmps(&pack.read(&page)?, &swatches)
}

/// The character a weapon belongs to: `player/<id>/weapon/<nn>_<s>.plw` is
/// held by `player/<id>/model_<s>.pld`.
pub fn character_entry(weapon: &str) -> Option<String> {
    let (folder, file) = weapon.rsplit_once("/weapon/")?;
    let (_, disc) = file.strip_suffix(".plw")?.split_once('_')?;
    Some(format!("{folder}/model_{disc}.pld"))
}

/// The screen half-height, in pixels, the model's half-extent is fitted to:
/// most of the 240-line frame's upper half, so the whole model shows with a
/// margin.
const FIT_HALF_HEIGHT: f64 = 80.0;

/// The nearest the model stands to the camera, in world units, so a tiny
/// model is not pushed into the near plane.
const MIN_DISTANCE: f64 = 800.0;

/// The screen half-height of the 240-line frame, and the share of it the
/// model's feet and head must stay inside to be stood on the floor.
const HALF_HEIGHT: f64 = 120.0;
const FLOOR_MARGIN: f64 = 0.9;

/// A model standing in the room, playing one clip on a loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Viewer {
    pub model: Arc<Emd>,
    /// The animation bank playing: 0 is the model's own clips, `n` its
    /// [`Emd::banks`] entry `n - 1`.
    pub bank: usize,
    pub anim: AnimPlayer,
    pub pos: [i32; 3],
    pub angle: u16,
    /// The cut the model was last stood in front of.
    placed: Option<usize>,
    /// The first pose's extent about the entity origin, facing angle 0:
    /// the widest horizontal reach and the top and bottom (`y` grows down).
    reach: i32,
    top: i32,
    bottom: i32,
}

impl Viewer {
    /// `model` playing `clip` of `bank`. A bank or clip it does not have is
    /// an error naming what it does have.
    pub fn new(model: Emd, bank: usize, clip: usize) -> Result<Self> {
        let banks = 1 + model.banks.len();
        if bank >= banks {
            bail!("the model has {banks} animation bank(s); bank {bank} is out of range");
        }
        let mut viewer = Self {
            model: Arc::new(model),
            bank,
            anim: AnimPlayer::new(clip),
            pos: [0; 3],
            angle: 0,
            placed: None,
            reach: 0,
            top: 0,
            bottom: 0,
        };
        let clips = bank_of(&viewer.model, bank).1.len();
        if clip >= clips && clips > 0 {
            bail!("bank {bank} has {clips} clip(s); clip {clip} is out of range");
        }
        // Snap to the clip's first frame rather than easing in from nothing.
        viewer.anim.blend_counter = 0;
        viewer.tick();
        viewer.measure();
        Ok(viewer)
    }

    /// Record the current pose's extent (see [`Viewer::place`]).
    fn measure(&mut self) {
        let (pos, angle) = (self.pos, self.angle);
        (self.pos, self.angle) = ([0; 3], 0);
        let joints = self.joints().unwrap_or_default();
        (self.pos, self.angle) = (pos, angle);
        let (mut reach, mut top, mut bottom) = (0f64, 0f64, 0f64);
        for (object, joint) in self.model.mesh.objects.iter().zip(&joints) {
            for vertex in &object.vertices {
                // The vertex buffer holds `(x, -y, z)`, as the renderer reads it.
                let vertex = [vertex[0], -vertex[1], vertex[2]];
                let world: [f64; 3] = std::array::from_fn(|axis| {
                    let rotated: i64 = (0..3)
                        .map(|k| i64::from(joint.r[axis][k]) * i64::from(vertex[k]))
                        .sum();
                    rotated as f64 / 4096.0 + f64::from(joint.t[axis])
                });
                reach = reach.max(world[0].hypot(world[2]));
                top = top.min(world[1]);
                bottom = bottom.max(world[1]);
            }
        }
        (self.reach, self.top, self.bottom) = (reach.ceil() as i32, top as i32, bottom as i32);
    }

    /// Stand the model in front of `cut`, turned to face it.
    ///
    /// It stands along the view, as far out as fits its first pose's
    /// extent into the middle of the frame (`FIT_HALF_HEIGHT` pixels of
    /// the camera's focal length). There it stands on the floor (`y = 0`)
    /// when its feet and head are both in view, else it is lifted or
    /// lowered onto the line of sight: a crow or a cockroach seen from a
    /// camera high above the floor stays in frame.
    pub fn place(&mut self, cut: &Cut) {
        let from = cut.pos.map(f64::from);
        let view: [f64; 3] = std::array::from_fn(|axis| f64::from(cut.look_at[axis]) - from[axis]);
        let level = view[0].hypot(view[2]);
        let focal = f64::from(cut.fov.max(1));
        let (top, bottom) = (f64::from(self.top), f64::from(self.bottom));
        let half = ((bottom - top) / 2.0).max(f64::from(self.reach));
        let distance = (half * focal / FIT_HALF_HEIGHT).max(MIN_DISTANCE);
        // The horizontal direction of view; straight down falls back to +Z.
        let (dx, dz) = if level > 0.0 {
            (view[0] / level, view[2] / level)
        } else {
            (0.0, 1.0)
        };
        let x = from[0] + dx * distance;
        let z = from[2] + dz * distance;
        // The screen rows of world height `y` at that spot, measured as the
        // angle below the line of sight against the frame's half-height.
        let pitch = view[1].atan2(level);
        let limit = (HALF_HEIGHT * FLOOR_MARGIN / focal).atan();
        let in_view = |y: f64| ((y - from[1]).atan2(distance) - pitch).abs() <= limit;
        let y = if in_view(bottom) && in_view(top) {
            0.0
        } else {
            // The line of sight's height there, less the pose's middle.
            from[1] + pitch.tan() * distance - (top + bottom) / 2.0
        };
        self.pos = [x.round() as i32, y.round() as i32, z.round() as i32];
        // An entity at angle `a` faces `(cos a, -sin a)` on X/Z in 4096ths
        // of a turn (`entity_matrix`); it faces back along the view.
        let turn = dz.atan2(-dx) / std::f64::consts::TAU;
        self.angle = ((turn * 4096.0).round() as i32).rem_euclid(4096) as u16;
    }

    /// Stand the model in front of the room's current cut, once per cut:
    /// a camera switch brings it along, so it is always in view.
    pub fn follow(&mut self, cuts: &[Cut], current: usize) {
        if self.placed == Some(current) {
            return;
        }
        if let Some(cut) = cuts.get(current) {
            self.place(cut);
            self.placed = Some(current);
        }
    }

    /// One 30 Hz tick: advance the clip, looping at its end.
    pub fn tick(&mut self) {
        self.anim.update(bank_of(&self.model, self.bank).1);
    }

    /// The posed joint matrices to draw, or `None` when the bank has no
    /// keyframe to show.
    pub fn joints(&self) -> Option<Vec<Mat4x3>> {
        let (keyframes, clips) = bank_of(&self.model, self.bank);
        let keyframe = self.anim.pose_keyframe(clips, keyframes)?;
        let entity = anim::entity_matrix(self.pos, self.angle);
        Some(anim::joint_matrices(
            &self.model.skeleton,
            &keyframe,
            &entity,
        ))
    }
}

/// Bank `bank`'s keyframes and clips (see [`Viewer::bank`]).
fn bank_of(model: &Emd, bank: usize) -> (&[Keyframe], &[Clip]) {
    match bank.checked_sub(1) {
        None => (&model.keyframes, &model.clips),
        Some(index) => {
            let bank = &model.banks[index];
            (&bank.keyframes, &bank.clips)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_weapon_belongs_to_its_character_s_model_on_the_same_disc() {
        assert_eq!(
            character_entry("player/0b/weapon/0a_1.plw").as_deref(),
            Some("player/0b/model_1.pld")
        );
        assert_eq!(character_entry("player/0b/model_1.pld"), None);
    }

    #[test]
    fn an_re2_enemy_takes_its_texture_from_the_page_and_swatches_or_a_tim() {
        let tim = crate::emd2::tests::enemy_tim();
        let expected = crate::tim::decode_8bpp(&tim).unwrap();
        let bmps = emd2::texture_bmps(&tim).unwrap();
        let mut writer = crate::pack::PackWriter::new();
        writer
            .add("enemy/3a/enemy_1.emd", crate::emd2::tests::emd())
            .unwrap();
        writer.add("enemy/3a/enemy_1.bmp", bmps[0].clone()).unwrap();
        for (row, swatch) in bmps.iter().enumerate().skip(1) {
            writer
                .add(&emd2::clut_entry("enemy/3a/enemy_1", row), swatch.clone())
                .unwrap();
        }
        writer
            .add("enemy/3b/enemy_0.emd", crate::emd2::tests::emd())
            .unwrap();
        writer.add("enemy/3b/enemy_0.tim", tim).unwrap();
        let pack = Pack::from_bytes(writer.to_bytes().unwrap()).unwrap();
        assert_eq!(
            load(&pack, "enemy/3a/enemy_1.emd").unwrap().texture,
            expected
        );
        assert_eq!(
            load(&pack, "enemy/3b/enemy_0.emd").unwrap().texture,
            expected
        );
    }

    /// One joint, two keyframes and one two-frame clip, plus a further bank
    /// with no clips.
    fn model() -> Emd {
        use crate::model::{AnimBank, ClipFrame, Skeleton, Texture8, Tmd, TmdObject};
        let frame = |keyframe| ClipFrame {
            keyframe,
            timing: 0,
        };
        Emd {
            skeleton: Skeleton {
                relative: vec![[0; 3]],
                children: vec![Vec::new()],
            },
            keyframes: vec![
                Keyframe::default(),
                Keyframe {
                    rotations: vec![[0x400, 0, 0]],
                    ..Keyframe::default()
                },
            ],
            clips: vec![Clip {
                frames: vec![frame(0), frame(1)],
            }],
            damage_clips: Vec::new(),
            damage_keyframes: Vec::new(),
            banks: vec![AnimBank::default()],
            // One upright stick a thousand units tall.
            mesh: Tmd {
                objects: vec![TmdObject {
                    vertices: vec![[0, 0, 0], [0, 1000, 0]],
                    ..TmdObject::default()
                }],
            },
            texture: Texture8 {
                width: 0,
                height: 0,
                indices: Vec::new(),
                palettes: Vec::new(),
                stp: Vec::new(),
            },
        }
    }

    #[test]
    fn a_bank_or_clip_the_model_lacks_is_an_error() {
        assert!(Viewer::new(model(), 0, 0).is_ok());
        assert!(Viewer::new(model(), 0, 1).is_err());
        assert!(Viewer::new(model(), 2, 0).is_err());
        // A bank with nothing in it loads and draws nothing.
        let viewer = Viewer::new(model(), 1, 0).unwrap();
        assert!(viewer.joints().is_none());
    }

    #[test]
    fn the_model_stands_in_front_of_each_camera_facing_it() {
        let mut viewer = Viewer::new(model(), 0, 0).unwrap();
        // Fitting a 500-unit half-height into 80 of the frame's lines at a
        // 240 focal length stands it 1500 units out.
        let high = Cut {
            pos: [1000, -2000, 0],
            look_at: [1000, -2000, 10_000],
            fov: 240,
            ..Cut::default()
        };
        let low = Cut {
            pos: [0, -600, 0],
            look_at: [-1000, -600, 0],
            fov: 240,
            ..Cut::default()
        };
        let cuts = [high, low];
        viewer.follow(&cuts, 0);
        // The floor is out of the high camera's view there, so the stick is
        // lifted onto the line of sight; it faces -Z back at the camera:
        // `(cos a, -sin a) = (0, -1)`, a quarter turn.
        // (Within a unit: the trig tables saturate just short of 1.0.)
        let near = |pos: [i32; 3], expected: [i32; 3]| {
            pos.iter()
                .zip(expected)
                .all(|(got, want)| (got - want).abs() <= 1)
        };
        assert!(near(viewer.pos, [1000, -1500, 1500]), "{:?}", viewer.pos);
        assert_eq!(viewer.angle, 1024);
        // The same cut does not move it again; a switch brings it along,
        // and the low camera sees it standing on the floor.
        viewer.pos = [7; 3];
        viewer.follow(&cuts, 0);
        assert_eq!(viewer.pos, [7; 3]);
        viewer.follow(&cuts, 1);
        assert!(near(viewer.pos, [-1500, 0, 0]), "{:?}", viewer.pos);
        assert_eq!(viewer.angle, 0);
    }

    #[test]
    fn the_clip_loops_one_frame_per_tick() {
        let mut viewer = Viewer::new(model(), 0, 0).unwrap();
        let mut poses = Vec::new();
        for _ in 0..4 {
            poses.push(viewer.joints().unwrap());
            viewer.tick();
        }
        assert_ne!(poses[0], poses[1]);
        assert_eq!(poses[0], poses[2]);
        assert_eq!(poses[1], poses[3]);
    }
}
