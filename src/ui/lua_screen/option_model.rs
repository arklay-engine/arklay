//! `api:option_model`: the player's model beside RE 2's KEYCONFIG list
//! ([`crate::player2::keyconfig`]). The engine runs the model; the title's
//! script calls it at the points `Config_main` and its draw do.

use mlua::{IntoLuaMulti, UserDataMethods};

use crate::player2::HandgunFrame;
use crate::player2::keyconfig::{PageAssets, PageModel, Statics};
use crate::ui::ScreenStatics;

use super::Api;

/// The keys the page's statics are kept under past the screen
/// ([`ScreenStatics`]), as the executable keeps them until it exits.
const KEPT: [&str; 2] = ["option_model_0", "option_model_1"];

/// The page's model and the files it plays.
#[derive(Default)]
pub(super) struct OptionModel {
    /// The model the last `make` set, with its files and what they were
    /// read for: the character, the weapon and the disc.
    model: Option<(PageModel, PageAssets, (u8, u8, u8))>,
}

/// An `option_model` call, checked before it touches the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Make { id: u8, kind: u16, disc: u8 },
    Start,
    Walk,
    WalkOn,
    Place,
    Face,
    TurnBack,
    Row(i32),
    Trace,
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
    // `"trace"` is the trace's `kcm`, nil with no model.
    methods.add_method_mut(
        "option_model",
        |lua, api, (op, args): (String, mlua::Variadic<mlua::Value>)| {
            let op = parse(&op, &args)?;
            let Api { host, cx, .. } = api;
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
                let model = match &last {
                    Some(model) => model.again(id, kind, handgun),
                    None => PageModel::new(
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
                Op::Make { .. } => unreachable!("made above"),
            }
            keep(cx.statics, entry, &model.statics);
            mlua::Value::Nil.into_lua_multi(lua)
        },
    );
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
