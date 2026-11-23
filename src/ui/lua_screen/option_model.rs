//! `api:option_model`: the player's model beside RE 2's KEYCONFIG list
//! ([`crate::player2::keyconfig`]). The engine runs the model; the title's
//! script calls it at the points `Config_main` and its draw do.

use mlua::{IntoLuaMulti, UserDataMethods};

use crate::player2::keyconfig::{PageAssets, PageModel, Statics};
use crate::ui::ScreenStatics;

use super::Api;

/// The keys the page's statics are kept under past the screen
/// ([`ScreenStatics`]), as the executable keeps them until it exits.
const KEPT: [&str; 2] = ["option_model_0", "option_model_1"];

/// The page's model and the files it plays.
#[derive(Default)]
pub(super) struct OptionModel {
    /// The model the last `make` set, with its files.
    model: Option<(PageModel, PageAssets)>,
}

/// An `option_model` call, checked before it touches the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Make { id: u8, kind: u16 },
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

fn parse(op: &str, a: Option<i64>, b: Option<i64>) -> mlua::Result<Op> {
    let error = |what: &str| Err(mlua::Error::RuntimeError(format!("option_model: {what}")));
    Ok(match op {
        "make" => match (a, b) {
            (Some(id @ 0..=LAST_ID), Some(kind @ 0..=0xFFFF)) if kind & 0xFFF <= 0xFF => Op::Make {
                id: id as u8,
                kind: kind as u16,
            },
            _ => {
                return error("\"make\" takes a character from 0 to 15 and a 16-bit weapon type");
            }
        },
        "row" => match a {
            Some(row @ 0..=LAST_ROW) => Op::Row(row as i32),
            _ => return error("\"row\" takes a row from 0 to 10"),
        },
        "start" => Op::Start,
        "walk" => Op::Walk,
        "walk_on" => Op::WalkOn,
        "place" => Op::Place,
        "face" => Op::Face,
        "turn_back" => Op::TurnBack,
        "trace" => Op::Trace,
        _ => return error(&format!("no operation {op:?}")),
    })
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
    // `"make", id, type` sets character `id` holding weapon `type` in
    // `Config_set_data`'s place (false when the pack has no such files);
    // `"start"`, `"walk"`, `"walk_on"`, `"place"`, `"face"`, `"turn_back"`
    // and `"row", row` are the page's calls (see `lua-types/screen.lua`);
    // `"trace"` is the trace's `kcm`, nil with no model.
    methods.add_method_mut(
        "option_model",
        |lua, api, (op, a, b): (String, Option<i64>, Option<i64>)| {
            let op = parse(&op, a, b)?;
            let Api { host, cx, .. } = api;
            let page = &mut host.option_model;
            let entry = host.entry.as_str();
            if let Op::Make { id, kind } = op {
                let weapon = (kind & 0xFFF) as u8;
                let last = page.model.take();
                let model = match &last {
                    Some((model, _)) => model.again(id, kind),
                    None => PageModel::new(id, kind, kept(cx.statics, entry).unwrap_or_default()),
                };
                let assets = match last {
                    Some((last, assets))
                        if last.id == id && last.work.kind & 0xFFF == kind & 0xFFF =>
                    {
                        Ok(assets)
                    }
                    _ => PageAssets::load(cx.pack, id, weapon),
                };
                keep(cx.statics, entry, &model.statics);
                return match assets {
                    Ok(assets) => {
                        page.model = Some((model, assets));
                        true.into_lua_multi(lua)
                    }
                    Err(err) => {
                        crate::warning!("[lua] option_model: {err:#}");
                        false.into_lua_multi(lua)
                    }
                };
            }
            let Some((model, assets)) = page.model.as_mut() else {
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
        assert_eq!(
            parse("make", Some(1), Some(0x1003)).unwrap(),
            Op::Make {
                id: 1,
                kind: 0x1003
            }
        );
        assert!(parse("make", Some(256), Some(2)).is_err());
        assert!(parse("make", Some(-1), Some(2)).is_err());
        assert!(parse("make", Some(0), Some(0x102)).is_err());
        assert!(parse("make", Some(0), Some(0x10000)).is_err());
        assert!(parse("make", Some(0), None).is_err());
        assert_eq!(parse("row", Some(10), None).unwrap(), Op::Row(10));
        assert!(parse("row", None, None).is_err());
        assert!(parse("row", Some(11), None).is_err());
        assert!(parse("jump", None, None).is_err());
        assert_eq!(parse("trace", None, None).unwrap(), Op::Trace);
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
