use super::*;
use crate::gui::input::Device;

fn key(k: GuiKey) -> GuiEvent {
    GuiEvent::Key(k, Device::Keyboard)
}

fn library(extra: &[(&str, &str)]) -> Library {
    let mut library = Library::builtin();
    for (name, source) in extra {
        library.add(name, source).unwrap();
    }
    library
}

fn option(label: &str, choices: &[&str], value: i64, default: i64) -> Value {
    Value::map([
        ("label", Value::from(label)),
        ("description", Value::from(format!("{label} does things."))),
        (
            "choices",
            Value::list(choices.iter().map(|c| Value::from(*c)).collect()),
        ),
        ("value", Value::Int(value)),
        ("default", Value::Int(default)),
    ])
}

#[test]
fn every_builtin_screen_builds() {
    let mut ui = Ui::new(Library::builtin()).unwrap();
    let names: Vec<String> = ui.library.screen_names().map(str::to_string).collect();
    assert!(names.len() >= 10, "{names:?}");
    for name in names {
        ui.clear();
        ui.open(&name)
            .unwrap_or_else(|err| panic!("{name}: {err:#}"));
        ui.draw(1280, 720);
    }
}

#[test]
fn keys_move_the_focus_and_change_values() {
    let screen = r#"<screen use="standard-screen" title="T">
        <list>
          <choice def="{a}"/>
          <choice def="{b}"/>
        </list>
      </screen>"#;
    let mut ui = Ui::new(library(&[("test", screen)])).unwrap();
    ui.set("a", option("A", &["x", "y", "z"], 0, 0));
    ui.set("b", option("B", &["p", "q"], 1, 1));
    ui.open("test").unwrap();
    assert_eq!(ui.focus_prop("label"), Value::from("A"));
    let out = ui.handle(&[key(GuiKey::Right)]);
    assert!(out.contains(&UiOutput::Changed {
        path: "a.value".into(),
        value: Value::Int(1),
    }));
    assert_eq!(ui.get("a.value"), Value::Int(1));
    // An end stop is quiet and changes nothing.
    ui.handle(&[key(GuiKey::Right)]);
    let out = ui.handle(&[key(GuiKey::Right)]);
    assert!(out.is_empty(), "{out:?}");
    ui.handle(&[key(GuiKey::Down)]);
    assert_eq!(ui.focus_prop("label"), Value::from("B"));
    // The list wraps.
    ui.handle(&[key(GuiKey::Down)]);
    assert_eq!(ui.focus_prop("label"), Value::from("A"));
    // Confirm on a choice wraps its value.
    ui.handle(&[key(GuiKey::Confirm)]);
    assert_eq!(ui.get("a.value"), Value::Int(0));
    // The hints follow the focus path: change, then back.
    let hints: Vec<GuiKey> = ui.shown_hints().into_iter().map(|(k, _)| k).collect();
    assert_eq!(hints, [GuiKey::Left, GuiKey::Cancel]);
    // Cancel goes back.
    let out = ui.handle(&[key(GuiKey::Cancel)]);
    assert!(out.contains(&UiOutput::Closed("test".into())));
    assert!(ui.is_empty());
}

#[test]
fn confirmed_actions_ask_first() {
    let screen = r#"<screen use="standard-screen" title="T">
        <list>
          <item text="Go" action="go:3" confirm="Sure?"/>
        </list>
      </screen>"#;
    let mut ui = Ui::new(library(&[("test", screen)])).unwrap();
    ui.open("test").unwrap();
    ui.handle(&[key(GuiKey::Confirm)]);
    assert_eq!(ui.top(), Some("dialog"));
    assert_eq!(ui.get("dialog.title"), Value::from("Sure?"));
    // No is the default.
    let out = ui.handle(&[key(GuiKey::Confirm)]);
    assert_eq!(ui.top(), Some("test"));
    assert!(out.iter().all(|o| o.action().is_none()), "{out:?}");
    ui.handle(&[key(GuiKey::Confirm)]);
    let out = ui.handle(&[key(GuiKey::Up), key(GuiKey::Confirm)]);
    assert_eq!(ui.top(), Some("test"));
    let action = out.iter().find_map(UiOutput::action);
    assert_eq!(action, Some(("go", &Value::Int(3))));
}

#[test]
fn tab_keys_come_from_the_nesting_depth() {
    let screen = r#"<screen use="standard-screen" title="T">
        <tabs>
          <tab label="One">
            <tabs>
              <tab label="A"><list><item text="a1" action="x"/></list></tab>
              <tab label="B"><list><item text="b1" action="x"/></list></tab>
            </tabs>
          </tab>
          <tab label="Two"><list><item text="two" action="x"/></list></tab>
        </tabs>
      </screen>"#;
    let mut ui = Ui::new(library(&[("test", screen)])).unwrap();
    ui.open("test").unwrap();
    assert_eq!(ui.tab_indices(), [0, 0]);
    ui.handle(&[key(GuiKey::SubNext)]);
    assert_eq!(ui.tab_indices(), [0, 1]);
    assert_eq!(ui.focus_prop("text"), Value::from("b1"));
    ui.handle(&[key(GuiKey::TabNext)]);
    assert_eq!(ui.tab_indices(), [1]);
    assert_eq!(ui.focus_prop("text"), Value::from("two"));
    // Coming back finds the focus where it was left.
    ui.handle(&[key(GuiKey::TabPrev)]);
    assert_eq!(ui.tab_indices(), [0, 1]);
    assert_eq!(ui.focus_prop("text"), Value::from("b1"));
}

#[test]
fn reset_restores_the_defaults_of_a_page_or_everything() {
    let screen = r#"<screen use="standard-screen" title="T">
        <command key="reset_all" label="All" action="reset"/>
        <tabs>
          <command key="reset_group" label="Page" action="reset"/>
          <tab label="One"><list><choice def="{a}"/></list></tab>
          <tab label="Two"><list><choice def="{b}"/></list></tab>
        </tabs>
      </screen>"#;
    let mut ui = Ui::new(library(&[("test", screen)])).unwrap();
    ui.set("a", option("A", &["x", "y"], 1, 0));
    ui.set("b", option("B", &["x", "y"], 1, 0));
    ui.open("test").unwrap();
    ui.handle(&[key(GuiKey::ResetGroup)]);
    assert_eq!(ui.get("a.value"), Value::Int(0));
    assert_eq!(ui.get("b.value"), Value::Int(1), "only the shown page");
    ui.handle(&[key(GuiKey::ResetAll)]);
    assert_eq!(ui.get("b.value"), Value::Int(0), "and the hidden ones");
}

#[test]
fn templates_repeat_for_their_items_and_keep_the_focus() {
    let screen = r#"<screen use="standard-screen" title="T">
        <list items="{rows}">
          <template><choice def="{item}"/></template>
        </list>
      </screen>"#;
    let mut ui = Ui::new(library(&[("test", screen)])).unwrap();
    let rows = |v: i64| {
        Value::list(vec![
            option("R0", &["off", "on"], 0, 0),
            option("R1", &["off", "on"], v, 0),
        ])
    };
    ui.set("rows", rows(0));
    ui.open("test").unwrap();
    ui.handle(&[key(GuiKey::Down)]);
    let out = ui.handle(&[key(GuiKey::Right)]);
    assert!(out.contains(&UiOutput::Changed {
        path: "rows.1.value".into(),
        value: Value::Int(1),
    }));
    // The host republishes; the focus stays on the row.
    ui.set("rows", rows(1));
    ui.handle(&[]);
    assert_eq!(ui.focus_prop("label"), Value::from("R1"));
    assert_eq!(ui.focus_prop("value"), Value::Int(1));
}

#[test]
fn the_pointer_hovers_and_clicks_rows_and_hints() {
    let screen = r#"<screen use="standard-screen" title="T">
        <list>
          <item text="A" action="a"/>
          <item text="B" action="b"/>
        </list>
      </screen>"#;
    let mut ui = Ui::new(library(&[("test", screen)])).unwrap();
    ui.open("test").unwrap();
    ui.draw(1920, 1080);
    let rect = ui
        .with_tree(|tree, _| {
            let id = tree.select(tree.root.unwrap(), "item")[1];
            tree[id].rect
        })
        .unwrap();
    let out = ui.handle(&[GuiEvent::PointerMove {
        x: rect.center_x(),
        y: rect.center_y(),
    }]);
    assert!(out.is_empty(), "hovering is silent: {out:?}");
    assert_eq!(ui.focus_prop("text"), Value::from("B"));
    let out = ui.handle(&[GuiEvent::PointerDown {
        x: rect.center_x(),
        y: rect.center_y(),
    }]);
    assert_eq!(
        out.iter().find_map(UiOutput::action),
        Some(("b", &Value::Nil))
    );
    // Clicking the Back hint presses Cancel.
    ui.draw(1920, 1080);
    let back = ui
        .with_tree(|tree, _| {
            let bar = tree.select(tree.root.unwrap(), "hint-bar")[0];
            tree[bar].parts.last().unwrap().0
        })
        .unwrap();
    let out = ui.handle(&[GuiEvent::PointerDown {
        x: back.center_x(),
        y: back.center_y(),
    }]);
    assert!(out.contains(&UiOutput::Closed("test".into())), "{out:?}");
}

#[test]
fn lua_scripts_build_and_drive_a_screen() {
    let screen = r#"<screen use="standard-screen" title="Lua" script="test.lua">
        <list id="rows"/>
      </screen>"#;
    let script = r##"
        function on_open(ui)
          local rows = ui:find("#rows")
          for i = 1, 3 do
            local row = ui:create('<item action="pick:' .. i .. '"/>')
            row:set("text", "Row " .. i)
            rows:append(row)
          end
          ui:set("picked", 0)
          ui:on("action:pick", function(ui, n)
            ui:set("picked", n)
            if n == 3 then ui:emit("done", ui:get("picked")) end
          end)
        end
    "##;
    let mut library = library(&[("test", screen)]);
    library.add_script("test.lua", script);
    let mut ui = Ui::new(library).unwrap();
    ui.open("test").unwrap();
    assert_eq!(ui.focus_prop("text"), Value::from("Row 1"));
    ui.handle(&[key(GuiKey::Down), key(GuiKey::Confirm)]);
    assert_eq!(ui.get("picked"), Value::Int(2));
    let out = ui.handle(&[key(GuiKey::Down), key(GuiKey::Confirm)]);
    assert_eq!(
        out.iter().find_map(UiOutput::action),
        Some(("done", &Value::Int(3)))
    );
}

#[test]
fn a_failing_script_closes_its_screen() {
    let screen = r#"<screen use="standard-screen" title="Lua" script="bad.lua"/>"#;
    let mut library = library(&[("test", screen)]);
    library.add_script("bad.lua", "function on_open(ui) error('boom') end");
    let mut ui = Ui::new(library).unwrap();
    ui.open("test").unwrap();
    assert!(ui.is_empty());
}

#[test]
fn breadcrumbs_and_modals_draw_over_the_screen_below() {
    let mut ui = Ui::new(Library::builtin()).unwrap();
    ui.open("menu").unwrap();
    ui.open("options").unwrap();
    assert!(ui.draw(640, 360));
    assert!(!ui.draw(640, 360), "nothing changed");
    ui.handle(&[key(GuiKey::ResetAll)]);
    assert_eq!(ui.screens(), ["menu", "options", "dialog"]);
    assert!(ui.draw(640, 360));
}
