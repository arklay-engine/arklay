//! `api:ex_models`: the four players on RE 2's Extreme Battle character
//! choice ([`crate::player2::exbattle`]). The choice's script turns the
//! stage and calls the model's steps where `ExBattle` runs them; the engine
//! plays the motions and the lights and draws them.

use mlua::{IntoLuaMulti, UserDataMethods};

use crate::anim::Mat4x3;
use crate::player2::exbattle::{self, ExAssets, ExModel, WORKS};
use crate::render::{Camera, CharacterShade, EntityLight, Slots};

use super::Api;

/// The four works, their files, and what the last tick's sort left to draw.
#[derive(Default)]
pub(super) struct ExModels {
    works: Vec<ExModel>,
    assets: Vec<Option<ExAssets>>,
    /// Each work's parts and light as `ExPl_move` last sorted them.
    sorted: Vec<Option<(Vec<Mat4x3>, EntityLight)>>,
}

/// An `ex_models` call, checked before it touches the works.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Load {
        disc: u8,
    },
    Place {
        slot: usize,
        x: i32,
        z: i32,
        cdir: u16,
    },
    Confirm {
        slot: usize,
    },
    Tick {
        cursor: usize,
    },
    Draw,
    Trace,
}

/// The call's arguments, which must be Lua integers, exactly as many as the
/// operation takes.
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

const LAST_SLOT: i64 = WORKS as i64 - 1;
/// How far a place may be from the stage's centre.
const REACH: i64 = 0x8000;

fn parse(op: &str, args: &[mlua::Value]) -> mlua::Result<Op> {
    let error = |what: &str| Err(mlua::Error::RuntimeError(format!("ex_models: {what}")));
    let none = |op| {
        if args.is_empty() {
            Ok(op)
        } else {
            error("this operation takes no arguments")
        }
    };
    match op {
        "load" => match integers(args) {
            Some([disc @ 0..=1]) => Ok(Op::Load { disc: disc as u8 }),
            _ => error("\"load\" takes a disc of 0 or 1"),
        },
        "place" => match integers(args) {
            Some([slot @ 0..=LAST_SLOT, x, z, cdir])
                if (-REACH..REACH).contains(&x) && (-REACH..REACH).contains(&z) =>
            {
                Ok(Op::Place {
                    slot: slot as usize,
                    x: x as i32,
                    z: z as i32,
                    cdir: cdir as u16,
                })
            }
            _ => error(
                "\"place\" takes a work from 0 to 3, x and z within 0x8000 of the centre, and a turn",
            ),
        },
        "confirm" => match integers(args) {
            Some([slot @ 0..=LAST_SLOT]) => Ok(Op::Confirm {
                slot: slot as usize,
            }),
            _ => error("\"confirm\" takes a work from 0 to 3"),
        },
        "tick" => match integers(args) {
            Some([cursor @ 0..=LAST_SLOT]) => Ok(Op::Tick {
                cursor: cursor as usize,
            }),
            _ => error("\"tick\" takes the cursor, a work from 0 to 3"),
        },
        "draw" => none(Op::Draw),
        "trace" => none(Op::Trace),
        _ => error(&format!("no operation {op:?}")),
    }
}

impl ExModels {
    /// `ExPl_animate`, then `ExPl_move`: each work's light (its step, at the
    /// root the last sort left) and its sort.
    fn tick(&mut self, cursor: usize) {
        for (work, assets) in self.works.iter_mut().zip(&self.assets) {
            work.animate(assets.as_ref());
        }
        self.sorted = self
            .works
            .iter_mut()
            .zip(&self.assets)
            .map(|(work, assets)| {
                work.light_step(cursor);
                let light = work.lighting(work.light_root());
                let assets = assets.as_ref()?;
                Some((work.sort(assets), light))
            })
            .collect();
    }

    fn trace(&self) -> Option<String> {
        if self.works.is_empty() {
            return None;
        }
        Some(
            self.works
                .iter()
                .map(ExModel::trace)
                .collect::<Vec<_>>()
                .join(";"),
        )
    }
}

pub(super) fn add_methods<'a, M: UserDataMethods<Api<'a>>>(methods: &mut M) {
    // `ex_models(op, ...)`: Extreme Battle's four players. `"load", disc`
    // sets the four works in their places and reads their models from disc
    // `disc`'s files (false when one is missing; that one isn't drawn);
    // `"place", work, x, z, turn` and `"confirm", work` are the choice's
    // writes to a work; `"tick", cursor` runs `ExPl_animate` and
    // `ExPl_move`; `"trace"` is the trace's `exbm`, nil before a load;
    // `"draw"` draws what the last tick sorted.
    methods.add_method_mut(
        "ex_models",
        |lua, api, (op, args): (String, mlua::Variadic<mlua::Value>)| {
            let op = parse(&op, &args)?;
            let Api {
                host,
                cx,
                framebuffer,
                ..
            } = api;
            let models = &mut host.ex_models;
            match op {
                Op::Load { disc } => {
                    models.works = (0..WORKS).map(ExModel::new).collect();
                    models.sorted = vec![None; WORKS];
                    models.assets = (0..WORKS)
                        .map(|slot| {
                            ExAssets::load(cx.pack, slot, disc)
                                .map_err(|err| crate::warning!("[lua] ex_models: {err:#}"))
                                .ok()
                        })
                        .collect();
                    let all = models.assets.iter().all(Option::is_some);
                    return all.into_lua_multi(lua);
                }
                Op::Trace => return models.trace().into_lua_multi(lua),
                _ => {}
            }
            if models.works.is_empty() {
                return mlua::Value::Nil.into_lua_multi(lua);
            }
            match op {
                Op::Place { slot, x, z, cdir } => models.works[slot].place(x, z, cdir),
                Op::Confirm { slot } => models.works[slot].confirm(),
                Op::Tick { cursor } => models.tick(cursor),
                Op::Draw => {
                    let framebuffer = framebuffer.as_deref_mut().ok_or_else(super::no_draw)?;
                    draw(framebuffer, models);
                }
                Op::Load { .. } | Op::Trace => unreachable!("answered above"),
            }
            mlua::Value::Nil.into_lua_multi(lua)
        },
    );
}

/// The view `Ex_set_view` sets.
fn view() -> Camera {
    Camera::from_set_view(exbattle::VIEW.0, exbattle::VIEW.1, exbattle::FOV)
}

/// `ExPl_sort`'s draws of the four, through `MarniSortEmdGallery` under
/// one depth buffer: each part lit by its work's light and the ambient,
/// at `Poly_rgb`'s colour. Ada's seams (`MulJh`, `Ex_sort_jh`) aren't
/// closed: her part 0 draws as the others do.
fn draw(framebuffer: &mut crate::render::Framebuffer, models: &ExModels) {
    let camera = view();
    let offset = super::draw::band_centre(framebuffer, [0, 0]);
    framebuffer.with_display(|framebuffer| {
        framebuffer.hold_depth();
        for (sorted, assets) in models.sorted.iter().zip(&models.assets) {
            let (Some((joints, light)), Some(assets)) = (sorted, assets) else {
                continue;
            };
            framebuffer.draw_character_shaded(
                &assets.body,
                joints,
                &camera,
                offset,
                &CharacterShade::Lit {
                    light: *light,
                    modulate: exbattle::POLY_RGB.saturating_mul(2),
                },
                true,
                Slots::All,
            );
        }
        framebuffer.release_depth();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(v: i64) -> mlua::Value {
        mlua::Value::Integer(v)
    }

    #[test]
    fn the_calls_are_checked_before_the_works() {
        assert_eq!(parse("load", &[int(1)]).unwrap(), Op::Load { disc: 1 });
        assert!(parse("load", &[int(2)]).is_err());
        assert_eq!(
            parse("place", &[int(3), int(-2250), int(0), int(2048)]).unwrap(),
            Op::Place {
                slot: 3,
                x: -2250,
                z: 0,
                cdir: 2048
            }
        );
        assert!(parse("place", &[int(4), int(0), int(0), int(0)]).is_err());
        assert!(parse("tick", &[]).is_err());
        assert!(parse("draw", &[int(0)]).is_err());
        assert!(parse("spin", &[]).is_err());
    }

    /// Before a load there are no works: a trace is nil and the steps do
    /// nothing. After one, the trace has the four works' motions.
    #[test]
    fn the_trace_has_the_four_works_once_loaded() {
        let mut models = ExModels::default();
        assert_eq!(models.trace(), None);
        models.works = (0..WORKS).map(ExModel::new).collect();
        models.assets = (0..WORKS).map(|_| None).collect();
        models.tick(1);
        assert_eq!(
            models.trace().as_deref(),
            Some("0,00000000;1,00000000;1,00000000;1,00000000")
        );
        assert_eq!(models.works[1].light.state, 1);
    }
}
