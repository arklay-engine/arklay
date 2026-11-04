//! The BioRand configuration screen's model: the package's option
//! definition turned into the rows the screen draws, a row's edit turned back
//! into a configuration value, and the saved configuration file.
//!
//! Nothing here names an option. The screen is generated from
//! [`Definition`]: a tab per definition page, a group per titled group, and per
//! item the control its [`Kind`] asks for (a switch is a toggle, a dropdown a
//! choice, a range or percent a slider), with the item's label and
//! description. A change to the definition (a new option, a new weapon in the
//! map) shows up without touching the launcher.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use arklay_biorand::config::{Config, Definition, Item, Kind, Page, Value as Setting};

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

/// One entry of a page's list: a titled group of options, or what an
/// untitled group puts straight into the list.
pub enum Entry<'a> {
    /// A titled group, with its warning, which shows above its options.
    Group {
        label: &'a str,
        warning: Option<&'a str>,
        items: &'a [Item],
    },
    /// The warning of an untitled group.
    Warning(&'a str),
    /// An option of an untitled group.
    Item(&'a Item),
}

/// The entries of `page`, in order.
pub fn entries(page: &Page) -> Vec<Entry<'_>> {
    let mut entries = Vec::new();
    for group in &page.groups {
        if group.label.is_empty() {
            if let Some(warning) = group.warning.as_deref() {
                entries.push(Entry::Warning(warning));
            }
            entries.extend(group.items.iter().map(Entry::Item));
        } else {
            entries.push(Entry::Group {
                label: &group.label,
                warning: group.warning.as_deref(),
                items: &group.items,
            });
        }
    }
    entries
}

/// The pages for the UI store: a tab each, whose list holds the page's
/// [`entries`]. An entry sets `group` and `rows` for a group, `warning` for a
/// warning, or the control of an option (`toggle`, `choice` or `slider`); the
/// markup hides what an entry leaves nil or empty.
pub fn pages(definition: &Definition, config: &Config) -> Value {
    Value::list(
        definition
            .pages
            .iter()
            .map(|page| {
                Value::map([
                    ("label", Value::from(page.label.as_str())),
                    (
                        "entries",
                        Value::list(
                            entries(page)
                                .iter()
                                .map(|entry| entry_ui(entry, config))
                                .collect(),
                        ),
                    ),
                ])
            })
            .collect(),
    )
}

/// A row of the UI store: a group's, a warning's or an option's, with every
/// field the markup reads.
fn row(
    group: Option<(&str, Vec<Value>)>,
    warning: &str,
    control: Option<(&'static str, Value)>,
) -> Value {
    let (label, rows) = group.unzip();
    let mut fields = vec![
        ("group", Value::from(label.is_some())),
        ("label", Value::from(label.unwrap_or(""))),
        ("rows", Value::list(rows.unwrap_or_default())),
        ("warning", Value::from(warning)),
        ("toggle", Value::Nil),
        ("choice", Value::Nil),
        ("slider", Value::Nil),
    ];
    if let Some((name, def)) = control {
        fields.retain(|(key, _)| *key != name);
        fields.push((name, def));
    }
    Value::map(fields)
}

/// An entry's row.
fn entry_ui(entry: &Entry<'_>, config: &Config) -> Value {
    match entry {
        Entry::Group {
            label,
            warning,
            items,
        } => {
            let mut rows: Vec<Value> = warning.iter().map(|w| row(None, w, None)).collect();
            rows.extend(items.iter().map(|item| item_row(item, config)));
            row(Some((*label, rows)), "", None)
        }
        Entry::Warning(warning) => row(None, warning, None),
        Entry::Item(item) => item_row(item, config),
    }
}

/// The option a changed store path names, such as
/// `biorand_config.pages.1.entries.3.rows.2.choice.value` (the third row of
/// a group; a group's warning is a row too) or
/// `biorand_config.pages.0.entries.0.toggle.value` (an untitled group's).
pub fn changed_item<'a>(definition: &'a Definition, path: &str) -> Option<&'a Item> {
    let mut parts = path.strip_prefix("biorand_config.pages.")?.split('.');
    let page = definition.pages.get(parts.next()?.parse::<usize>().ok()?)?;
    if parts.next()? != "entries" {
        return None;
    }
    let entry = entries(page).into_iter().nth(parts.next()?.parse().ok()?)?;
    match entry {
        Entry::Item(item) => Some(item),
        Entry::Group { warning, items, .. } => {
            if parts.next()? != "rows" {
                return None;
            }
            let row: usize = parts.next()?.parse().ok()?;
            items.get(row.checked_sub(usize::from(warning.is_some()))?)
        }
        Entry::Warning(_) => None,
    }
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
    row(None, "", Some(control_def(item, config)))
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

    /// The row of every option in `pages`, with the store path it is at.
    fn option_rows(pages: &Value) -> Vec<(String, &Value)> {
        let mut found = Vec::new();
        for (p, page) in pages.as_list().iter().enumerate() {
            for (e, entry) in page.at("entries").unwrap().as_list().iter().enumerate() {
                let base = format!("biorand_config.pages.{p}.entries.{e}");
                if entry.at("group").unwrap().truthy() {
                    for (r, row) in entry.at("rows").unwrap().as_list().iter().enumerate() {
                        found.push((format!("{base}.rows.{r}"), row));
                    }
                } else {
                    found.push((base, entry));
                }
            }
        }
        found
            .into_iter()
            .filter(|(_, row)| {
                ["toggle", "choice", "slider"]
                    .iter()
                    .any(|control| row.at(control).unwrap().truthy())
            })
            .collect()
    }

    #[test]
    fn every_item_of_the_definition_gets_a_row_with_its_control() {
        let definition = definition_for_tests();
        let config = Config::defaults(&definition);
        let pages = pages(&definition, &config);
        assert_eq!(pages.as_list().len(), definition.pages.len());
        let rows = option_rows(&pages);
        assert_eq!(rows.len(), definition.items().count());
        for (path, ui) in rows {
            let item = changed_item(&definition, &format!("{path}.toggle.value"))
                .unwrap_or_else(|| panic!("{path} names no option"));
            let control = match item.kind {
                Kind::Switch => "toggle",
                Kind::Dropdown { .. } => "choice",
                _ => "slider",
            };
            for other in ["toggle", "choice", "slider"] {
                assert_eq!(ui.at(other).unwrap().truthy(), other == control, "{path}");
            }
            assert_eq!(
                field(ui, control, "label").to_string(),
                match &item.category {
                    Some(category) => format!("{} [{}]", item.label, category.label),
                    None => item.label.clone(),
                }
            );
            // A default config shows every item at its default.
            assert_eq!(
                field(ui, control, "value"),
                field(ui, control, "default"),
                "{}",
                item.id
            );
        }
    }

    #[test]
    fn titled_groups_are_native_groups_and_untitled_ones_are_bare_rows() {
        let definition = definition_for_tests();
        let pages = pages(&definition, &Config::defaults(&definition));
        for (page, ui) in definition.pages.iter().zip(pages.as_list().iter()) {
            let entries = ui.at("entries").unwrap().as_list();
            let titled = page.groups.iter().filter(|g| !g.label.is_empty());
            let untitled: usize = page
                .groups
                .iter()
                .filter(|g| g.label.is_empty())
                .map(|g| g.items.len() + usize::from(g.warning.is_some()))
                .sum();
            assert_eq!(entries.len(), titled.clone().count() + untitled);
            let groups: Vec<_> = entries
                .iter()
                .filter(|entry| entry.at("group").unwrap().truthy())
                .collect();
            for (group, ui) in titled.zip(groups) {
                assert_eq!(ui.at("label").unwrap().to_string(), group.label);
                // A warning is the group's first row, then its options.
                let rows = ui.at("rows").unwrap().as_list();
                assert_eq!(
                    rows.len(),
                    group.items.len() + usize::from(group.warning.is_some())
                );
                if let Some(warning) = &group.warning {
                    assert_eq!(rows[0].at("warning").unwrap().to_string(), *warning);
                }
            }
        }
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
    fn a_changed_path_names_its_option() {
        let definition = definition_for_tests();
        let config = Config::defaults(&definition);
        let pages = pages(&definition, &config);
        // Every row maps back to the option it shows, in a group (after any
        // warning) or bare.
        for (path, ui) in option_rows(&pages) {
            let control = ["toggle", "choice", "slider"]
                .into_iter()
                .find(|control| ui.at(control).unwrap().truthy())
                .unwrap();
            let item = changed_item(&definition, &format!("{path}.{control}.value")).unwrap();
            let label = field(ui, control, "label").to_string();
            assert!(label.starts_with(&item.label), "{path}: {label}");
        }
        assert!(changed_item(&definition, "update.channel.value").is_none());
        assert!(changed_item(&definition, "biorand_config.pages.99.entries.0").is_none());
        assert!(changed_item(&definition, "biorand_config.pages.0.entries.99").is_none());
    }
}
