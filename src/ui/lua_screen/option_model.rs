//! `api:option_model`: the player's model beside RE 2's KEYCONFIG list
//! ([`crate::player2::keyconfig`]). The engine runs the model; the title's
//! script calls it at the points `Config_main` and its draw do.

use mlua::{IntoLuaMulti, UserDataMethods};

use crate::player2::keyconfig::{PageAssets, PageModel, Statics};
use crate::player2::{HandgunFrame, Work};
use crate::render::{Camera, CharacterShade, EntityLight, Framebuffer, Lighting, Slots};
use crate::state::Image;
use crate::state::Light;
use crate::ui::ScreenStatics;

use super::Api;

/// `FUN_0044db30`'s first view: `Set_view` from behind the model.
const MAIN_VIEW: ([i32; 3], [i32; 3]) = ([-100, 80, 700], [-100, 80, 1700]);
/// Its second, the reflection's, from the far side.
const MIRROR_VIEW: ([i32; 3], [i32; 3]) = ([-100, 80, 0x2134], [-100, 80, 0x1D4C]);
/// `SetGeomScreen(0xD0)`.
const PAGE_FOV: i32 = 0xD0;
/// `FUN_0044d890`'s colours: the model at `0x7F7F7F`, its reflection darker
/// at `0x525252`.
const MAIN_COLOUR: u8 = 0x7F;
const MIRROR_COLOUR: u8 = 0x52;
/// `Config_set_data`'s ambient, `0x4F4F4F`. Classic REbirth's `WinMain`
/// clears the lighting bypass (`0x680592`), so the page's model is lit.
const PAGE_AMBIENT: u8 = 0x4F;
/// `FUN_00449e90`'s table at `0x525A88`: the gain, then three lights as
/// `Set_Light_data` takes them (a point light's falloff is by x and z):
/// a far point light, a directional one and none.
const PAGE_GAIN: u8 = 1;
const PAGE_LIGHTS: [(u16, [i32; 3], [u8; 3], u16); 3] = [
    (0, [23800, -2000, -420], [0xE4, 0x58, 0x30], 26000),
    (1, [9880, -3600, -900], [0x30; 3], 100),
    (0, [0; 3], [0; 3], 0),
];

/// The page's lights latched at `pos` (`FUN_0044d890`, from part 0's
/// `Workm.t` as each pass leaves it) on the colour scale of
/// `MarniSortEmdGallery`'s light matrices: `0x449E00` stores each channel
/// times 16 in 4.12 and `FUN_004462e0` takes the raw channel back, so a
/// unit of a light's channel is a unit of the lit colour, as the ambient
/// is ([`CharacterShade::Lit`]).
fn page_light(pos: [i32; 3]) -> EntityLight {
    let lighting = Lighting {
        ambient: [0; 3],
        lights: PAGE_LIGHTS.map(|(kind, pos, color, radius)| Light {
            pos,
            color,
            kind,
            radius,
            re2_gain: Some(PAGE_GAIN),
        }),
    };
    let mut light = lighting.latched(pos);
    light.ambient = [f64::from(PAGE_AMBIENT); 3];
    light
}

/// `MarniSortEmdGallery`'s modulate for one of `FUN_0044d890`'s colours:
/// the colour doubled, of 255, over the lit colour.
fn modulate(colour: u8) -> u8 {
    colour.saturating_mul(2)
}

/// `FUN_00440950`'s patch of the room's picture, `x, y, w, h` in the
/// picture: drawn again over the mirror's left edge.
const PATCH: [i32; 4] = [0, 0x38, 0x37, 0xA0];
/// The patch's ordering-table slot, `5000 >> 4`, at view Z 5000: queued
/// ahead of the parts already in it, and depth tested.
const PATCH_Z: i32 = 5000;
const PATCH_SLOT: u32 = (PATCH_Z >> 4) as u32;
/// The reflection's shadow's slot.
const SHADOW_SLOT: u32 = 0x96;

/// The keys the page's statics are kept under past the screen
/// ([`ScreenStatics`]), as the executable keeps them until it exits.
const KEPT: [&str; 2] = ["option_model_0", "option_model_1"];

/// The page's model and the files it plays.
#[derive(Default)]
pub(super) struct OptionModel {
    /// The model the last `make` set, with its files and what they were
    /// read for: the character, the weapon and the disc.
    model: Option<(PageModel, PageAssets, (u8, u8, u8))>,
    /// The pack's shadow page (`shadow/kage`), once looked for.
    kage: Option<Option<Image>>,
    /// The player's work when the screen is play's (`Config`): the next
    /// `make` is `pPl` as play left it ([`PageModel::from_play`]).
    player: Option<Box<Work>>,
}

impl OptionModel {
    /// The screen is play's: the page's model will be the player's `work`.
    pub(super) fn set_player(&mut self, work: &Work) {
        self.player = Some(Box::new(work.clone()));
    }

    /// The model, once made.
    pub(super) fn model(&self) -> Option<&PageModel> {
        self.model.as_ref().map(|(model, _, _)| model)
    }
}

/// An `option_model` call, checked before it touches the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Make {
        id: u8,
        kind: u16,
        disc: u8,
    },
    Start,
    Walk,
    WalkOn,
    Place,
    Face,
    TurnBack,
    Row(i32),
    Trace,
    /// The page's draw, with the room's picture handle and where the
    /// screen drew it (`draw_background`'s origin), for the patch.
    Draw(Option<(usize, [i32; 2])>),
}

/// The largest `Id`: Sherry's.
const LAST_ID: i64 = 0xF;
/// The list's last row.
const LAST_ROW: i64 = 10;
/// The last weapon of `Pl_weapon_ch`'s tables: the gatling gun.
const LAST_WEAPON: i64 = 0x13;

/// The call's arguments, which must be Lua integers (a float isn't
/// truncated to one), exactly as many as the operation takes.
fn integers<const N: usize>(args: &[mlua::Value]) -> Option<[i64; N]> {
    if args.len() != N {
        return None;
    }
    let mut out = [0; N];
    for (out, arg) in out.iter_mut().zip(args) {
        let mlua::Value::Integer(value) = arg else {
            return None;
        };
        *out = *value;
    }
    Some(out)
}

fn parse(op: &str, args: &[mlua::Value]) -> mlua::Result<Op> {
    let error = |what: &str| Err(mlua::Error::RuntimeError(format!("option_model: {what}")));
    let none = |op| {
        if args.is_empty() {
            Ok(op)
        } else {
            error("this operation takes no arguments")
        }
    };
    match op {
        "make" => match integers(args) {
            Some([id @ 0..=LAST_ID, kind @ 0..=0xFFFF, disc @ 0..=1])
                if kind & 0xFFF <= LAST_WEAPON =>
            {
                Ok(Op::Make {
                    id: id as u8,
                    kind: kind as u16,
                    disc: disc as u8,
                })
            }
            _ => error(
                "\"make\" takes a character from 0 to 15, a 16-bit weapon type whose low 12 bits are a weapon up to 0x13, and a disc of 0 or 1",
            ),
        },
        "row" => match integers(args) {
            Some([row @ 0..=LAST_ROW]) => Ok(Op::Row(row as i32)),
            _ => error("\"row\" takes a row from 0 to 10"),
        },
        "start" => none(Op::Start),
        "walk" => none(Op::Walk),
        "walk_on" => none(Op::WalkOn),
        "place" => none(Op::Place),
        "face" => none(Op::Face),
        "turn_back" => none(Op::TurnBack),
        "trace" => none(Op::Trace),
        "draw" if args.is_empty() => Ok(Op::Draw(None)),
        "draw" => match integers(args) {
            Some([handle, x, y]) if handle >= 0 => Ok(Op::Draw(Some((
                usize::try_from(handle).unwrap_or(usize::MAX),
                [x, y].map(|side| side.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32),
            )))),
            _ => error("\"draw\" takes nothing, or the room's picture and where it was drawn"),
        },
        _ => error(&format!("no operation {op:?}")),
    }
}

/// The statics a screen kept, if it kept them.
fn kept(statics: Option<&ScreenStatics>, entry: &str) -> Option<Statics> {
    let statics = statics?;
    let words = KEPT.map(|key| statics.get(entry, key));
    Some(Statics::from_words([words[0]?, words[1]?]))
}

fn keep(statics: Option<&ScreenStatics>, entry: &str, page: &Statics) {
    if let Some(statics) = statics {
        for (key, word) in KEPT.into_iter().zip(page.to_words()) {
            // The title keeps few enough values that this can't be refused.
            statics.set(entry, key, Some(word));
        }
    }
}

pub(super) fn add_methods<'a, M: UserDataMethods<Api<'a>>>(methods: &mut M) {
    // `option_model(op, ...)`: the KEYCONFIG page's model.
    // `"make", id, type, disc` sets character `id` holding weapon `type`
    // in `Config_set_data`'s place, from disc `disc`'s files (false when
    // the pack has no such files);
    // `"start"`, `"walk"`, `"walk_on"`, `"place"`, `"face"`, `"turn_back"`
    // and `"row", row` are the page's calls (see `lua-types/screen.lua`);
    // `"trace"` is the trace's `kcm`, nil with no model; `"draw"` draws it.
    methods.add_method_mut(
        "option_model",
        |lua, api, (op, args): (String, mlua::Variadic<mlua::Value>)| {
            let op = parse(&op, &args)?;
            let Api {
                host,
                cx,
                framebuffer,
                ..
            } = api;
            let page = &mut host.option_model;
            let entry = host.entry.as_str();
            if let Op::Make { id, kind, disc } = op {
                let weapon = (kind & 0xFFF) as u8;
                let (last, assets) = match page.model.take() {
                    Some((model, assets, files)) => {
                        (Some(model), (files == (id, weapon, disc)).then_some(assets))
                    }
                    None => (None, None),
                };
                let assets = match assets {
                    Some(assets) => Ok(assets),
                    None => PageAssets::load(cx.pack, id, weapon, disc),
                };
                let assets = match assets {
                    Ok(assets) => assets,
                    Err(err) => {
                        // `Pl_weapon_ch` returns before its set-up when
                        // the file isn't there; the last model goes with
                        // the visit.
                        crate::warning!("[lua] option_model: {err:#}");
                        return false.into_lua_multi(lua);
                    }
                };
                let handgun = match cx.statics {
                    Some(statics) => statics.re2_equip(id, weapon),
                    None => HandgunFrame::default().equip(id, weapon),
                };
                let model = match (&last, page.player.as_deref()) {
                    (Some(model), _) => model.again(id, kind, handgun),
                    (None, Some(player)) => PageModel::from_play(
                        id,
                        kind,
                        kept(cx.statics, entry).unwrap_or_default(),
                        handgun,
                        player,
                    ),
                    (None, None) => PageModel::new(
                        id,
                        kind,
                        kept(cx.statics, entry).unwrap_or_default(),
                        handgun,
                    ),
                };
                keep(cx.statics, entry, &model.statics);
                page.model = Some((model, assets, (id, weapon, disc)));
                return true.into_lua_multi(lua);
            }
            let Some((model, assets, _)) = page.model.as_mut() else {
                return mlua::Value::Nil.into_lua_multi(lua);
            };
            match op {
                Op::Start => model.start(assets),
                Op::Walk => model.walk(assets),
                Op::WalkOn => model.walk_on(),
                Op::Place => model.place(),
                Op::Face => model.face(assets),
                Op::TurnBack => model.turn_back(),
                Op::Row(row) => model.demonstrate(assets, row),
                Op::Trace => return model.trace().into_lua_multi(lua),
                Op::Draw(background) => {
                    let framebuffer = framebuffer.as_deref_mut().ok_or_else(super::no_draw)?;
                    let kage = page.kage.get_or_insert_with(|| kage_page(cx.pack)).as_ref();
                    let picture = match background {
                        Some((handle, origin)) => {
                            host.assets.picture(handle)?;
                            Some(origin)
                        }
                        None => None,
                    };
                    let (assets_store, pack) = (&host.assets, cx.pack);
                    let mut redraw = |framebuffer: &mut Framebuffer, rect: [i32; 4]| {
                        if let Some((handle, origin)) = background {
                            // Checked above.
                            let _ = super::draw::draw_background_rect(
                                assets_store,
                                pack,
                                framebuffer,
                                handle,
                                origin,
                                rect,
                            );
                        }
                    };
                    draw(framebuffer, model, assets, kage, picture, &mut redraw);
                    return mlua::Value::Nil.into_lua_multi(lua);
                }
                Op::Make { .. } => unreachable!("made above"),
            }
            keep(cx.statics, entry, &model.statics);
            mlua::Value::Nil.into_lua_multi(lua)
        },
    );
}

/// The pack's RE 2 shadow page, baked as play's.
fn kage_page(pack: &crate::pack::Pack) -> Option<Image> {
    let asset = pack
        .read_asset(crate::shadow::KAGE_STEM, crate::pack::AssetClass::Image)
        .ok()??;
    crate::assets::decode_page(&asset, crate::assets::Geometry::Shipped)
        .and_then(|art| crate::shadow::bake_re2(&art))
        .map_err(|err| crate::warning!("[lua] option_model: {}: {err:#}", asset.path()))
        .ok()
}

/// The page's two views: the model's, and the reflection's with only its
/// matrix's `m[0][0]` negated.
pub(super) fn views() -> (Camera, Camera, Camera) {
    let main = Camera::from_set_view(MAIN_VIEW.0, MAIN_VIEW.1, PAGE_FOV);
    let second = Camera::from_set_view(MIRROR_VIEW.0, MIRROR_VIEW.1, PAGE_FOV);
    (main, second, second.with_x_scale_negated())
}

/// `FUN_0044db30` as its ordering table walks it, under one depth buffer:
/// the reflection's parts (through the negated view, culled by its turned
/// winding) from the furthest slot, the picture's patch at view Z 5000
/// ahead of slot 312's parts, the reflection's shadow (through the plain
/// second view, at the negated x) ahead of slot 0x96's, then the model.
/// Each pass's lights are latched at part 0 as the pass before it left it
/// (`FUN_0044d890` reads `Workm.t` before the pass computes it): the
/// model's, drawn first, at the last draw's root, and the reflection's at
/// this draw's, unreflected, since only its view is mirrored. `background` is where the screen
/// drew the room's picture, for the patch; `redraw` draws the frame's
/// rect of it again where the frame keeps no pixels (the GPU's). Claire's
/// hair swing (`Kami_rot05`) and the other characters' extra parts aren't
/// drawn.
fn draw(
    framebuffer: &mut Framebuffer,
    model: &mut PageModel,
    assets: &PageAssets,
    kage: Option<&Image>,
    background: Option<[i32; 2]>,
    redraw: &mut dyn FnMut(&mut Framebuffer, [i32; 4]),
) {
    let joints = model.joints(assets);
    let offset = super::draw::band_centre(framebuffer, [0, 0]);
    let (main, second, mirror) = views();
    let root = joints.first().map_or(model.body.pos, |root| root.t);
    let lit = |colour, pos| CharacterShade::Lit {
        light: page_light(pos),
        modulate: modulate(colour),
    };
    let main_root = model.pass_root(root);
    let reflected = lit(MIRROR_COLOUR, model.pass_root(root));
    // The patch's rect in the frame, as `draw_background` places the
    // picture.
    let patch = background.map(|origin| {
        let left = origin[0] + (framebuffer.width as i32 - crate::render::WIDTH as i32) / 2;
        let top = origin[1] + (framebuffer.height as i32 - crate::render::HEIGHT as i32) / 2;
        [left + PATCH[0], top + PATCH[1], PATCH[2], PATCH[3]]
    });
    let saved = patch.and_then(|rect| framebuffer.save_rect(rect));
    // Marni draws the page as play: on the 640x480 display, its depth each
    // corner's `sz`.
    let half = Some(crate::render::re2_near_clip(PAGE_FOV) as f32);
    framebuffer.with_display(|framebuffer| {
        framebuffer.hold_depth();
        let reflection = |framebuffer: &mut Framebuffer, slots| {
            framebuffer.draw_character_shaded(
                &assets.body,
                &joints,
                &mirror,
                offset,
                &reflected,
                true,
                Slots::Parts(slots),
            );
        };
        reflection(framebuffer, PATCH_SLOT + 1..=u32::MAX);
        match (&saved, patch) {
            (Some(saved), _) => framebuffer.restore_rect_at(saved, PATCH_Z as f32, half),
            (None, Some(rect)) => redraw(framebuffer, rect),
            (None, None) => {}
        }
        reflection(framebuffer, SHADOW_SLOT + 1..=PATCH_SLOT);
        if let (Some(texture), Some(shadow)) = (kage, model.shadow(&joints)) {
            let shadow = crate::render::Shadow {
                texture,
                pos: shadow.pos,
                angle: shadow.angle,
                half_x: shadow.half.0,
                half_z: shadow.half.1,
                shift: [0, 0],
                lift: 0,
                tint: crate::shadow::kage_tint(crate::shadow::PLAYER_COLOR),
            };
            framebuffer.draw_shadows(&[shadow], &second, offset, half);
        }
        reflection(framebuffer, 0..=SHADOW_SLOT);
        framebuffer.release_depth();
        framebuffer.draw_character_shaded(
            &assets.body,
            &joints,
            &main,
            offset,
            &lit(MAIN_COLOUR, main_root),
            true,
            Slots::All,
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_calls_are_checked_before_the_model() {
        use mlua::Value::{Integer as I, Nil, Number as N};
        assert_eq!(
            parse("make", &[I(1), I(0x1003), I(1)]).unwrap(),
            Op::Make {
                id: 1,
                kind: 0x1003,
                disc: 1
            }
        );
        assert_eq!(
            parse("make", &[I(0), I(0x13), I(0)]).unwrap(),
            Op::Make {
                id: 0,
                kind: 0x13,
                disc: 0
            }
        );
        for bad in [
            &[I(256), I(2), I(0)][..],
            &[I(-1), I(2), I(0)],
            // Past the gatling gun, in the low 12 bits.
            &[I(0), I(0x14), I(0)],
            &[I(0), I(0x102), I(0)],
            &[I(0), I(0x10000), I(0)],
            &[I(0), I(2), I(2)],
            // Floats aren't truncated.
            &[N(-0.5), N(2.9), I(0)],
            &[N(0.0), I(2), I(0)],
            &[I(0), I(2)],
            &[I(0), I(2), I(0), I(0)],
            &[I(0), Nil, I(0)],
        ] {
            assert!(parse("make", bad).is_err(), "{bad:?}");
        }
        assert_eq!(parse("row", &[I(10)]).unwrap(), Op::Row(10));
        assert!(parse("row", &[]).is_err());
        assert!(parse("row", &[I(11)]).is_err());
        assert!(parse("row", &[N(10.9)]).is_err());
        assert!(parse("row", &[I(1), I(2)]).is_err());
        assert!(parse("jump", &[]).is_err());
        assert_eq!(parse("trace", &[]).unwrap(), Op::Trace);
        assert!(parse("start", &[I(1)]).is_err());
    }

    #[test]
    fn the_models_colours_are_doubled_over_the_lit_colour() {
        assert_eq!(modulate(MAIN_COLOUR), 254);
        assert_eq!(modulate(MIRROR_COLOUR), 164);
    }

    #[test]
    fn the_pages_lights_are_latched_at_the_light_matrices_scale() {
        let light = page_light(crate::player2::keyconfig::PLACE);
        assert_eq!(light.ambient, [79.0; 3]);
        // The far point light, 25836 off at x and z of its 26000, keeps
        // under 2% of each raw channel: green's 1, which halving lost.
        assert_eq!(light.sources[0].color, [4, 1, 0]);
        // The directional light at its raw byte.
        assert_eq!(light.sources[1].color, [0x30; 3]);
        let dir = light.sources[1].dir;
        let length = (9880f64.powi(2) + 3600f64.powi(2) + 900f64.powi(2)).sqrt();
        assert_eq!(
            dir,
            [
                (9880.0 / length * 4096.0) as i32,
                (3600.0 / length * 4096.0) as i32,
                (-900.0 / length * 4096.0) as i32
            ]
        );
        assert_eq!(light.sources[2].color, [0; 3]);
    }

    #[test]
    fn the_draw_takes_the_pictures_handle_and_origin() {
        use mlua::Value::{Integer as I, Number as N};
        assert_eq!((PATCH_SLOT, SHADOW_SLOT), (312, 150));
        assert_eq!(parse("draw", &[]).unwrap(), Op::Draw(None));
        assert_eq!(
            parse("draw", &[I(3), I(0), I(-30)]).unwrap(),
            Op::Draw(Some((3, [0, -30])))
        );
        assert!(parse("draw", &[I(3)]).is_err());
        assert!(parse("draw", &[I(-1), I(0), I(0)]).is_err());
        assert!(parse("draw", &[I(3), N(0.5), I(0)]).is_err());
        assert!(parse("draw_back", &[]).is_err());
    }

    #[test]
    fn the_views_are_set_views() {
        let (main, second, mirror) = views();
        assert_eq!(
            (main.from, main.look_at, main.fov),
            ([-100, 80, 700], [-100, 80, 1700], 0xD0)
        );
        assert_eq!(
            (second.from, second.look_at),
            ([-100, 80, 8500], [-100, 80, 7500])
        );
        assert_eq!(mirror.view[0][0], -second.view[0][0]);
        assert_eq!(mirror.trans, second.trans);
    }

    /// A page model of one part holding two triangles, one wound each way
    /// so both views draw one, in a mid-grey texture.
    fn page() -> (PageModel, PageAssets) {
        use crate::model::{Clip, ClipFrame, Keyframe, Skeleton, Texture8, TmdObject, TmdPrim};
        let prim = |vertices| TmdPrim {
            vertices,
            normals: [0; 3],
            uv: [[0, 0]; 3],
            clut: 0x7800,
            tsb: 0x80,
            textured: true,
            blend: false,
            mix: crate::model::PacketMix::Weighted,
            raw_y: false,
            flat_color: None,
            quad: None,
            uv_nudge: [0; 3],
        };
        let object = TmdObject {
            vertices: vec![[-300, 0, 0], [300, 0, 0], [0, -900, 0]],
            normals: vec![[0, 0, -4096]],
            prims: vec![prim([0, 1, 2]), prim([0, 2, 1])],
        };
        let skeleton = Skeleton {
            relative: vec![[0; 3]],
            children: vec![Vec::new()],
        };
        let keyframes = vec![Keyframe {
            offset: [0; 3],
            vertex: [0; 3],
            rotations: vec![[0; 3]],
        }];
        let clip = Clip {
            frames: vec![ClipFrame {
                keyframe: 0,
                timing: 0,
                flags: 0,
            }],
        };
        let body = crate::model::Emd {
            marni: None,
            skeleton: skeleton.clone(),
            keyframes: keyframes.clone(),
            clips: vec![clip.clone(); 16],
            damage_clips: Vec::new(),
            damage_keyframes: Vec::new(),
            banks: Vec::new(),
            mesh: crate::model::Tmd {
                objects: vec![object],
            },
            texture: Texture8 {
                width: 1,
                height: 1,
                indices: vec![1],
                palettes: vec![[0, 0, 0, 255], [200, 200, 200, 255]],
                stp: Vec::new(),
            },
            morphs: Vec::new(),
        };
        let weapon = crate::model::Emw {
            skeleton,
            keyframes,
            clips: vec![clip; 16],
            mesh: Default::default(),
        };
        let model = PageModel::new(0, 2, Statics::default(), HandgunFrame::default());
        (model, PageAssets::new(body, weapon))
    }

    /// Each pass is its lit colour times its own modulate, in integers:
    /// the page's triangle faces away from both lights, so it is the
    /// ambient's 79 (and the far point light's red), 78 or 50 of 255 of the
    /// texel's 200. The model is drawn to the left of the reflection.
    #[test]
    fn each_pass_is_its_lit_colour_times_its_modulate() {
        let (mut model, assets) = page();
        let mut framebuffer = Framebuffer::new();
        draw(
            &mut framebuffer,
            &mut model,
            &assets,
            None,
            None,
            &mut |_, _| {},
        );
        let side = |range: std::ops::Range<usize>| {
            let mut found = std::collections::BTreeSet::new();
            let painted = |x: usize, y: usize| {
                let at = (y * 320 + x) * 4;
                framebuffer.rgba[at..at + 3] != [0; 3]
            };
            for y in 1..239 {
                for x in range.clone().filter(|x| (1..319).contains(x)) {
                    let at = (y * 320 + x) * 4;
                    let pixel = &framebuffer.rgba[at..at + 3];
                    // A face's inside: on RE2's display its edges are halved
                    // with what's beside them.
                    let inside = [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]
                        .iter()
                        .all(|&(x, y)| painted(x, y));
                    if painted(x, y) && inside {
                        found.insert([pixel[0], pixel[1], pixel[2]]);
                    }
                }
            }
            found
        };
        // The model's part 0 is far from the point light: 200 * (79 * 254
        // / 255 = 78) / 255 in green and blue, and red has some of the
        // light's 4 too.
        let main = side(0..70);
        assert_eq!(main.len(), 1, "{main:?}");
        let [red, green, blue] = main.into_iter().next().unwrap();
        assert_eq!([green, blue], [koo(79, 254); 2]);
        assert!((koo(79, 254)..=koo(83, 254)).contains(&red), "{red}");
        // The reflection is lit at the same, unreflected root, at 164 of
        // 255: at the negated x the light's [30, 11, 6] would show in all
        // three.
        let mirror = side(70..320);
        assert_eq!(mirror.len(), 1, "{mirror:?}");
        let [red, green, blue] = mirror.into_iter().next().unwrap();
        assert_eq!([green, blue], [koo(79, 164); 2]);
        assert!((koo(79, 164)..=koo(83, 164)).contains(&red), "{red}");
    }

    /// Each pass's lights are latched at the root the pass before it left:
    /// the model's at the last draw's, the reflection's at this draw's.
    #[test]
    fn each_pass_is_lit_at_the_root_the_last_pass_left() {
        let (mut model, assets) = page();
        let root = model.joints(&assets)[0].t;
        let mut framebuffer = Framebuffer::new();
        draw(
            &mut framebuffer,
            &mut model,
            &assets,
            None,
            None,
            &mut |_, _| {},
        );
        let moved = [root[0] + 100, root[1], root[2]];
        assert_eq!(model.pass_root(moved), root, "the model's");
        assert_eq!(model.pass_root(moved), moved, "the reflection's");
    }

    /// The texel's 200 under a lit byte `lit` times `modulate` / 255.
    fn koo(lit: u32, modulate: u32) -> u8 {
        let colour = lit * modulate / 255;
        (200.0 / 255.0 * (colour as f32 / 255.0) * 255.0).round() as u8
    }

    /// The patch is the frame as the picture left it, put back between the
    /// reflection's slots at view Z 5000: over the reflection, which is
    /// further, and only in its rect.
    #[test]
    fn the_patch_puts_the_picture_back_over_the_reflection() {
        let (mut model, assets) = page();
        let fresh = || {
            let mut framebuffer = Framebuffer::new();
            for pixel in framebuffer.rgba.as_chunks_mut::<4>().0 {
                *pixel = [10, 20, 30, 255];
            }
            framebuffer
        };
        // The picture drawn from (70, 0): its strip covers x 70 to 124.
        let rect = [70, PATCH[1], PATCH[2], PATCH[3]];
        let drawn_in = |framebuffer: &Framebuffer, inside: bool| {
            let mut count = 0;
            for y in 0..240i32 {
                for x in 0..320i32 {
                    let within = x >= rect[0]
                        && x < rect[0] + rect[2]
                        && y >= rect[1]
                        && y < rect[1] + rect[3];
                    let at = (y * 320 + x) as usize * 4;
                    if within == inside && framebuffer.rgba[at..at + 3] != [10, 20, 30] {
                        count += 1;
                    }
                }
            }
            count
        };
        let mut plain = fresh();
        draw(&mut plain, &mut model, &assets, None, None, &mut |_, _| {});
        assert!(
            drawn_in(&plain, true) > 0,
            "the reflection reaches the strip"
        );
        let mut patched = fresh();
        draw(
            &mut patched,
            &mut model,
            &assets,
            None,
            Some([70, 0]),
            &mut |_, _| panic!("a software frame keeps its pixels"),
        );
        assert_eq!(drawn_in(&patched, true), 0, "the strip is the picture's");
        assert_eq!(drawn_in(&patched, false), drawn_in(&plain, false));
    }

    /// Under the GPU renderer the whole page, the shadow included, is queued
    /// for it: no software 3D, so the frame isn't refused.
    #[test]
    fn the_page_draws_on_a_recording_frame() {
        let (mut model, assets) = page();
        let kage = Image {
            width: 4,
            height: 4,
            rgba: [0, 0, 0, 128].repeat(16),
        };
        let mut framebuffer = Framebuffer::new();
        framebuffer.record();
        let mut redrawn = Vec::new();
        draw(
            &mut framebuffer,
            &mut model,
            &assets,
            Some(&kage),
            Some([0, 0]),
            &mut |_, rect| redrawn.push(rect),
        );
        // The patch drawn again from the picture, where the frame keeps no
        // pixels to put back.
        assert_eq!(redrawn, [[0, 0x38, 0x37, 0xA0]]);
        let list = framebuffer.list().expect("recording");
        assert_eq!(list.error, None);
        let models = list
            .items
            .iter()
            .filter(|item| matches!(item.kind, crate::draw2d::Kind::Model(_)))
            .count();
        assert!(models >= 2, "the reflection and the model");
        assert!(
            list.items
                .iter()
                .any(|item| matches!(item.kind, crate::draw2d::Kind::Sprite)),
            "the shadow"
        );
    }

    /// On a 320 by 180 band the picture is centred 30 rows up, and the
    /// patch with it: from its origin of 0, 0 the strip is rows 26 to 185,
    /// covering the band's foot.
    #[test]
    fn the_patch_follows_the_picture_on_a_band() {
        let (mut model, assets) = page();
        let mut framebuffer = Framebuffer::with_size(320, 180);
        framebuffer.record();
        let mut redrawn = Vec::new();
        draw(
            &mut framebuffer,
            &mut model,
            &assets,
            None,
            Some([0, 0]),
            &mut |_, rect| redrawn.push(rect),
        );
        assert_eq!(redrawn, [[0, 0x38 - 30, 0x37, 0xA0]]);
        let [_, top, _, height] = redrawn[0];
        assert!(top + height >= 180, "the band's last row is patched");
    }

    #[test]
    fn the_statics_outlive_the_screen() {
        let statics = ScreenStatics::default();
        assert_eq!(kept(Some(&statics), "ui/title.lua"), None);
        let page = Statics {
            turned: true,
            returning: true,
            key: 0x140,
            count: 0x4A,
            step: 3,
            kept: 2,
            row: 5,
            last_key: 0x201,
        };
        keep(Some(&statics), "ui/title.lua", &page);
        // A title made again reads them back.
        assert_eq!(kept(Some(&statics), "ui/title.lua"), Some(page));
    }
}
