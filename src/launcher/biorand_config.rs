//! The BioRand configuration screen's model: the package's option
//! definition turned into the rows the screen draws, a row's edit turned back
//! into a configuration value, and the saved configuration file.
//!
//! Nothing here names an option. The screen is generated from
//! [`Definition`]: one page per definition page, a heading per group, and per
//! item the control its [`Kind`] asks for (a switch is a toggle, a dropdown a
//! choice, a range or percent a slider), with the item's label and
//! description. A change to the definition (a new option, a new weapon in the
//! map) shows up without touching the launcher.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use arklay_biorand::config::{Config, Definition, Item, Kind, Value as Setting};

use crate::gui::dom::Value;

/// The file the configuration is saved in, beside `arklay.toml`: staging's
/// flat JSON object, so a file exported from staging loads here and this one
/// loads there.
pub fn config_path() -> PathBuf {
    crate::config::Config::dir().join("biorand.json")
}

/// The saved configuration; empty when there is none, or it can't be read
/// (the definition's defaults then apply, and the file is rewritten on the
/// next change).
pub fn load(path: &Path) -> Config {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| Config::from_json(&text).ok())
        .unwrap_or_default()
}

/// Write `config` to `path`.
pub fn save(path: &Path, config: &Config) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("failed to create {}", dir.display()))?;
    }
    let mut text = config.to_json();
    text.push('\n');
    std::fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))
}

/// The saved values over the definition's defaults: every option has a value.
pub fn complete(definition: &Definition, saved: &Config) -> Config {
    Config::defaults(definition).merge(saved)
}

/// One row of a page: a group heading, or an option.
pub struct Row<'a> {
    /// The option, `None` for a heading.
    pub item: Option<&'a Item>,
    /// The row's value for the UI store.
    pub ui: Value,
}

/// The titles of the definition's pages.
pub fn page_titles(definition: &Definition) -> Vec<&str> {
    definition
        .pages
        .iter()
        .map(|page| page.label.as_str())
        .collect()
}

/// The rows of page `page`, showing the values in `config`.
pub fn rows<'a>(definition: &'a Definition, page: usize, config: &Config) -> Vec<Row<'a>> {
    let Some(page) = definition.pages.get(page) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for group in &page.groups {
        if !group.label.is_empty() || group.warning.is_some() {
            rows.push(Row {
                item: None,
                ui: Value::map([
                    ("heading", Value::from(group.label.as_str())),
                    (
                        "warning",
                        Value::from(group.warning.as_deref().unwrap_or("")),
                    ),
                ]),
            });
        }
        for item in &group.items {
            rows.push(Row {
                item: Some(item),
                ui: item_row(item, config),
            });
        }
    }
    rows
}

/// The percentage a fraction is shown as.
fn percent(fraction: f64) -> i64 {
    (fraction * 100.0).round() as i64
}

/// The slider scale of a kind: `(min, max, step, suffix)`.
fn scale(kind: &Kind) -> Option<(i64, i64, i64, &'static str)> {
    match *kind {
        Kind::Range { min, max, step } => Some((
            min.round() as i64,
            max.round() as i64,
            step.map_or(1, |step| step.round() as i64).max(1),
            "",
        )),
        Kind::Percent { min, max, step } => Some((
            percent(min),
            percent(max),
            step.map_or(1, percent).max(1),
            "%",
        )),
        _ => None,
    }
}

/// The index or number the control shows for `setting`, in the units of
/// [`scale`] for sliders. An unknown dropdown choice (a hand-edited file)
/// shows the first.
fn shown(kind: &Kind, setting: Option<&Setting>) -> i64 {
    match kind {
        Kind::Switch => i64::from(setting.and_then(Setting::as_bool).unwrap_or(false)),
        Kind::Dropdown { options } => setting
            .map(Setting::as_text)
            .and_then(|text| options.iter().position(|option| *option == text))
            .unwrap_or(0) as i64,
        Kind::Range { .. } => setting
            .and_then(Setting::as_f64)
            .map_or(0, |value| value.round() as i64),
        Kind::Percent { .. } => setting.and_then(Setting::as_f64).map_or(0, percent),
    }
}

/// The control of `item`: which of `toggle`, `choice` and `slider` it is, and
/// its definition for the UI (label, description, value, default and the
/// choices or scale).
fn control_def(item: &Item, config: &Config) -> (&'static str, Value) {
    let label = match &item.category {
        Some(category) => format!("{} [{}]", item.label, category.label),
        None => item.label.clone(),
    };
    let mut def = vec![
        ("label", Value::from(label)),
        (
            "description",
            Value::from(item.description.as_deref().unwrap_or("")),
        ),
        ("value", Value::Int(shown(&item.kind, config.get(&item.id)))),
        (
            "default",
            Value::Int(shown(&item.kind, Some(&item.default))),
        ),
    ];
    let control = match &item.kind {
        Kind::Switch => "toggle",
        Kind::Dropdown { options } => {
            def.push((
                "choices",
                Value::list(options.iter().map(|o| Value::from(o.as_str())).collect()),
            ));
            "choice"
        }
        kind @ (Kind::Range { .. } | Kind::Percent { .. }) => {
            let (min, max, step, suffix) = scale(kind).expect("a slider kind has a scale");
            def.push(("min", Value::Int(min)));
            def.push(("max", Value::Int(max)));
            def.push(("step", Value::Int(step)));
            def.push(("suffix", Value::from(suffix)));
            "slider"
        }
    };
    (control, Value::map(def))
}

/// `item`'s definition for a bare control (see [`control_def`]).
pub fn control(item: &Item, config: &Config) -> Value {
    control_def(item, config).1
}

/// An item's row: only the control of its kind is set, which hides the
/// others.
fn item_row(item: &Item, config: &Config) -> Value {
    let (control, def) = control_def(item, config);
    let mut row = vec![
        ("heading", Value::Nil),
        ("toggle", Value::Nil),
        ("choice", Value::Nil),
        ("slider", Value::Nil),
    ];
    row.retain(|(key, _)| *key != control);
    row.push((control, def));
    Value::map(row)
}

/// A number as a setting: whole numbers are ints, as staging reads them.
fn number(value: f64) -> Setting {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        Setting::Int(value as i64)
    } else {
        Setting::Float(value)
    }
}

/// Store what the control of `item` was set to (its index, or its slider
/// number) into `config`.
pub fn set(item: &Item, shown: i64, config: &mut Config) {
    let value = match &item.kind {
        Kind::Switch => Setting::Bool(shown != 0),
        Kind::Dropdown { options } => {
            let Some(option) = usize::try_from(shown).ok().and_then(|i| options.get(i)) else {
                return;
            };
            Setting::Str(option.clone())
        }
        Kind::Range { .. } => Setting::Int(shown),
        // Round to the hundredth, so 30% is 0.3 and not 0.30000000000000004.
        Kind::Percent { .. } => number((shown as f64 / 100.0 * 1e6).round() / 1e6),
    };
    config.set(item.id.clone(), value);
}

/// The row a store path like `biorand_config.rows.7.slider.value` names.
pub fn changed_row(path: &str) -> Option<usize> {
    path.strip_prefix("biorand_config.rows.")?
        .split('.')
        .next()?
        .parse()
        .ok()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use arklay_biorand::config;
    use arklay_biorand::map::Map;

    /// The checked-in map's definition, as a base pack's would give.
    pub(crate) fn definition_for_tests() -> Definition {
        let mut rooms: Vec<(&str, &str)> = crate::room::RE1_FILES
            .iter()
            .filter_map(|(path, text)| path.strip_suffix("/random.xml").map(|dir| (dir, *text)))
            .collect();
        rooms.sort();
        let map = Map::parse(
            crate::biorand::BIORAND_FILES
                .iter()
                .find(|(path, _)| *path == "biorand/map.xml")
                .map(|(_, text)| *text)
                .unwrap(),
            rooms.iter().copied(),
        )
        .unwrap();
        config::definition(&map)
    }

    fn field<'a>(row: &'a Value, control: &str, key: &str) -> &'a Value {
        row.at(&format!("{control}.{key}")).unwrap()
    }

    #[test]
    fn every_item_of_the_definition_gets_a_row_with_its_control() {
        let definition = definition_for_tests();
        let config = Config::defaults(&definition);
        let mut items = 0;
        for page in 0..definition.pages.len() {
            for row in rows(&definition, page, &config) {
                let Some(item) = row.item else {
                    assert!(
                        row.ui.at("heading").unwrap().truthy() || row.ui.at("warning").is_some()
                    );
                    continue;
                };
                items += 1;
                let control = match item.kind {
                    Kind::Switch => "toggle",
                    Kind::Dropdown { .. } => "choice",
                    _ => "slider",
                };
                for other in ["toggle", "choice", "slider"] {
                    assert_eq!(row.ui.at(other).unwrap().truthy(), other == control);
                }
                assert_eq!(
                    field(&row.ui, control, "label").to_string(),
                    match &item.category {
                        Some(category) => format!("{} [{}]", item.label, category.label),
                        None => item.label.clone(),
                    }
                );
                // A default config shows every item at its default.
                assert_eq!(
                    field(&row.ui, control, "value"),
                    field(&row.ui, control, "default"),
                    "{}",
                    item.id
                );
            }
        }
        assert_eq!(items, definition.items().count());
    }

    #[test]
    fn dropdowns_keep_the_default_and_random_choices() {
        let definition = definition_for_tests();
        let player = definition.items().find(|item| item.id == "player").unwrap();
        let config = Config::defaults(&definition);
        let row = item_row(player, &config);
        let choices: Vec<String> = field(&row, "choice", "choices")
            .as_list()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(choices, ["Chris", "Jill", "Random"]);
        assert_eq!(field(&row, "choice", "value"), &Value::Int(2));
    }

    #[test]
    fn edits_round_trip_through_the_config() {
        let definition = definition_for_tests();
        let mut config = Config::defaults(&definition);
        let by_id = |id: &str| definition.items().find(|item| item.id == id).unwrap();

        set(by_id("player"), 1, &mut config);
        assert_eq!(config.get_str("player", ""), "Jill");
        set(by_id("hard"), 1, &mut config);
        assert!(config.get_bool("hard", false));
        set(by_id("doors/segments"), 5, &mut config);
        assert_eq!(config.get_int("doors/segments", 0), 5);
        // A percentage is a clean fraction, and 100% a whole number.
        set(by_id("locks/ratio"), 30, &mut config);
        assert_eq!(config.get("locks/ratio"), Some(&Setting::Float(0.3)));
        set(by_id("inventory/primary/ammo/min"), 100, &mut config);
        assert_eq!(
            config.get("inventory/primary/ammo/min"),
            Some(&Setting::Int(1))
        );

        for id in ["player", "hard", "doors/segments", "locks/ratio"] {
            let item = by_id(id);
            let row = item_row(item, &config);
            let control = match item.kind {
                Kind::Switch => "toggle",
                Kind::Dropdown { .. } => "choice",
                _ => "slider",
            };
            let mut again = Config::new();
            set(
                item,
                field(&row, control, "value").as_int().unwrap(),
                &mut again,
            );
            assert_eq!(again.get(id), config.get(id), "{id}");
        }
        // An index out of range changes nothing.
        set(by_id("player"), 9, &mut config);
        assert_eq!(config.get_str("player", ""), "Jill");
    }

    #[test]
    fn a_saved_config_loads_over_the_defaults_and_keeps_unknown_keys() {
        let definition = definition_for_tests();
        let dir = std::env::temp_dir().join(format!("arklay-biorand-cfg-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("biorand.json");

        assert_eq!(load(&path), Config::new());
        let mut config = complete(&definition, &load(&path));
        assert_eq!(config, Config::defaults(&definition));
        set(
            definition.items().find(|item| item.id == "hard").unwrap(),
            1,
            &mut config,
        );
        config.set("title/call/random", true);
        save(&path, &config).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded, config);
        assert_eq!(complete(&definition, &loaded), config);

        // A partial file, as staging exports, fills in the rest.
        std::fs::write(&path, r#"{"hard": true, "player": "Chris"}"#).unwrap();
        let partial = complete(&definition, &load(&path));
        assert!(partial.get_bool("hard", false));
        assert_eq!(partial.get_str("player", ""), "Chris");
        assert_eq!(
            partial.get("doors/random"),
            Config::defaults(&definition).get("doors/random")
        );

        // A broken file is ignored.
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(load(&path), Config::new());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_changed_row_is_named_by_its_store_path() {
        assert_eq!(changed_row("biorand_config.rows.12.slider.value"), Some(12));
        assert_eq!(changed_row("update.channel.value"), None);
    }
}
