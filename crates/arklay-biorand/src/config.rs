//! BioRand's configuration: the option definition the launcher draws, the
//! values a player picks, and how the generator resolves them.
//!
//! This ports staging's `RandomizerConfigurationDefinition` and
//! `RandomizerConfiguration` (`biorand-common`), with RE1's options from
//! `ClassicRandomizer.CreateConfigDefinition`,
//! `Re1ClassicRandomizerController.UpdateConfigDefinition` and
//! `ApplyConfigModifications`.
//!
//! # The definition
//!
//! [`definition`] builds the pages, groups and items in staging's order. Each
//! item keeps staging's id, which is its key in a saved configuration, so a
//! configuration saved by staging loads unchanged.
//!
//! Left out, because the plan puts the cosmetic options out of scope for now:
//!
//! - the title call, `title/call/random`;
//! - the enemy skins, `enemies/skin/*`, and their Skins group;
//! - the Protagonist, NPCs and Music pages: player models, NPC models and
//!   background music.
//!
//! A configuration that still has those keys loads: [`Config`] keeps every key
//! it is given, so they are written back unchanged.
//!
//! # Values
//!
//! A [`Config`] maps each id to a [`Value`]. Readers use the `get_*` accessors,
//! which convert the way .NET's `Convert.ChangeType` does, as staging's
//! `GetValueOrDefault` does. Where .NET would throw (a string that isn't a
//! number, say), the accessor returns the reader's fallback instead, so a
//! hand-edited file still loads.
//!
//! # Missing keys
//!
//! Staging never fills in a missing key with its schema default. Each reader
//! passes its own fallback, and many of those differ from the schema default:
//! `doors/random` reads as `false` when it is missing, though its default is
//! `true`. A reader in the generator must therefore use staging's fallback, not
//! the item's default. These are the fallbacks that differ, read from staging
//! at `9f862154`:
//!
//! | Key | Staging's fallback | Schema default |
//! |---|---|---|
//! | `doors/random`, `items/random`, `enemies/random` | `false` | `true` |
//! | `items/documents`, `items/documents/keys`, `items/hidden/keys` | `false` | `true` |
//! | `items/weapon/enabled/*` | `false` | `true` |
//! | `items/weapon/initial/max/*` | `0` | the weapon's max |
//! | `items/stack/max/*` | the item's max | `15` |
//! | `items/distribution/*` | `0.0` | `0.5` |
//! | `inventory/min` | `0` | `4` |
//! | `inventory/{primary,secondary}/none`, `…/<weapon>` | `false` | `true` |
//! | `inventory/{primary,secondary}/ammo/{min,max}` | `0.0` | `1`, `2` |
//! | `inventory/health/*/max`, `inventory/ink/max` | `0` | `2`, `3` |
//! | `doors/rooms`, `doors/segments` | `0.5`, `2` | `0.4`, `3` |
//! | `locks/ratio` | `0.0` | `0.25` |
//! | `enemies/rooms`, `enemies/distribution/*` | `0.0` | `0.5` |
//! | `enemies/box`, `enemies/save` | `false` | `true` |
//! | `progression/guardhouse`, `progression/mansion2` | `"Never"` | `"Always"` |
//! | `progression/caves` | `"Always"` | `"Always (Segmented)"` |
//! | `progression/lab` | `"Never"` | `"Always (Segmented)"` |
//! | `ink/enable` | `"Default"`, resolved per character by [`resolve`] | `"Always"` |
//!
//! [`resolve`] writes only the keys staging writes. `ink/enable`,
//! `inventory/size` and `inventory/special/lockpick` are always written, and so
//! are `player`, `progression/tyrant1` and `progression/tyrant2` when they are
//! missing or `Random` (their default is `Random`). Those keys are therefore
//! present afterwards, and never reach a fallback. Otherwise a missing key stays
//! missing, except that the three ink keys are written as `0` when ink is off.

use std::collections::BTreeMap;

use crate::map::Map;
use crate::rng::Rng;

/// The option definition: pages of groups of items, in the order the launcher
/// draws them (staging's `RandomizerConfigurationDefinition`).
#[derive(Clone, Debug, PartialEq)]
pub struct Definition {
    pub pages: Vec<Page>,
}

impl Definition {
    /// Every item, in page, group and item order (staging's `AllItems`).
    pub fn items(&self) -> impl Iterator<Item = &Item> {
        self.pages
            .iter()
            .flat_map(|page| &page.groups)
            .flat_map(|group| &group.items)
    }
}

/// A page of the configuration screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub label: String,
    pub groups: Vec<Group>,
}

/// A group of items on a page. An empty label is an untitled group.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub label: String,
    /// A warning shown with the group.
    pub warning: Option<String>,
    pub items: Vec<Item>,
}

/// One option: its id, its text and how the player edits it.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// The key in a configuration, such as `doors/random`.
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    /// A coloured chip shown with the item.
    pub category: Option<Category>,
    pub kind: Kind,
    /// The value the item starts with (staging's `Default`). Its variant is
    /// the literal staging uses (an int, a double, a bool or a string), so the
    /// JSON written for it matches staging's: `1`, not `1.0`.
    pub default: Value,
}

/// The coloured chip on an item (staging's `GroupItemCategory`).
#[derive(Clone, Debug, PartialEq)]
pub struct Category {
    pub label: String,
    pub text_color: String,
    pub background_color: String,
}

/// How an item is edited, and the values it allows.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// A toggle, with a bool value.
    Switch,
    /// One of `options`, as strings. Some options are special (`Random`,
    /// `Default` and so on), and [`resolve`] replaces them.
    Dropdown { options: Vec<String> },
    /// A number from `min` to `max`. `step` is `None` where staging sets none.
    Range {
        min: f64,
        max: f64,
        step: Option<f64>,
    },
    /// A fraction from `min` to `max`, shown as a percentage: 1.0 is 100%.
    /// `max` may be above 1: starting ammo goes up to 800%.
    Percent {
        min: f64,
        max: f64,
        step: Option<f64>,
    },
}

/// The share of weapon capacity, which staging's ammo descriptions end with.
const AMMO_SHARE: &str =
    "Percentage of weapon capacity. 100% would be fully loaded, no extra ammo.";

/// 2^63, the bound of an i64 as a float.
const TWO_POW_63: f64 = 9_223_372_036_854_775_808.0;

impl Page {
    fn new(label: &str, groups: Vec<Group>) -> Self {
        Self {
            label: label.into(),
            groups,
        }
    }
}

impl Group {
    fn new(label: &str, items: Vec<Item>) -> Self {
        Self {
            label: label.into(),
            warning: None,
            items,
        }
    }

    fn warned(mut self, warning: &str) -> Self {
        self.warning = Some(warning.into());
        self
    }
}

impl Category {
    fn new(label: &str, text_color: &str, background_color: &str) -> Self {
        Self {
            label: label.into(),
            text_color: text_color.into(),
            background_color: background_color.into(),
        }
    }
}

impl Item {
    fn new(id: &str, label: &str, kind: Kind, default: impl Into<Value>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
            category: None,
            kind,
            default: default.into(),
        }
    }

    fn switch(id: &str, label: &str, default: bool) -> Self {
        Self::new(id, label, Kind::Switch, default)
    }

    fn dropdown(id: &str, label: &str, options: &[&str], default: &str) -> Self {
        let options: Vec<String> = options.iter().map(|option| (*option).into()).collect();
        Self::new(id, label, Kind::Dropdown { options }, default)
    }

    fn range(
        id: &str,
        label: &str,
        min: f64,
        max: f64,
        step: Option<f64>,
        default: impl Into<Value>,
    ) -> Self {
        Self::new(id, label, Kind::Range { min, max, step }, default)
    }

    fn percent(
        id: &str,
        label: &str,
        min: f64,
        max: f64,
        step: Option<f64>,
        default: impl Into<Value>,
    ) -> Self {
        Self::new(id, label, Kind::Percent { min, max, step }, default)
    }

    fn described(mut self, description: &str) -> Self {
        self.description = Some(description.into());
        self
    }

    fn in_category(mut self, category: Category) -> Self {
        self.category = Some(category);
        self
    }
}

/// Staging's RE1 configuration definition, built from the map: the same pages,
/// groups, items, ids, labels, descriptions, kinds, ranges and defaults, minus
/// the cosmetic options listed in the module docs.
///
/// Entries generated from the map follow the map's order: the items in
/// `map.items` order, and the enemies by group and then entry.
pub fn definition(map: &Map) -> Definition {
    Definition {
        pages: vec![
            general_page(),
            inventory_page(map),
            items_page(map),
            enemies_page(map),
        ],
    }
}

/// The General page, with its groups in the order the launcher draws them.
fn general_page() -> Page {
    Page::new(
        "General",
        vec![
            Group::new(
                "",
                vec![
                    Item::dropdown("player", "Player", &["Chris", "Jill", "Random"], "Random")
                        .described("Set which player of the game to randomize."),
                    Item::switch("cutscenes/disable", "Disable cutscenes", false).described(
                        "Disables all cutscenes keeping the randomizer completely gameplay focused.",
                    ),
                    Item::dropdown(
                        "ink/enable",
                        "Requires Ink Ribbons to Save",
                        &["Default", "Never", "Always", "Random"],
                        "Always",
                    )
                    .described(concat!(
                        "If disabled, no ink ribbons will be placed, and saving can be done at any time unlimited times at a typewriter. ",
                        "Default will enable it for Chris, but not Jill."
                    )),
                    Item::switch("hard", "Hard Mode", false).described(
                        "If enabled, enemies will be harder to kill. This is equivalent to new game+.",
                    ),
                ],
            ),
            Group::new(
                "Progression (non-door randomizer)",
                vec![
                    Item::dropdown(
                        "progression/guardhouse",
                        "Guardhouse",
                        &["Random", "Always", "Never"],
                        "Always",
                    )
                    .described(
                        "Include the guardhouse in the randomizer. If disabled, the gates to the guardhouse will be locked.",
                    ),
                    Item::dropdown(
                        "progression/mansion2",
                        "Mansion 2",
                        &["Random", "Always", "Never"],
                        "Always",
                    )
                    .described(concat!(
                        "Split the mansion into two segments, before and after plant 42. ",
                        "The mansion 2 key will be behind plant 42, and the battery will be in a mansion 2 room. ",
                        "Only applicable if guardhouse is enabled."
                    )),
                    Item::dropdown(
                        "progression/caves",
                        "Caves",
                        &["Always", "Always (Segmented)"],
                        "Always (Segmented)",
                    )
                    .described(
                        "If segmented, key items required for caves will not be found prior to reaching the caves.",
                    ),
                    Item::dropdown(
                        "progression/lab",
                        "Lab",
                        &[
                            "Random",
                            "Random (Segmented)",
                            "Always",
                            "Always (Segmented)",
                            "Never",
                        ],
                        "Always (Segmented)",
                    )
                    .described(concat!(
                        "Include the lab in the randomizer. If disabled, there will be no doom books, and you can go straight to the heliport when you reach the fountain. ",
                        "If segmented, key items required for the lab will not be found prior to reaching the lab."
                    )),
                    Item::dropdown(
                        "progression/tyrant1",
                        "Mandatory Tyrant 1",
                        &["Random", "Always", "Never"],
                        "Random",
                    )
                    .described("If enabled, Tyrant 1 must be defeated to complete the randomizer."),
                    Item::dropdown(
                        "progression/tyrant2",
                        "Mandatory Tyrant 2",
                        &["Random", "Always", "Never"],
                        "Random",
                    )
                    .described("If enabled, Tyrant 2 must be defeated to complete the randomizer."),
                ],
            ),
            Group::new(
                "Door Randomizer",
                vec![
                    Item::switch("doors/random", "Randomize Doors", true)
                        .described("Let BioRand randomize all the doors in the game."),
                    // Staging sets no step for the segments.
                    Item::range("doors/segments", "Number of Segments", 1.0, 6.0, None, 3)
                        .described(concat!(
                            "Choose the number the segments in the randomizer. ",
                            "Each segment usually ends with a boss or multi-key door.",
                            " Once a segment is complete, no key items are required from a previous segment."
                        )),
                    Item::percent("doors/rooms", "Number of Rooms", 0.0, 1.0, Some(0.1), 0.4)
                        .described(concat!(
                            "Choose the number of rooms to include in the randomizer. ",
                            "The total number of rooms is spread roughly evenly between each segment."
                        )),
                ],
            ),
            Group::new(
                "Door Locks",
                vec![
                    Item::switch("locks/random", "Randomize Locks", false).described(concat!(
                        "Let BioRand randomize the door locks. ",
                        // Staging's wording, typo and all: the text is what players see.
                        "Doors originally without locks may need a key, others may required a different key."
                    )),
                    Item::switch("locks/preserve", "Preserve Vanilla Locks", false)
                        .described("Do not change doors that already have locks."),
                    Item::percent("locks/ratio", "Amount of Locked Doors", 0.0, 1.0, Some(0.01), 0.25)
                        .described("Amount of doors to lock."),
                ],
            ),
        ],
    )
}

/// The Inventory page: the knife and the starting counts, each slot's weapons,
/// then the extras (ammo, healing and ink).
fn inventory_page(map: &Map) -> Page {
    Page::new(
        "Inventory",
        vec![
            Group::new(
                "Main",
                vec![
                    Item::dropdown(
                        "inventory/weapon/knife",
                        "Knife",
                        &["Random", "Never", "Always"],
                        "Always",
                    )
                    .described("Include the knife in starting inventory."),
                    Item::range("inventory/min", "Min. Starting Items", 0.0, 8.0, Some(1.0), 4)
                        .described("Minimum number of inventory slots to fill up."),
                    Item::range("inventory/max", "Max. Starting Items", 0.0, 8.0, Some(1.0), 8)
                        .described("Maximum number of inventory slots to fill up."),
                    Item::dropdown(
                        "inventory/special/lockpick",
                        "Lockpick",
                        &["Default", "Random", "Never", "Always"],
                        "Default",
                    )
                    .described(
                        "Allows you to open locked drawers and sword key doors. Default will leave Jill with lockpick, Chris without.",
                    ),
                    Item::dropdown(
                        "inventory/size",
                        "Size",
                        &["Default", "Random", "6", "8"],
                        "Default",
                    )
                    .described(
                        "Control how many inventory slots you have. Default will leave Chris with 6 slots, Jill with 8.",
                    ),
                ],
            ),
            weapon_group(map, "primary", "Primary"),
            weapon_group(map, "secondary", "Secondary"),
            extras_group(map),
        ],
    )
}

/// One slot's weapon choices: a `None` switch, then a switch for each weapon.
/// The knife is left out; the Main group's knife option covers it.
fn weapon_group(map: &Map, slot: &str, title: &str) -> Group {
    let mut items = vec![Item::switch(
        &format!("inventory/{slot}/none"),
        "None",
        true,
    )];
    for item in map.items.values() {
        if item.kind.starts_with("weapon/") && item.kind != "weapon/knife" {
            items.push(Item::switch(
                &format!("inventory/{slot}/{}", item.kind),
                &item.name,
                true,
            ));
        }
    }
    Group::new(&format!("{title} Weapon"), items)
}

/// The Extras group: the ammo each slot starts with, the healing items and
/// the ink ribbons. Its warning says what happens while the inventory has room.
fn extras_group(map: &Map) -> Group {
    let mut items = Vec::new();
    for (slot, title) in [("primary", "Primary"), ("secondary", "Secondary")] {
        items.push(
            Item::percent(
                &format!("inventory/{slot}/ammo/min"),
                &format!("Min. Ammo for {title} Weapon"),
                0.0,
                8.0,
                Some(0.1),
                1,
            )
            .described(&format!("Minimum ammo for the {slot} weapon. {AMMO_SHARE}")),
        );
        items.push(
            Item::percent(
                &format!("inventory/{slot}/ammo/max"),
                &format!("Max. Ammo for {title} Weapon"),
                0.0,
                8.0,
                Some(0.1),
                2,
            )
            .described(&format!("Maximum ammo for the {slot} weapon. {AMMO_SHARE}")),
        );
    }
    for item in map.items.values() {
        if item.kind.starts_with("health/") {
            items.push(Item::range(
                &format!("inventory/{}/min", item.kind),
                &format!("Min. {}", item.name),
                0.0,
                10.0,
                Some(1.0),
                0,
            ));
            items.push(Item::range(
                &format!("inventory/{}/max", item.kind),
                &format!("Max. {}", item.name),
                0.0,
                10.0,
                Some(1.0),
                2,
            ));
        }
    }
    items.push(
        Item::range(
            "inventory/ink/min",
            "Min. Ink Ribbons",
            0.0,
            32.0,
            Some(1.0),
            0,
        )
        .described("Minimum number of ink ribbons."),
    );
    items.push(
        Item::range(
            "inventory/ink/max",
            "Max. Ink Ribbons",
            0.0,
            32.0,
            Some(1.0),
            3,
        )
        .described("Maximum number of ink ribbons."),
    );
    Group::new("Extras", items).warned("A random selection is chosen until inventory is full.")
}

/// The Items page: the placement switches, the weapons, their starting ammo,
/// the stack sizes and how often each kind of item is placed.
fn items_page(map: &Map) -> Page {
    Page::new(
        "Items",
        vec![
            Group::new(
                "",
                vec![
                    Item::switch("items/random", "Randomize Items", true)
                        .described("Let BioRand randomize all the items in the game."),
                    Item::switch("items/hidden/keys", "Hidden Key Items", true)
                        .described("Hidden items can be keys or weapons."),
                    Item::switch("items/documents", "Replace documents", true)
                        .described("Documents & maps will be replaced with items."),
                    Item::switch("items/documents/keys", "Documents can be Keys", true).described(
                        "Documents & maps can be keys or weapons if replaced with items.",
                    ),
                ],
            ),
            weapons_group(map),
            initial_ammo_group(map),
            stack_group(map),
            distribution_group(map),
        ],
    )
}

/// Every weapon, the knife included, as a switch.
fn weapons_group(map: &Map) -> Group {
    let items = map
        .items
        .values()
        .filter(|item| item.kind.starts_with("weapon/"))
        .map(|item| {
            Item::switch(
                &format!("items/weapon/enabled/{}", item.kind),
                &item.name,
                true,
            )
        })
        .collect();
    Group::new("Weapons", items)
}

/// The starting ammo range of each weapon with a capacity (its `max`). The
/// default maximum is that capacity.
fn initial_ammo_group(map: &Map) -> Group {
    let mut items = Vec::new();
    for item in map
        .items
        .values()
        .filter(|item| item.kind.starts_with("weapon/") && item.max != 0)
    {
        let max = f64::from(item.max);
        items.push(Item::range(
            &format!("items/weapon/initial/min/{}", item.kind),
            &format!("Min. {}", item.name),
            0.0,
            max,
            Some(1.0),
            0,
        ));
        items.push(Item::range(
            &format!("items/weapon/initial/max/{}", item.kind),
            &format!("Max. {}", item.name),
            0.0,
            max,
            Some(1.0),
            i64::from(item.max),
        ));
    }
    Group::new("Weapons (Initial Ammo)", items)
}

/// The stack size range of each ammo kind and of ink.
fn stack_group(map: &Map) -> Group {
    let mut items = Vec::new();
    for item in map
        .items
        .values()
        .filter(|item| item.kind.starts_with("ammo/") || item.kind == "ink")
    {
        let max = f64::from(item.max);
        items.push(Item::range(
            &format!("items/stack/min/{}", item.kind),
            &format!("Min. {}", item.name),
            1.0,
            max,
            Some(1.0),
            1,
        ));
        items.push(Item::range(
            &format!("items/stack/max/{}", item.kind),
            &format!("Max. {}", item.name),
            1.0,
            max,
            Some(1.0),
            15,
        ));
    }
    Group::new("Stack", items)
}

/// How much of each ammo kind, health kind and ink is placed.
fn distribution_group(map: &Map) -> Group {
    let items = map
        .items
        .values()
        .filter(|item| {
            item.kind.starts_with("ammo/") || item.kind.starts_with("health/") || item.kind == "ink"
        })
        .map(|item| {
            Item::range(
                &format!("items/distribution/{}", item.kind),
                &item.name,
                0.0,
                1.0,
                Some(0.01),
                0.5,
            )
            .in_category(distribution_category(&item.kind))
        })
        .collect();
    Group::new("Item Distribution", items)
}

/// The chip for an item kind: ammo, health or ink. Other kinds never reach it
/// (staging's `GetCategory` has the same fallback).
fn distribution_category(kind: &str) -> Category {
    match kind.split('/').next() {
        Some("ammo") => Category::new("Ammo", "#fff", "#66f"),
        Some("health") => Category::new("Health", "#fff", "#696"),
        Some("ink") => Category::new("Ink", "#fff", "#000"),
        _ => Category::new("None", "#fff", "#333"),
    }
}

/// The Enemies page: the placement switches and the share of rooms with
/// enemies, then how often each enemy kind is placed and how many per room.
fn enemies_page(map: &Map) -> Page {
    Page::new(
        "Enemies",
        vec![
            Group::new(
                "",
                vec![
                    Item::switch("enemies/random", "Randomize Enemies", true).described(
                        "Allow BioRand to place random enemies in random places in each room.",
                    ),
                    Item::switch("enemies/safe", "Enemies in safe rooms", false).described(
                        "Allow enemies in safe rooms. i.e. rooms where safe music is playing.",
                    ),
                    Item::switch("enemies/save", "Enemies in save rooms", true).described(
                        "Allow enemies in save rooms. i.e. rooms where there is a typewriter.",
                    ),
                    Item::switch("enemies/box", "Enemies in box rooms", true).described(
                        "Allow enemies in box rooms. i.e. rooms where there is an item box.",
                    ),
                    Item::percent(
                        "enemies/rooms",
                        "Number of rooms containing enemies.",
                        0.0,
                        1.0,
                        Some(0.01),
                        0.5,
                    )
                    .described("The number of rooms that contain one or more enemies."),
                ],
            ),
            enemy_distribution_group(map),
            enemy_min_max_group(map),
        ],
    )
}

/// How often each enemy kind is placed, in its group's colours.
fn enemy_distribution_group(map: &Map) -> Group {
    let mut items = Vec::new();
    for enemies in &map.enemies {
        let category = Category::new(&enemies.name, &enemies.foreground, &enemies.background);
        for entry in &enemies.entries {
            items.push(
                Item::range(
                    &format!("enemies/distribution/{}", entry.key),
                    &entry.name,
                    0.0,
                    1.0,
                    Some(0.01),
                    0.5,
                )
                .in_category(category.clone()),
            );
        }
    }
    Group::new("Room Distribution", items)
}

/// The fewest and most of each enemy kind in a room.
fn enemy_min_max_group(map: &Map) -> Group {
    let mut items = Vec::new();
    for enemies in &map.enemies {
        for entry in &enemies.entries {
            items.push(Item::range(
                &format!("enemies/min/{}", entry.key),
                &format!("Min. {}", entry.name),
                1.0,
                16.0,
                Some(1.0),
                1,
            ));
            items.push(Item::range(
                &format!("enemies/max/{}", entry.key),
                &format!("Max. {}", entry.name),
                1.0,
                16.0,
                Some(1.0),
                6,
            ));
        }
    }
    Group::new("Min/Max per Room", items)
}

/// A configuration value. Staging stores the .NET object that a JSON value
/// becomes: a bool, an int, a double or a string.
///
/// Numbers read the way staging's `FromJson` reads them: a whole number is an
/// int and any other number a double. See [`Config::from_json`].
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

impl Value {
    /// The value as a bool, as `Convert.ChangeType(value, typeof(bool))`. Any
    /// non-zero number is true, and a string must be `true` or `false` in any
    /// case, ignoring surrounding spaces. `None` when it isn't either.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(value) => Some(*value),
            Value::Int(value) => Some(*value != 0),
            Value::Float(value) => Some(*value != 0.0),
            Value::Str(text) => {
                let text = text.trim();
                if text.eq_ignore_ascii_case("true") {
                    Some(true)
                } else if text.eq_ignore_ascii_case("false") {
                    Some(false)
                } else {
                    None
                }
            }
        }
    }

    /// The value as an int, as `Convert.ChangeType(value, typeof(int))`. A
    /// double rounds half to even (2.5 is 2). A string must be a whole number.
    /// `None` when it isn't convertible, or is out of range.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Bool(value) => Some(i64::from(*value)),
            Value::Int(value) => Some(*value),
            Value::Float(value) => round_half_even(*value),
            Value::Str(text) => text.trim().parse().ok(),
        }
    }

    /// The value as a double, as `Convert.ChangeType(value, typeof(double))`.
    /// A string must be a plain number (`.NET` also takes thousands
    /// separators, which this doesn't).
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Bool(value) => Some(f64::from(u8::from(*value))),
            Value::Int(value) => Some(*value as f64),
            Value::Float(value) => Some(*value),
            Value::Str(text) => text.trim().parse().ok(),
        }
    }

    /// The value as text, as `Convert.ChangeType(value, typeof(string))`: a
    /// bool is `True` or `False`, and a number is its text.
    pub fn as_text(&self) -> String {
        match self {
            Value::Bool(true) => "True".into(),
            Value::Bool(false) => "False".into(),
            Value::Int(value) => value.to_string(),
            Value::Float(value) => value.to_string(),
            Value::Str(text) => text.clone(),
        }
    }

    /// A JSON value as a configuration value, as staging's `ProcessConfigValue`
    /// does. Null, arrays and objects have no value and are dropped.
    fn from_json(value: &serde_json::Value) -> Option<Value> {
        match value {
            serde_json::Value::Bool(value) => Some(Value::Bool(*value)),
            serde_json::Value::Number(number) => Some(match number.as_i64() {
                Some(int) => Value::Int(int),
                None => whole_or_float(number.as_f64()?),
            }),
            serde_json::Value::String(text) => Some(Value::Str(text.clone())),
            serde_json::Value::Null
            | serde_json::Value::Array(_)
            | serde_json::Value::Object(_) => None,
        }
    }

    /// The value as JSON. Not-a-number and the infinities aren't JSON, so they
    /// are written as null, which reads back as no value.
    fn to_json(&self) -> serde_json::Value {
        match self {
            Value::Bool(value) => serde_json::Value::Bool(*value),
            Value::Int(value) => serde_json::Value::from(*value),
            Value::Float(value) => serde_json::Number::from_f64(*value)
                .map_or(serde_json::Value::Null, serde_json::Value::Number),
            Value::Str(text) => serde_json::Value::String(text.clone()),
        }
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Value::Bool(value)
    }
}

impl From<i64> for Value {
    fn from(value: i64) -> Self {
        Value::Int(value)
    }
}

impl From<i32> for Value {
    fn from(value: i32) -> Self {
        Value::Int(i64::from(value))
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Value::Float(value)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Value::Str(value.into())
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Value::Str(value)
    }
}

/// Rounds half to even, as `Convert.ToInt32(double)` does. `None` for NaN and
/// for values outside the i64 range.
fn round_half_even(value: f64) -> Option<i64> {
    let rounded = value.round_ties_even();
    (-TWO_POW_63..TWO_POW_63)
        .contains(&rounded)
        .then_some(rounded as i64)
}

/// A JSON number that isn't an i64: a whole number in range is an int, as
/// staging's `ProcessNumber` makes it, and anything else a double.
fn whole_or_float(value: f64) -> Value {
    if value.fract() == 0.0 && (-TWO_POW_63..TWO_POW_63).contains(&value) {
        Value::Int(value as i64)
    } else {
        Value::Float(value)
    }
}

/// A configuration: the value of each id (staging's `RandomizerConfiguration`).
///
/// The values are kept in id order, so equal configurations print the same
/// JSON. Staging keeps insertion order, but nothing reads it: its random
/// options walk the definition, not the values.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Config {
    values: BTreeMap<String, Value>,
}

impl Config {
    /// An empty configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// The definition's default for every item (staging's `GetDefault`).
    pub fn defaults(definition: &Definition) -> Self {
        Self {
            values: definition
                .items()
                .map(|item| (item.id.clone(), item.default.clone()))
                .collect(),
        }
    }

    /// The value of `id`, exactly as it is stored.
    pub fn get(&self, id: &str) -> Option<&Value> {
        self.values.get(id)
    }

    /// Sets `id`, replacing any value it had.
    pub fn set(&mut self, id: impl Into<String>, value: impl Into<Value>) {
        self.values.insert(id.into(), value.into());
    }

    /// Every id and its value, in id order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.values.iter().map(|(id, value)| (id.as_str(), value))
    }

    /// The value of `id` as a bool, or `default` when it is missing or isn't
    /// convertible (staging's `GetValueOrDefault<bool>`).
    pub fn get_bool(&self, id: &str, default: bool) -> bool {
        self.get(id).and_then(Value::as_bool).unwrap_or(default)
    }

    /// The value of `id` as an int, or `default`.
    pub fn get_int(&self, id: &str, default: i64) -> i64 {
        self.get(id).and_then(Value::as_int).unwrap_or(default)
    }

    /// The value of `id` as a double, or `default`.
    pub fn get_f64(&self, id: &str, default: f64) -> f64 {
        self.get(id).and_then(Value::as_f64).unwrap_or(default)
    }

    /// The value of `id` as text, or `default`. A number or a bool is
    /// converted, as staging's `GetValueOrDefault<string>` converts it.
    pub fn get_str(&self, id: &str, default: &str) -> String {
        self.get(id).map_or_else(|| default.into(), Value::as_text)
    }

    /// `self` with every value of `right` written over it (staging's `+`).
    pub fn merge(&self, right: &Config) -> Config {
        let mut merged = self.clone();
        merged.values.extend(
            right
                .values
                .iter()
                .map(|(id, value)| (id.clone(), value.clone())),
        );
        merged
    }

    /// Reads a flat JSON object, as staging's `FromJson` does. Null, arrays
    /// and objects are dropped, and every other key is kept, known or not.
    /// Anything other than an object is an error.
    pub fn from_json(text: &str) -> serde_json::Result<Config> {
        let object: serde_json::Map<String, serde_json::Value> = serde_json::from_str(text)?;
        Ok(Config {
            values: object
                .iter()
                .filter_map(|(id, value)| Some((id.clone(), Value::from_json(value)?)))
                .collect(),
        })
    }

    /// Writes a pretty-printed flat JSON object, in id order.
    pub fn to_json(&self) -> String {
        let object: serde_json::Map<String, serde_json::Value> = self
            .values
            .iter()
            .map(|(id, value)| (id.clone(), value.to_json()))
            .collect();
        serde_json::to_string_pretty(&object).expect("a map of JSON scalars always serializes")
    }
}

/// Resolves a configuration the way staging's `ApplyConfigModifications` does,
/// and returns the configuration the generator reads. `config` is not changed.
///
/// In order:
///
/// 1. A dropdown set to `Random` takes one of its options, drawn from `rng`,
///    except `Random`, `Random (Segmented)` and `Always (Segmented)`. A dropdown
///    set to `Random (Segmented)` takes one except `Random`, `Random
///    (Segmented)` and `Always`. Dropdowns are visited in definition order, so
///    a seed always gives the same choices. A missing key reads as its default,
///    so a missing `player` is random too.
/// 2. `ink/enable`: `Default` becomes `Always` for Chris and `Never` for Jill.
///    Unless the result is `Always`, `inventory/ink/min`, `inventory/ink/max`
///    and `items/distribution/ink` become `0`.
/// 3. `inventory/size`: `Default` becomes `6` for Chris and `8` for Jill.
/// 4. `inventory/special/lockpick`: `Default` becomes `Never` for Chris and
///    `Always` for Jill.
///
/// A missing `ink/enable` reads as `Default`, which staging does, rather than
/// as its schema default `Always`. So a configuration without it gets the value
/// for its player. Staging also falls back to Chris for a missing `player` in
/// step 2, but step 1 has already written one, so that fallback is never used.
///
/// Only the keys above are written, as staging writes them. Staging's
/// `ApplyModGeneral`, which fills the mod's `General` block, is not part of
/// this function.
///
/// `rng` should be `Rng::named(seed, "config")`, the stream staging's
/// `GetRng("config")` names. Its values differ from staging's for a seed (see
/// [`Rng`]). Staging's definition also has cosmetic dropdowns that this one
/// leaves out, and each one set to `Random` shifts the draws after it.
pub fn resolve(definition: &Definition, config: &Config, rng: &mut Rng) -> Config {
    let mut resolved = config.clone();
    choose_random_options(definition, &mut resolved, rng);

    let ink = resolve_per_character(&mut resolved, "ink/enable", "Always", "Never");
    if ink != "Always" {
        resolved.set("inventory/ink/min", 0);
        resolved.set("inventory/ink/max", 0);
        resolved.set("items/distribution/ink", 0);
    }
    resolve_per_character(&mut resolved, "inventory/size", "6", "8");
    resolve_per_character(
        &mut resolved,
        "inventory/special/lockpick",
        "Never",
        "Always",
    );
    resolved
}

/// Staging's `ApplyRandomConfigOptions`: each dropdown set to a `Random`
/// option takes one of its allowed choices.
fn choose_random_options(definition: &Definition, config: &mut Config, rng: &mut Rng) {
    for item in definition.items() {
        let Kind::Dropdown { options } = &item.kind else {
            continue;
        };
        // Staging reads the value with the item's default as the fallback,
        // when the default is a string (`configItem.Default as string ?? ""`).
        let fallback = match &item.default {
            Value::Str(text) => text.as_str(),
            _ => "",
        };
        let excluded: &[&str] = match config.get_str(&item.id, fallback).as_str() {
            "Random" => &["Random", "Random (Segmented)", "Always (Segmented)"],
            "Random (Segmented)" => &["Random", "Random (Segmented)", "Always"],
            _ => continue,
        };
        let choices: Vec<&str> = options
            .iter()
            .map(String::as_str)
            .filter(|option| !excluded.contains(option))
            .collect();
        // With nothing to choose from, the value stays `Random`.
        if !choices.is_empty() {
            let chosen = *rng.pick(&choices);
            config.set(item.id.as_str(), chosen);
        }
    }
}

/// Staging's `UpdateConfigNeverAlways`: a `Default` value (or a missing one)
/// becomes `chris` or `jill`, by the player. Writes the value and returns it.
fn resolve_per_character(config: &mut Config, id: &str, chris: &str, jill: &str) -> String {
    let value = config.get_str(id, "Default");
    let value = if value == "Default" {
        match config.get_str("player", "Chris").as_str() {
            "Chris" => chris.to_string(),
            _ => jill.to_string(),
        }
    } else {
        value
    };
    config.set(id, value.as_str());
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{EnemyEntry, EnemyGroup, ItemDef};
    use indexmap::IndexMap;
    use std::collections::{BTreeSet, HashSet};

    /// A small map with each kind the definition sorts: a knife and two other
    /// weapons, ammo, two herbs and ink, plus two kinds that are left out.
    fn synthetic_map() -> Map {
        let rows: [(u8, &str, &str, u16); 9] = [
            (1, "Knife", "weapon/knife", 0),
            (2, "Handgun", "weapon/handgun", 15),
            (3, "Shotgun", "weapon/shotgun", 10),
            (4, "Handgun Ammo", "ammo/handgun", 50),
            (5, "Green Herb", "health/g", 0),
            (6, "Red Herb", "health/r", 0),
            (7, "Ink Ribbon", "ink", 32),
            (8, "Blue Key", "key/consumable", 0),
            (9, "Map", "map", 0),
        ];
        let mut items = IndexMap::new();
        for (id, name, kind, max) in rows {
            items.insert(
                id,
                ItemDef {
                    name: name.into(),
                    kind: kind.into(),
                    max,
                    ..ItemDef::default()
                },
            );
        }
        let entry = |key: &str, name: &str| EnemyEntry {
            key: key.into(),
            name: name.into(),
            ids: Vec::new(),
            esp: Vec::new(),
            poses: Vec::new(),
        };
        Map {
            items,
            enemies: vec![
                EnemyGroup {
                    name: "Zombie".into(),
                    background: "#66f".into(),
                    foreground: "#fff".into(),
                    entries: vec![entry("zombie", "Zombie"), entry("hunter", "Hunter")],
                },
                EnemyGroup {
                    name: "Animal".into(),
                    background: "#696".into(),
                    foreground: "#000".into(),
                    entries: vec![entry("crow", "Crow"), entry("dog", "Dog")],
                },
            ],
            ..Map::default()
        }
    }

    /// Staging's ids for `synthetic_map`, in staging's order, from
    /// `ClassicRandomizer.CreateConfigDefinition` and
    /// `Re1ClassicRandomizerController.UpdateConfigDefinition`, without the
    /// title call, the enemy skins and the Protagonist, NPCs and Music pages.
    const EXPECTED_IDS: &[&str] = &[
        // General
        "player",
        "cutscenes/disable",
        "ink/enable",
        "hard",
        "progression/guardhouse",
        "progression/mansion2",
        "progression/caves",
        "progression/lab",
        "progression/tyrant1",
        "progression/tyrant2",
        "doors/random",
        "doors/segments",
        "doors/rooms",
        "locks/random",
        "locks/preserve",
        "locks/ratio",
        // Inventory
        "inventory/weapon/knife",
        "inventory/min",
        "inventory/max",
        "inventory/special/lockpick",
        "inventory/size",
        "inventory/primary/none",
        "inventory/primary/weapon/handgun",
        "inventory/primary/weapon/shotgun",
        "inventory/secondary/none",
        "inventory/secondary/weapon/handgun",
        "inventory/secondary/weapon/shotgun",
        "inventory/primary/ammo/min",
        "inventory/primary/ammo/max",
        "inventory/secondary/ammo/min",
        "inventory/secondary/ammo/max",
        "inventory/health/g/min",
        "inventory/health/g/max",
        "inventory/health/r/min",
        "inventory/health/r/max",
        "inventory/ink/min",
        "inventory/ink/max",
        // Items
        "items/random",
        "items/hidden/keys",
        "items/documents",
        "items/documents/keys",
        "items/weapon/enabled/weapon/knife",
        "items/weapon/enabled/weapon/handgun",
        "items/weapon/enabled/weapon/shotgun",
        "items/weapon/initial/min/weapon/handgun",
        "items/weapon/initial/max/weapon/handgun",
        "items/weapon/initial/min/weapon/shotgun",
        "items/weapon/initial/max/weapon/shotgun",
        "items/stack/min/ammo/handgun",
        "items/stack/max/ammo/handgun",
        "items/stack/min/ink",
        "items/stack/max/ink",
        "items/distribution/ammo/handgun",
        "items/distribution/health/g",
        "items/distribution/health/r",
        "items/distribution/ink",
        // Enemies
        "enemies/random",
        "enemies/safe",
        "enemies/save",
        "enemies/box",
        "enemies/rooms",
        "enemies/distribution/zombie",
        "enemies/distribution/hunter",
        "enemies/distribution/crow",
        "enemies/distribution/dog",
        "enemies/min/zombie",
        "enemies/max/zombie",
        "enemies/min/hunter",
        "enemies/max/hunter",
        "enemies/min/crow",
        "enemies/max/crow",
        "enemies/min/dog",
        "enemies/max/dog",
    ];

    /// The pages' groups, in order, as (page, group) labels.
    const EXPECTED_GROUPS: &[(&str, &str)] = &[
        ("General", ""),
        ("General", "Progression (non-door randomizer)"),
        ("General", "Door Randomizer"),
        ("General", "Door Locks"),
        ("Inventory", "Main"),
        ("Inventory", "Primary Weapon"),
        ("Inventory", "Secondary Weapon"),
        ("Inventory", "Extras"),
        ("Items", ""),
        ("Items", "Weapons"),
        ("Items", "Weapons (Initial Ammo)"),
        ("Items", "Stack"),
        ("Items", "Item Distribution"),
        ("Enemies", ""),
        ("Enemies", "Room Distribution"),
        ("Enemies", "Min/Max per Room"),
    ];

    fn item<'a>(definition: &'a Definition, id: &str) -> &'a Item {
        definition
            .items()
            .find(|item| item.id == id)
            .unwrap_or_else(|| panic!("no item {id}"))
    }

    fn assert_one_of(config: &Config, id: &str, allowed: &[&str]) {
        let value = config.get_str(id, "");
        assert!(
            allowed.contains(&value.as_str()),
            "{id} resolved to {value:?}, not one of {allowed:?}"
        );
    }

    #[test]
    fn ids_match_staging_and_are_unique() {
        let definition = definition(&synthetic_map());
        let ids: Vec<&str> = definition.items().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, EXPECTED_IDS);
        let unique: HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(unique.len(), ids.len(), "ids must be unique");
    }

    #[test]
    fn pages_and_groups_follow_staging() {
        let definition = definition(&synthetic_map());
        let pages: Vec<&str> = definition.pages.iter().map(|p| p.label.as_str()).collect();
        assert_eq!(pages, ["General", "Inventory", "Items", "Enemies"]);
        let groups: Vec<(&str, &str)> = definition
            .pages
            .iter()
            .flat_map(|page| {
                page.groups
                    .iter()
                    .map(move |group| (page.label.as_str(), group.label.as_str()))
            })
            .collect();
        assert_eq!(groups, EXPECTED_GROUPS);
        let extras = &definition.pages[1].groups[3];
        assert_eq!(
            extras.warning.as_deref(),
            Some("A random selection is chosen until inventory is full.")
        );
    }

    #[test]
    fn texts_match_staging() {
        let definition = definition(&synthetic_map());
        assert_eq!(
            item(&definition, "doors/segments").description.as_deref(),
            Some(
                "Choose the number the segments in the randomizer. Each segment usually ends with a boss or multi-key door. Once a segment is complete, no key items are required from a previous segment."
            )
        );
        assert_eq!(
            item(&definition, "locks/random").description.as_deref(),
            Some(
                "Let BioRand randomize the door locks. Doors originally without locks may need a key, others may required a different key."
            )
        );
        assert_eq!(
            item(&definition, "inventory/primary/ammo/min").label,
            "Min. Ammo for Primary Weapon"
        );
        assert_eq!(
            item(&definition, "inventory/secondary/ammo/max")
                .description
                .as_deref(),
            Some(
                "Maximum ammo for the secondary weapon. Percentage of weapon capacity. 100% would be fully loaded, no extra ammo."
            )
        );
    }

    #[test]
    fn kinds_and_ranges_match_staging() {
        let definition = definition(&synthetic_map());
        assert_eq!(
            item(&definition, "doors/segments").kind,
            Kind::Range {
                min: 1.0,
                max: 6.0,
                step: None
            }
        );
        assert_eq!(
            item(&definition, "doors/rooms").kind,
            Kind::Percent {
                min: 0.0,
                max: 1.0,
                step: Some(0.1)
            }
        );
        assert_eq!(
            item(&definition, "inventory/primary/ammo/max").kind,
            Kind::Percent {
                min: 0.0,
                max: 8.0,
                step: Some(0.1)
            }
        );
        assert_eq!(
            item(&definition, "inventory/weapon/knife").kind,
            Kind::Dropdown {
                options: vec!["Random".into(), "Never".into(), "Always".into()]
            }
        );
        assert_eq!(
            item(&definition, "items/weapon/initial/min/weapon/handgun").kind,
            Kind::Range {
                min: 0.0,
                max: 15.0,
                step: Some(1.0)
            }
        );
    }

    #[test]
    fn chips_follow_staging() {
        let definition = definition(&synthetic_map());
        assert_eq!(
            item(&definition, "items/distribution/health/g").category,
            Some(Category::new("Health", "#fff", "#696"))
        );
        assert_eq!(
            item(&definition, "items/distribution/ink").category,
            Some(Category::new("Ink", "#fff", "#000"))
        );
        assert_eq!(
            item(&definition, "enemies/distribution/crow").category,
            Some(Category::new("Animal", "#000", "#696"))
        );
        assert_eq!(item(&definition, "inventory/ink/min").category, None);
    }

    #[test]
    fn defaults_keep_staging_values_and_types() {
        let definition = definition(&synthetic_map());
        let config = Config::defaults(&definition);
        assert_eq!(config.iter().count(), EXPECTED_IDS.len());
        assert_eq!(config.get("player"), Some(&Value::Str("Random".into())));
        assert_eq!(config.get("cutscenes/disable"), Some(&Value::Bool(false)));
        assert_eq!(config.get("doors/segments"), Some(&Value::Int(3)));
        assert_eq!(config.get("doors/rooms"), Some(&Value::Float(0.4)));
        // Staging's literal `1` for starting ammo is an int, not a double.
        assert_eq!(
            config.get("inventory/primary/ammo/min"),
            Some(&Value::Int(1))
        );
        assert_eq!(
            config.get("items/weapon/initial/max/weapon/shotgun"),
            Some(&Value::Int(10))
        );
        assert_eq!(
            config.get("items/distribution/ink"),
            Some(&Value::Float(0.5))
        );
        assert_eq!(config.get("enemies/max/dog"), Some(&Value::Int(6)));
        assert_eq!(config.get("title/call/random"), None);
    }

    #[test]
    fn values_convert_like_convert_changetype() {
        // Doubles round half to even, as Convert.ToInt32 does.
        assert_eq!(Value::Float(2.5).as_int(), Some(2));
        assert_eq!(Value::Float(3.5).as_int(), Some(4));
        assert_eq!(Value::Float(-2.5).as_int(), Some(-2));
        assert_eq!(Value::Float(0.5).as_int(), Some(0));
        assert_eq!(Value::Float(2.6).as_int(), Some(3));
        assert_eq!(Value::Float(f64::NAN).as_int(), None);
        assert_eq!(Value::Float(1e300).as_int(), None);

        // Numbers and bools convert to each other.
        assert_eq!(Value::Int(4).as_f64(), Some(4.0));
        assert_eq!(Value::Bool(true).as_int(), Some(1));
        assert_eq!(Value::Bool(false).as_f64(), Some(0.0));

        // Any non-zero number is true, as Convert.ToBoolean.
        assert_eq!(Value::Int(0).as_bool(), Some(false));
        assert_eq!(Value::Int(1).as_bool(), Some(true));
        assert_eq!(Value::Int(2).as_bool(), Some(true));
        assert_eq!(Value::Float(0.0).as_bool(), Some(false));
        assert_eq!(Value::Float(f64::NAN).as_bool(), Some(true));

        // Strings parse: a whole number as an int, any number as a double,
        // and `true` or `false` as a bool.
        assert_eq!(Value::Str(" 7 ".into()).as_int(), Some(7));
        assert_eq!(Value::Str("2.5".into()).as_int(), None);
        assert_eq!(Value::Str("0.4".into()).as_f64(), Some(0.4));
        assert_eq!(Value::Str("abc".into()).as_f64(), None);
        assert_eq!(Value::Str("True".into()).as_bool(), Some(true));
        assert_eq!(Value::Str("1".into()).as_bool(), None);

        // Everything is text, as Convert.ToString.
        assert_eq!(Value::Int(6).as_text(), "6");
        assert_eq!(Value::Float(0.5).as_text(), "0.5");
        assert_eq!(Value::Bool(false).as_text(), "False");
    }

    #[test]
    fn readers_fall_back_when_missing_or_unconvertible() {
        let mut config = Config::new();
        config.set("fraction", 2.5);
        config.set("text", "abc");
        config.set("count", 4);
        assert_eq!(config.get_int("fraction", 9), 2);
        assert_eq!(config.get_int("text", 9), 9);
        assert_eq!(config.get_int("missing", 9), 9);
        assert_eq!(config.get_f64("count", 0.0), 4.0);
        assert!(config.get_bool("text", true));
        assert_eq!(config.get_str("count", "x"), "4");
        assert_eq!(config.get_str("missing", "x"), "x");
    }

    #[test]
    fn merge_writes_the_right_side_over_the_left() {
        let mut left = Config::new();
        left.set("a", 1);
        left.set("b", 2);
        let mut right = Config::new();
        right.set("b", 3);
        right.set("c", 4);
        let merged = left.merge(&right);
        assert_eq!(merged.get("a"), Some(&Value::Int(1)));
        assert_eq!(merged.get("b"), Some(&Value::Int(3)));
        assert_eq!(merged.get("c"), Some(&Value::Int(4)));
        assert_eq!(merged.iter().count(), 3);
    }

    #[test]
    fn json_reads_like_from_json_and_round_trips() {
        let text = r#"{
            "player": "Jill",
            "hard": true,
            "inventory/min": 4.0,
            "doors/rooms": 0.4,
            "items/documents": null,
            "enemies/skin/zombie": [1, 2],
            "custom/key": {"nested": 1},
            "title/call/random": false
        }"#;
        let config = Config::from_json(text).unwrap();
        assert_eq!(config.get("player"), Some(&Value::Str("Jill".into())));
        assert_eq!(config.get("hard"), Some(&Value::Bool(true)));
        // A whole number is an int, as staging's FromJson makes it.
        assert_eq!(config.get("inventory/min"), Some(&Value::Int(4)));
        assert_eq!(config.get("doors/rooms"), Some(&Value::Float(0.4)));
        // Null, arrays and objects are dropped.
        assert_eq!(config.get("items/documents"), None);
        assert_eq!(config.get("enemies/skin/zombie"), None);
        assert_eq!(config.get("custom/key"), None);
        // Keys the definition doesn't know are kept.
        assert_eq!(config.get("title/call/random"), Some(&Value::Bool(false)));

        assert!(Config::from_json("[1, 2]").is_err());
        assert_eq!(Config::from_json(&config.to_json()).unwrap(), config);
    }

    #[test]
    fn defaults_write_staging_shaped_json() {
        let definition = definition(&synthetic_map());
        let config = Config::defaults(&definition);
        let json: serde_json::Value = serde_json::from_str(&config.to_json()).unwrap();
        assert_eq!(json["inventory/primary/ammo/min"], 1);
        assert_eq!(json["doors/rooms"], 0.4);
        assert_eq!(json["player"], "Random");
        assert_eq!(json["cutscenes/disable"], false);
        assert_eq!(Config::from_json(&config.to_json()).unwrap(), config);
    }

    fn with_player(config: &Config, player: &str) -> Config {
        let mut config = config.clone();
        config.set("player", player);
        config
    }

    #[test]
    fn default_resolves_per_character() {
        let definition = definition(&synthetic_map());
        let mut config = Config::defaults(&definition);
        config.set("ink/enable", "Default");
        config.set("inventory/size", "Default");
        config.set("inventory/special/lockpick", "Default");

        let chris = resolve(
            &definition,
            &with_player(&config, "Chris"),
            &mut Rng::named(1, "config"),
        );
        assert_eq!(chris.get_str("ink/enable", ""), "Always");
        assert_eq!(chris.get_int("inventory/ink/max", -1), 3, "ink is kept");
        assert_eq!(chris.get_str("inventory/size", ""), "6");
        assert_eq!(chris.get_str("inventory/special/lockpick", ""), "Never");

        let jill = resolve(
            &definition,
            &with_player(&config, "Jill"),
            &mut Rng::named(1, "config"),
        );
        assert_eq!(jill.get_str("ink/enable", ""), "Never");
        assert_eq!(jill.get("inventory/ink/min"), Some(&Value::Int(0)));
        assert_eq!(jill.get("inventory/ink/max"), Some(&Value::Int(0)));
        assert_eq!(jill.get("items/distribution/ink"), Some(&Value::Int(0)));
        assert_eq!(jill.get_str("inventory/size", ""), "8");
        assert_eq!(jill.get_str("inventory/special/lockpick", ""), "Always");
    }

    #[test]
    fn missing_keys_keep_staging_fallbacks() {
        let definition = definition(&synthetic_map());
        let mut config = Config::new();
        config.set("player", "Jill");
        let resolved = resolve(&definition, &config, &mut Rng::named(1, "config"));
        // Staging reads a missing ink/enable as Default, so it is per
        // character, not the schema's Always.
        assert_eq!(resolved.get_str("ink/enable", ""), "Never");
        assert_eq!(resolved.get_str("inventory/size", ""), "8");
        assert_eq!(resolved.get_str("inventory/special/lockpick", ""), "Always");
        // Keys staging doesn't write stay missing, for readers to default.
        assert_eq!(resolved.get("doors/random"), None);
        assert_eq!(resolved.get("progression/guardhouse"), None);
    }

    #[test]
    fn a_missing_player_is_random() {
        let definition = definition(&synthetic_map());
        let mut players = BTreeSet::new();
        for seed in 0..50 {
            let resolved = resolve(&definition, &Config::new(), &mut Rng::named(seed, "config"));
            assert_one_of(&resolved, "player", &["Chris", "Jill"]);
            players.insert(resolved.get_str("player", ""));
        }
        assert_eq!(players.len(), 2, "both players are drawn");
    }

    #[test]
    fn random_options_pick_only_allowed_choices() {
        let definition = definition(&synthetic_map());
        let mut config = Config::defaults(&definition);
        for (id, value) in [
            ("player", "Random"),
            ("ink/enable", "Random"),
            ("progression/guardhouse", "Random"),
            ("progression/lab", "Random"),
            ("inventory/weapon/knife", "Random"),
            ("inventory/size", "Random"),
            ("inventory/special/lockpick", "Random"),
        ] {
            config.set(id, value);
        }
        let before = config.clone();
        let mut players = BTreeSet::new();
        for seed in 0..200 {
            let resolved = resolve(&definition, &config, &mut Rng::named(seed, "config"));
            assert_one_of(&resolved, "player", &["Chris", "Jill"]);
            assert_one_of(&resolved, "ink/enable", &["Always", "Never"]);
            assert_one_of(&resolved, "progression/guardhouse", &["Always", "Never"]);
            // Random never gives a segmented option.
            assert_one_of(&resolved, "progression/lab", &["Always", "Never"]);
            assert_one_of(&resolved, "inventory/weapon/knife", &["Never", "Always"]);
            assert_one_of(&resolved, "inventory/size", &["6", "8"]);
            assert_one_of(
                &resolved,
                "inventory/special/lockpick",
                &["Never", "Always"],
            );
            players.insert(resolved.get_str("player", ""));
        }
        assert_eq!(players.len(), 2, "both players are drawn");
        assert_eq!(config, before, "resolve doesn't change its input");
    }

    #[test]
    fn random_segmented_takes_a_segmented_option_or_never() {
        let definition = definition(&synthetic_map());
        let mut config = Config::defaults(&definition);
        config.set("progression/lab", "Random (Segmented)");
        let mut choices = BTreeSet::new();
        for seed in 0..200 {
            let resolved = resolve(&definition, &config, &mut Rng::named(seed, "config"));
            assert_one_of(
                &resolved,
                "progression/lab",
                &["Always (Segmented)", "Never"],
            );
            choices.insert(resolved.get_str("progression/lab", ""));
        }
        assert_eq!(choices.len(), 2, "both choices are drawn");
    }

    #[test]
    fn a_seed_always_gives_the_same_configuration() {
        let definition = definition(&synthetic_map());
        let mut config = Config::defaults(&definition);
        config.set("player", "Random");
        config.set("progression/lab", "Random");
        let resolve_seed = |seed| resolve(&definition, &config, &mut Rng::named(seed, "config"));
        assert_eq!(resolve_seed(42), resolve_seed(42));
    }
}
