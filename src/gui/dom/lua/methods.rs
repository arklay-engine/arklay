//! The Lua method registrations, split out of the parent module (#266).
//! mlua turns each registered closure into generic glue that is costly to
//! compile, and moving these closures' spans recompiles all of it. In their
//! own file, edits to the parent module no longer move them.

use super::*;

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
