//! Lua scripting for screens.
//!
//! A screen whose root names a `script` runs it in its own sandboxed state
//! when it opens (a new state per open, so nothing leaks between opens).
//! The chunk defines `on_open(ui)`; inside it, and inside every handler it
//! registers, `ui` is the live UI:
//!
//! | method | does |
//! |---|---|
//! | `ui:get(path)` / `ui:set(path, v)` | read / publish a store value |
//! | `ui:find(selector)` | the first node matching `#id` or an element name |
//! | `ui:create(xml)` | build a detached node from markup |
//! | `ui:open(name)` / `ui:close()` | open a screen on top / close this one |
//! | `ui:emit(name, arg)` | send an action to the host |
//! | `ui:on(event, fn)` | call `fn(ui, arg)` for `action:<name>` |
//! | `node:append(child)` / `node:remove()` / `node:clear()` | edit the tree |
//! | `node:get(name)` / `node:set(name, text)` | read a property / set an attribute (markup syntax) |
//! | `node:focus()` | move the focus to it |
//!
//! An error in a script is logged once and closes its screen.

use std::rc::Rc;

use anyhow::{Result, anyhow};
use mlua::{AnyUserData, Function, Lua, ObjectLike, Table, UserData, UserDataMethods};

use super::markup::{Element, Expr};
use super::tree::{Builder, NodeId};
use super::value::Value;
use super::{Ui, UiOutput};
use crate::lua::{Budget, new_sandboxed_state};

/// One open screen's Lua state.
pub struct Script {
    lua: Lua,
    budget: Rc<Budget>,
}

const UI_KEY: &str = "arklay.ui";
const UID_KEY: &str = "arklay.ui.screen";
const HANDLERS_KEY: &str = "arklay.ui.handlers";

impl Script {
    /// Run the chunk `source` in a new sandbox.
    pub fn load(name: &str, source: &str) -> Result<Self> {
        let (lua, budget) = new_sandboxed_state()?;
        budget.begin();
        lua.load(source)
            .set_name(name)
            .exec()
            .map_err(|err| anyhow!("{name}: {err}"))?;
        let handlers = lua.create_table().map_err(lua_err)?;
        lua.set_named_registry_value(HANDLERS_KEY, handlers)
            .map_err(lua_err)?;
        Ok(Self { lua, budget })
    }
}

fn lua_err(err: mlua::Error) -> anyhow::Error {
    anyhow!("{err}")
}

/// Call screen `uid`'s handler for `event` (`open` calls `on_open`);
/// returns whether there was one. A failing script closes its screen.
pub(super) fn call(ui: &mut Ui, uid: u64, event: &str, arg: Value) -> bool {
    let Some(s) = ui.index_of(uid) else {
        return false;
    };
    let Some(script) = ui.screens[s].script.take() else {
        return false;
    };
    let name = ui.screens[s].name.clone();
    let result = run(&script, ui, uid, event, arg);
    if let Some(s) = ui.index_of(uid) {
        ui.screens[s].script = Some(script);
    }
    ui.touch();
    match result {
        Ok(handled) => handled,
        Err(err) => {
            eprintln!("[ui] script for {name} failed: {err}");
            if let Some(s) = ui.index_of(uid) {
                ui.screens.truncate(s);
                ui.outputs.push(UiOutput::Closed(name));
            }
            true
        }
    }
}

fn run(script: &Script, ui: &mut Ui, uid: u64, event: &str, arg: Value) -> mlua::Result<bool> {
    let lua = &script.lua;
    let function: Option<Function> = if event == "open" {
        lua.globals().get("on_open")?
    } else {
        let handlers: Table = lua.named_registry_value(HANDLERS_KEY)?;
        handlers.get(event)?
    };
    let Some(function) = function else {
        return Ok(false);
    };
    script.budget.begin();
    lua.set_named_registry_value(UID_KEY, uid as i64)?;
    let arg = to_lua(lua, &arg)?;
    let result = lua.scope(|scope| {
        let handle = scope.create_userdata_ref_mut(ui)?;
        lua.set_named_registry_value(UI_KEY, &handle)?;
        function.call::<()>((handle, arg))
    });
    lua.unset_named_registry_value(UI_KEY)?;
    result.map(|()| true)
}

/// The screen a call runs for.
fn screen_of(lua: &Lua, ui: &Ui) -> mlua::Result<usize> {
    let uid: i64 = lua.named_registry_value(UID_KEY)?;
    ui.index_of(uid as u64)
        .ok_or_else(|| mlua::Error::runtime("the screen has closed"))
}

/// A node of the calling screen. Its methods forward to the `ui` handle
/// (the only way to reach the borrowed UI), as `ui:_node(op, id, ...)`.
#[derive(Clone, Copy)]
struct NodeRef(NodeId);

impl UserData for NodeRef {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        for op in ["append", "remove", "clear", "get", "set", "focus"] {
            methods.add_method(op, move |lua, this, args: mlua::MultiValue| {
                let handle: AnyUserData = lua.named_registry_value(UI_KEY)?;
                handle.call_method::<mlua::Value>("_node", (op, this.0.to_bits(), args))
            });
        }
    }
}

/// `ui:_node(op, id, ...)`: a node method on the borrowed UI.
fn node_op(
    lua: &Lua,
    ui: &mut Ui,
    op: &str,
    node: NodeId,
    mut args: mlua::MultiValue,
) -> mlua::Result<mlua::Value> {
    let s = screen_of(lua, ui)?;
    if !ui.screens[s].tree.contains(node) {
        return Err(mlua::Error::runtime("the node was removed"));
    }
    ui.touch();
    let mut arg = || args.pop_front().unwrap_or(mlua::Value::Nil);
    match op {
        "append" => {
            let mlua::Value::UserData(child) = arg() else {
                return Err(mlua::Error::runtime("append takes a node"));
            };
            let child = child.borrow::<NodeRef>()?.0;
            let tree = &mut ui.screens[s].tree;
            if !tree.contains(child) || tree.ancestry(node).contains(&child) {
                return Err(mlua::Error::runtime("cannot append that node here"));
            }
            tree.append(node, child);
        }
        "remove" => {
            if node == ui.screens[s].root {
                return Err(mlua::Error::runtime("cannot remove the screen's root"));
            }
            ui.screens[s].tree.remove(node);
        }
        "clear" => {
            let tree = &mut ui.screens[s].tree;
            for child in tree[node].children.clone() {
                tree.remove(child);
            }
        }
        "get" => {
            let name = lua_text(&arg())?;
            ui.evaluate(s, false);
            let value = ui.screens[s].tree[node].prop(&name).clone();
            return to_lua(lua, &value);
        }
        "set" => {
            let name = lua_text(&arg())?;
            let text = match arg() {
                mlua::Value::Nil => None,
                other => Some(lua_text(&other)?),
            };
            let node = &mut ui.screens[s].tree[node];
            match text {
                Some(text) => node.set_attr(&name, Expr::parse(&text)),
                None => node.attrs.retain(|(key, _)| *key != name),
            }
        }
        "focus" => ui.screens[s].focus = Some(node),
        _ => return Err(mlua::Error::runtime(format!("no node method {op}"))),
    }
    Ok(mlua::Value::Nil)
}

fn lua_text(value: &mlua::Value) -> mlua::Result<String> {
    Ok(match value {
        mlua::Value::String(s) => s.to_str()?.to_string(),
        mlua::Value::Integer(i) => i.to_string(),
        mlua::Value::Number(n) => n.to_string(),
        mlua::Value::Boolean(b) => b.to_string(),
        _ => return Err(mlua::Error::runtime("expected text")),
    })
}

impl UserData for Ui {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("get", |lua, ui, path: String| to_lua(lua, &ui.get(&path)));
        methods.add_method_mut("set", |_, ui, (path, value): (String, mlua::Value)| {
            ui.set(&path, from_lua(value, 0)?);
            Ok(())
        });
        methods.add_method_mut("find", |lua, ui, selector: String| {
            let s = screen_of(lua, ui)?;
            let screen = &ui.screens[s];
            Ok(screen
                .tree
                .select(screen.root, &selector)
                .first()
                .map(|&id| NodeRef(id)))
        });
        methods.add_method_mut("create", |lua, ui, source: String| {
            let s = screen_of(lua, ui)?;
            let el = Element::parse(&source).map_err(|e| mlua::Error::runtime(format!("{e:#}")))?;
            let names: Vec<String> = ui.customs.keys().cloned().collect();
            let is_custom = |n: &str| names.iter().any(|c| c == n);
            let builder = Builder {
                library: &ui.library,
                customs: &is_custom,
            };
            let id = builder
                .build(&mut ui.screens[s].tree, &el, None)
                .map_err(|e| mlua::Error::runtime(format!("{e:#}")))?;
            Ok(NodeRef(id))
        });
        methods.add_method_mut("open", |_, ui, name: String| {
            ui.open(&name)
                .map_err(|e| mlua::Error::runtime(format!("{e:#}")))
        });
        methods.add_method_mut("close", |lua, ui, ()| {
            let s = screen_of(lua, ui)?;
            ui.screens.truncate(s + 1);
            ui.close();
            Ok(())
        });
        methods.add_method_mut("emit", |lua, ui, (name, arg): (String, mlua::Value)| {
            let s = screen_of(lua, ui)?;
            let arg = from_lua(arg, 0)?;
            ui.outputs.push(UiOutput::Action {
                screen: ui.screens[s].name.clone(),
                name,
                arg,
            });
            Ok(())
        });
        methods.add_method_mut(
            "_node",
            |lua, ui, (op, id, args): (String, i64, mlua::MultiValue)| {
                node_op(lua, ui, &op, NodeId::from_bits(id), args)
            },
        );
        methods.add_method("on", |lua, _, (event, function): (String, Function)| {
            let handlers: Table = lua.named_registry_value(HANDLERS_KEY)?;
            handlers.set(event, function)
        });
    }
}

fn to_lua(lua: &Lua, value: &Value) -> mlua::Result<mlua::Value> {
    Ok(match value {
        Value::Nil | Value::Image(_) => mlua::Value::Nil,
        Value::Bool(b) => mlua::Value::Boolean(*b),
        Value::Int(i) => mlua::Value::Integer(*i),
        Value::Str(s) => mlua::Value::String(lua.create_string(&**s)?),
        Value::List(items) => {
            let table = lua.create_table()?;
            for (i, item) in items.iter().enumerate() {
                table.raw_set(i + 1, to_lua(lua, item)?)?;
            }
            mlua::Value::Table(table)
        }
        Value::Map(fields) => {
            let table = lua.create_table()?;
            for (key, item) in fields.iter() {
                table.raw_set(key.as_str(), to_lua(lua, item)?)?;
            }
            mlua::Value::Table(table)
        }
    })
}

fn from_lua(value: mlua::Value, depth: usize) -> mlua::Result<Value> {
    if depth > 16 {
        return Err(mlua::Error::runtime("the value nests too deeply"));
    }
    Ok(match value {
        mlua::Value::Nil => Value::Nil,
        mlua::Value::Boolean(b) => Value::Bool(b),
        mlua::Value::Integer(i) => Value::Int(i),
        mlua::Value::Number(n) => Value::Int(n.round() as i64),
        mlua::Value::String(s) => Value::from(s.to_str()?.to_string()),
        mlua::Value::Table(table) => {
            let len = table.raw_len();
            if len > 0 {
                let mut items = Vec::with_capacity(len);
                for i in 1..=len {
                    items.push(from_lua(table.raw_get(i)?, depth + 1)?);
                }
                Value::list(items)
            } else {
                let mut fields = Vec::new();
                for pair in table.pairs::<String, mlua::Value>() {
                    let (key, item) = pair?;
                    fields.push((key, from_lua(item, depth + 1)?));
                }
                Value::map(fields)
            }
        }
        _ => return Err(mlua::Error::runtime("that value cannot be stored")),
    })
}
