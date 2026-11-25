//! A screen that shows and sets the player's bindings (RE 2's KEYCONFIG
//! page): `on_bindings(api, bindings, defaults)` tells it every action's
//! keyboard keys and gamepad sources, `api:set_bindings(changes)` asks for
//! some of them to be replaced, and the `input` table's `keys` and
//! `buttons` name the keyboard keys and the gamepad sources newly down on a
//! tick.

use mlua::{IntoLuaMulti, Lua, MultiValue, Table, UserDataMethods, Value};

use crate::controls::{ACTIONS, Action, Bindings, Source};

use super::Api;

/// The settings' bindings and the shipped ones, as `on_bindings` takes
/// them: two tables keyed by each action's `[bindings]` name, each entry
/// `{ keys = {...}, pads = {...} }`, the action's keyboard keys and its
/// gamepad buttons and axes by their `device:name`, in their order. Mouse
/// buttons and the wheel are left out: the page binds neither.
pub(crate) struct BindingsArg<'a> {
    pub bindings: &'a Bindings,
    pub defaults: &'a Bindings,
}

impl IntoLuaMulti for BindingsArg<'_> {
    fn into_lua_multi(self, lua: &Lua) -> mlua::Result<MultiValue> {
        let bindings = bindings_table(lua, self.bindings)?;
        let defaults = bindings_table(lua, self.defaults)?;
        (bindings, defaults).into_lua_multi(lua)
    }
}

fn bindings_table(lua: &Lua, bindings: &Bindings) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for action in ACTIONS {
        let entry = lua.create_table()?;
        let names = |pad: bool| -> Vec<String> {
            bindings
                .get(action)
                .iter()
                .filter(|source| on_device(**source, pad))
                .map(Source::to_string)
                .collect()
        };
        entry.set("keys", names(false))?;
        entry.set("pads", names(true))?;
        table.set(action.name(), entry)?;
    }
    Ok(table)
}

/// Whether `source` is on the gamepad (`pad`) or is a keyboard key.
fn on_device(source: Source, pad: bool) -> bool {
    if pad {
        source.is_pad()
    } else {
        matches!(source, Source::Key(_))
    }
}

/// One action's part of a request: the action, its new keyboard keys and
/// its new gamepad sources, each `None` to leave that device as it is.
type BindingChange = (Action, Option<Vec<Source>>, Option<Vec<Source>>);

/// What `api:set_bindings` asks for, an action at a time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BindingsRequest(Vec<BindingChange>);

impl BindingsRequest {
    /// The request from `api:set_bindings`'s table: each key an action's
    /// `[bindings]` name, each value `{ keys = {...}?, pads = {...}? }`. An
    /// unknown action, a name that isn't a source, a key in `pads`, a pad
    /// source in `keys` or a reserved input is an error.
    fn from_table(table: Table) -> mlua::Result<Self> {
        let mut changes = Vec::new();
        for pair in table.pairs::<String, Table>() {
            let (name, entry) = pair?;
            let action = Action::from_name(&name).ok_or_else(|| {
                mlua::Error::RuntimeError(format!("api:set_bindings: unknown action `{name}`"))
            })?;
            let list = |pad: bool| -> mlua::Result<Option<Vec<Source>>> {
                let field = if pad { "pads" } else { "keys" };
                let Some(names) = entry.get::<Option<Vec<String>>>(field)? else {
                    return Ok(None);
                };
                names
                    .iter()
                    .map(|text| {
                        let source = Source::parse(text).map_err(|err| {
                            mlua::Error::RuntimeError(format!(
                                "api:set_bindings: {name}.{field}: {err:#}"
                            ))
                        })?;
                        if !on_device(source, pad) {
                            return Err(mlua::Error::RuntimeError(format!(
                                "api:set_bindings: {name}.{field}: `{text}` is not a {}",
                                if pad {
                                    "gamepad source"
                                } else {
                                    "keyboard key"
                                }
                            )));
                        }
                        Ok(source)
                    })
                    .collect::<mlua::Result<Vec<_>>>()
                    .map(Some)
            };
            changes.push((action, list(false)?, list(true)?));
        }
        // `pairs` has no order: the request applies in the actions' own.
        changes.sort_by_key(|(action, ..)| *action);
        Ok(Self(changes))
    }

    /// Two requests on one tick are one: for each action and device the
    /// later's list, where it gives one, wins.
    fn merge(&mut self, later: Self) {
        for (action, keys, pads) in later.0 {
            match self.0.iter_mut().find(|(a, ..)| *a == action) {
                Some((_, held_keys, held_pads)) => {
                    if keys.is_some() {
                        *held_keys = keys;
                    }
                    if pads.is_some() {
                        *held_pads = pads;
                    }
                }
                None => self.0.push((action, keys, pads)),
            }
        }
        self.0.sort_by_key(|(action, ..)| *action);
    }

    /// `bindings` with the request applied: each listed device's sources
    /// replaced by the request's (an action's mouse buttons and wheel stay,
    /// ahead of its gamepad sources as the shipped layout lists them).
    /// `Err` names an essential action the result would leave without a
    /// key of its own ([`Bindings::lockout`]), or a menu action left without
    /// a gamepad button of its own ([`Bindings::pad_lockout`]) when the
    /// bindings had one; nothing is changed then.
    pub fn apply(&self, bindings: &Bindings) -> Result<Bindings, Action> {
        let mut out = bindings.clone();
        for (action, keys, pads) in &self.0 {
            let current = out.get(*action).to_vec();
            let keys = keys.clone().unwrap_or_else(|| {
                current
                    .iter()
                    .copied()
                    .filter(|&s| on_device(s, false))
                    .collect()
            });
            let others = current
                .iter()
                .copied()
                .filter(|&s| !s.is_pad() && !on_device(s, false));
            let pads = pads
                .clone()
                .unwrap_or_else(|| current.iter().copied().filter(|s| s.is_pad()).collect());
            out.set(
                *action,
                keys.into_iter().chain(others).chain(pads).collect(),
            );
        }
        if let Some(action) = out.lockout() {
            return Err(action);
        }
        if let Some(action) = out.pad_lockout()
            && bindings.pad_lockout().is_none()
        {
            return Err(action);
        }
        Ok(out)
    }
}

/// The `input` table's `keys` and `buttons`: the keyboard keys, and the
/// gamepad buttons and axes, that went down on the tick
/// ([`crate::ui::UiInput::newly_down`]), in order, by `device:name`. A
/// reserved input (F1, F9, the pad's start) and the mouse are neither.
pub(crate) fn pressed_names(pressed: &[Option<Source>]) -> (Vec<String>, Vec<String>) {
    let mut keys = Vec::new();
    let mut buttons = Vec::new();
    for &source in pressed.iter().flatten() {
        if source.reserved_bit().is_some() {
            continue;
        }
        if matches!(source, Source::Key(_)) {
            keys.push(source.to_string());
        } else if source.is_pad() {
            buttons.push(source.to_string());
        }
    }
    (keys, buttons)
}

pub(super) fn add_methods<'a, M: UserDataMethods<Api<'a>>>(methods: &mut M) {
    methods.add_method_mut("set_bindings", |_lua, api, changes: Value| {
        let Value::Table(table) = changes else {
            return Err(mlua::Error::RuntimeError(format!(
                "api:set_bindings takes a table, got {}",
                changes.type_name()
            )));
        };
        let request = BindingsRequest::from_table(table)?;
        let pending = api
            .host
            .bindings_request
            .get_or_insert_with(Default::default);
        pending.merge(request);
        Ok(())
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdl3_sys::gamepad::{
        SDL_GAMEPAD_AXIS_RIGHT_TRIGGER, SDL_GAMEPAD_BUTTON_NORTH, SDL_GAMEPAD_BUTTON_SOUTH,
        SDL_GAMEPAD_BUTTON_START,
    };
    use sdl3_sys::scancode::{
        SDL_SCANCODE_F1, SDL_SCANCODE_I, SDL_SCANCODE_K, SDL_SCANCODE_RETURN, SDL_SCANCODE_UP,
        SDL_SCANCODE_W,
    };

    fn request(lua: &Lua, code: &str) -> mlua::Result<BindingsRequest> {
        BindingsRequest::from_table(lua.load(code).eval()?)
    }

    #[test]
    fn a_request_replaces_only_the_devices_it_lists() {
        let lua = Lua::new();
        let changes = request(
            &lua,
            r#"return { fire = { keys = { "key:K", "key:Return" } }, up = { pads = { "pad:y" } } }"#,
        )
        .unwrap();
        let defaults = Bindings::default();
        let applied = changes.apply(&defaults).unwrap();
        assert_eq!(
            applied.get(Action::Fire),
            [
                Source::key(SDL_SCANCODE_K),
                Source::key(SDL_SCANCODE_RETURN),
                // The mouse stays, and the pad as it was.
                Source::Mouse(1),
                Source::PadButton(SDL_GAMEPAD_BUTTON_SOUTH.0),
                Source::PadAxis(SDL_GAMEPAD_AXIS_RIGHT_TRIGGER.0, true),
            ]
        );
        assert_eq!(
            applied.get(Action::Up),
            [
                Source::key(SDL_SCANCODE_W),
                Source::key(SDL_SCANCODE_UP),
                Source::PadButton(SDL_GAMEPAD_BUTTON_NORTH.0),
            ]
        );
        for action in ACTIONS {
            if action != Action::Fire && action != Action::Up {
                assert_eq!(applied.get(action), defaults.get(action), "{action:?}");
            }
        }
    }

    #[test]
    fn a_request_that_locks_a_menu_out_changes_nothing() {
        let lua = Lua::new();
        // START's keys are Tab and I: none left.
        let changes = request(&lua, r#"return { start = { keys = {} } }"#).unwrap();
        assert_eq!(changes.apply(&Bindings::default()), Err(Action::Start));
        let changes = request(&lua, r#"return { start = { keys = { "key:I" } } }"#).unwrap();
        assert_eq!(
            changes
                .apply(&Bindings::default())
                .unwrap()
                .get(Action::Start),
            [
                Source::key(SDL_SCANCODE_I),
                Source::PadButton(SDL_GAMEPAD_BUTTON_NORTH.0)
            ]
        );
        // The pad's confirm keeps a button of its own.
        let changes = request(&lua, r#"return { confirm = { pads = {} } }"#).unwrap();
        assert_eq!(changes.apply(&Bindings::default()), Err(Action::Confirm));
    }

    #[test]
    fn a_request_names_real_sources_on_their_own_device() {
        let lua = Lua::new();
        for bad in [
            r#"return { jump = { keys = {} } }"#,
            r#"return { fire = { keys = { "pad:a" } } }"#,
            r#"return { fire = { pads = { "key:K" } } }"#,
            r#"return { fire = { keys = { "key:F1" } } }"#,
            r#"return { fire = { keys = { "key:NoSuchKey" } } }"#,
        ] {
            assert!(request(&lua, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_screen_is_told_every_action_by_device() {
        let lua = Lua::new();
        let defaults = Bindings::default();
        let mut bindings = Bindings::default();
        bindings.set(
            Action::Aim,
            vec![Source::Mouse(3), Source::key(SDL_SCANCODE_K)],
        );
        let values = BindingsArg {
            bindings: &bindings,
            defaults: &defaults,
        }
        .into_lua_multi(&lua)
        .unwrap();
        let tables: Vec<Table> = values
            .into_iter()
            .map(|value| lua.unpack(value).unwrap())
            .collect();
        let aim: Table = tables[0].get("aim").unwrap();
        assert_eq!(aim.get::<Vec<String>>("keys").unwrap(), ["key:K"]);
        assert!(aim.get::<Vec<String>>("pads").unwrap().is_empty());
        let fire: Table = tables[1].get("fire").unwrap();
        assert_eq!(
            fire.get::<Vec<String>>("keys").unwrap(),
            ["key:F", "key:Return"]
        );
        assert_eq!(
            fire.get::<Vec<String>>("pads").unwrap(),
            ["pad:a", "axis:righttrigger+"]
        );
        for action in ACTIONS {
            assert!(tables[0].contains_key(action.name()).unwrap());
        }
    }

    #[test]
    fn a_ticks_presses_are_keys_and_buttons_in_order() {
        let pressed = [
            Some(Source::Mouse(1)),
            Some(Source::PadAxis(SDL_GAMEPAD_AXIS_RIGHT_TRIGGER.0, true)),
            Some(Source::key(SDL_SCANCODE_K)),
            Some(Source::key(SDL_SCANCODE_F1)),
            Some(Source::PadButton(SDL_GAMEPAD_BUTTON_START.0)),
            Some(Source::PadButton(SDL_GAMEPAD_BUTTON_SOUTH.0)),
            Some(Source::key(SDL_SCANCODE_I)),
            None,
        ];
        assert_eq!(
            pressed_names(&pressed),
            (
                vec!["key:K".to_string(), "key:I".to_string()],
                vec!["axis:righttrigger+".to_string(), "pad:a".to_string()]
            )
        );
    }

    #[test]
    fn two_requests_on_a_tick_merge_by_device() {
        let lua = Lua::new();
        let mut held = request(
            &lua,
            r#"return { fire = { keys = { "key:K" } }, up = { keys = { "key:W" } } }"#,
        )
        .unwrap();
        held.merge(
            request(
                &lua,
                r#"return { fire = { pads = { "pad:y" } }, up = { keys = { "key:Up" } } }"#,
            )
            .unwrap(),
        );
        let applied = held.apply(&Bindings::default()).unwrap();
        assert_eq!(
            applied.get(Action::Fire),
            [
                Source::key(SDL_SCANCODE_K),
                Source::Mouse(1),
                Source::PadButton(SDL_GAMEPAD_BUTTON_NORTH.0)
            ]
        );
        assert_eq!(applied.get(Action::Up)[0], Source::key(SDL_SCANCODE_UP));
    }
}
