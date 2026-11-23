//! `api:option_model`: the player's model beside RE 2's KEYCONFIG list
//! ([`crate::player2::keyconfig`]). The engine runs the model; the title's
//! script calls it at the points `Config_main` and its draw do.

use mlua::{IntoLuaMulti, UserDataMethods};

use crate::player2::keyconfig::{PageAssets, PageModel, Statics};

use super::Api;

/// The page's model and the files it plays, with the statics that outlive
/// every model the page makes.
#[derive(Default)]
pub(super) struct OptionModel {
    statics: Statics,
    /// The model the last `make` set, with its files.
    model: Option<(PageModel, PageAssets)>,
}

impl OptionModel {
    /// The page's statics: the current model's, or what the last one left.
    fn statics(&self) -> Statics {
        self.model
            .as_ref()
            .map_or(self.statics, |(model, _)| model.statics)
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
            let page = &mut api.host.option_model;
            if op == "make" {
                let (Some(id), Some(kind)) = (a, b) else {
                    return Err(mlua::Error::RuntimeError(
                        "option_model(\"make\", id, type) takes a character and a weapon".into(),
                    ));
                };
                let (id, kind) = (id as u8, kind as u16);
                let statics = page.statics();
                let assets = match page.model.take() {
                    Some((model, assets)) if model.id == id && model.work.kind == kind => {
                        Ok(assets)
                    }
                    _ => PageAssets::load(api.cx.pack, id, kind as u8),
                };
                page.statics = statics;
                return match assets {
                    Ok(assets) => {
                        page.model = Some((PageModel::new(id, kind, statics), assets));
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
            match op.as_str() {
                "start" => model.start(assets),
                "walk" => model.walk(assets),
                "walk_on" => model.walk_on(),
                "place" => model.place(),
                "face" => model.face(assets),
                "turn_back" => model.turn_back(),
                "row" => model.demonstrate(assets, a.unwrap_or(0) as i32),
                "trace" => return model.trace().into_lua_multi(lua),
                _ => {
                    return Err(mlua::Error::RuntimeError(format!(
                        "option_model: no operation {op:?}"
                    )));
                }
            }
            mlua::Value::Nil.into_lua_multi(lua)
        },
    );
}
