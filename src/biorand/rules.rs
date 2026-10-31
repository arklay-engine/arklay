//! A room's `random.xml`: what the generator may do with the room, so a route
//! is planned without reading the room's scripts.
//!
//! ```xml
//! <random>
//!   <room name="1F EAST SAVE" tags="start"/>
//!   <doors>
//!     <door id="0" target="10B:2" kind="normal">
//!       <entrance x="6000" y="0" z="8000" d="1024" cut="1"/>
//!     </door>
//!   </doors>
//!   <items><item id="1" name="CHEMICAL"/></items>
//!   <enemies exclude="4"><spot x="0" y="0" z="0" d="0" include="2 6"/></enemies>
//! </random>
//! ```
//!
//! A door's and an item's `id` are the AOT number of its `door_aot_set` or
//! `item_aot_set`. `target` is the door the original leads to, as a note for
//! whoever edits the file. An `entrance` is where a player arriving through
//! the door stands and the camera they see. `exclude` and `include` hold
//! enemy ids in hex. `<enemies fixed="true"/>` keeps the room's own enemies.
//! The `<room>` tags are `start`, `goal` and `visit2`, for a room whose scripts
//! run only on the second mansion visit.

use anyhow::{Context, Result, anyhow, bail};

/// Where a door puts a player arriving in the room.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entrance {
    pub x: i16,
    pub y: i16,
    pub z: i16,
    pub d: i16,
    pub cut: u8,
}

/// What kind of door a `<door>` is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoorKind {
    /// An ordinary door the generator can connect anywhere.
    Normal,
    /// Stairs, a ladder, a lift, a hidden door or an already sealed one: its
    /// animation or script suits only the original's route, so the generator
    /// seals it.
    Fixed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoorRule {
    pub aot: u8,
    pub kind: DoorKind,
    pub entrance: Option<Entrance>,
}

impl DoorRule {
    /// Whether the generator may connect this door.
    pub fn connectable(&self) -> bool {
        self.kind == DoorKind::Normal && self.entrance.is_some()
    }
}

/// Where one enemy may stand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spot {
    pub x: i16,
    pub y: i16,
    pub z: i16,
    pub d: i16,
    pub exclude: Vec<u8>,
    pub include: Vec<u8>,
}

impl Spot {
    fn allows(&self, enemy: u8) -> bool {
        !self.exclude.contains(&enemy) && (self.include.is_empty() || self.include.contains(&enemy))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnemyRules {
    /// Keep the room's own enemies.
    pub fixed: bool,
    pub exclude: Vec<u8>,
    pub spots: Vec<Spot>,
}

impl EnemyRules {
    /// The spots `enemy` may stand on, or none if the room excludes it.
    pub fn spots_for(&self, enemy: u8) -> impl Iterator<Item = (usize, &Spot)> {
        let allowed = !self.exclude.contains(&enemy);
        self.spots
            .iter()
            .enumerate()
            .filter(move |(_, spot)| allowed && spot.allows(enemy))
    }
}

/// One room's `random.xml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomRules {
    /// Stage digit and room number, e.g. `0x21A`.
    pub room: u16,
    pub name: String,
    pub start: bool,
    pub goal: bool,
    /// The original has this room only on the second mansion visit, so its
    /// scripts need their visit tests dropped.
    pub second_visit: bool,
    pub doors: Vec<DoorRule>,
    /// The AOT numbers of the room's item slots.
    pub items: Vec<u8>,
    pub enemies: EnemyRules,
}

impl RoomRules {
    pub fn connectable_doors(&self) -> impl Iterator<Item = &DoorRule> {
        self.doors.iter().filter(|door| door.connectable())
    }

    pub fn door(&self, aot: u8) -> Option<&DoorRule> {
        self.doors.iter().find(|door| door.aot == aot)
    }
}

fn attribute<T: std::str::FromStr>(node: roxmltree::Node<'_, '_>, name: &str) -> Result<T> {
    let text = node
        .attribute(name)
        .ok_or_else(|| anyhow!("<{}> has no `{name}`", node.tag_name().name()))?;
    text.parse()
        .map_err(|_| anyhow!("<{}> has a bad `{name}`: {text}", node.tag_name().name()))
}

fn enemy_list(node: roxmltree::Node<'_, '_>, name: &str) -> Result<Vec<u8>> {
    node.attribute(name)
        .unwrap_or("")
        .split_ascii_whitespace()
        .map(|id| u8::from_str_radix(id, 16).with_context(|| format!("bad enemy id `{id}`")))
        .collect()
}

fn child<'a, 'input>(
    root: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Option<roxmltree::Node<'a, 'input>> {
    root.children().find(|node| node.has_tag_name(name))
}

fn children<'a, 'input>(
    parent: Option<roxmltree::Node<'a, 'input>>,
    name: &'a str,
) -> impl Iterator<Item = roxmltree::Node<'a, 'input>> {
    parent
        .into_iter()
        .flat_map(|node| node.children())
        .filter(move |node| node.has_tag_name(name))
}

fn door(node: roxmltree::Node<'_, '_>) -> Result<DoorRule> {
    let kind = match node.attribute("kind").unwrap_or("normal") {
        "normal" => DoorKind::Normal,
        "stairs" | "ladder" | "lift" | "hidden" | "sealed" => DoorKind::Fixed,
        other => bail!("unknown door kind `{other}`"),
    };
    let entrance = child(node, "entrance")
        .map(|entrance| {
            Ok::<_, anyhow::Error>(Entrance {
                x: attribute(entrance, "x")?,
                y: attribute(entrance, "y")?,
                z: attribute(entrance, "z")?,
                d: attribute(entrance, "d")?,
                cut: attribute(entrance, "cut")?,
            })
        })
        .transpose()?;
    Ok(DoorRule {
        aot: attribute(node, "id")?,
        kind,
        entrance,
    })
}

fn spot(node: roxmltree::Node<'_, '_>) -> Result<Spot> {
    Ok(Spot {
        x: attribute(node, "x")?,
        y: attribute(node, "y")?,
        z: attribute(node, "z")?,
        d: attribute(node, "d")?,
        exclude: enemy_list(node, "exclude")?,
        include: enemy_list(node, "include")?,
    })
}

/// Parse the `random.xml` of room `room`.
pub fn parse(room: u16, text: &str) -> Result<RoomRules> {
    let document = roxmltree::Document::parse(text).context("not XML")?;
    let root = document.root_element();
    if !root.has_tag_name("random") {
        bail!("the root element is not <random>");
    }
    let header = child(root, "room");
    let name = header
        .and_then(|node| node.attribute("name"))
        .unwrap_or_default()
        .to_string();
    let has_tag = |tag: &str| {
        header
            .and_then(|node| node.attribute("tags"))
            .is_some_and(|tags| tags.split_ascii_whitespace().any(|t| t == tag))
    };
    let doors = children(child(root, "doors"), "door")
        .map(door)
        .collect::<Result<Vec<_>>>()?;
    let items = children(child(root, "items"), "item")
        .map(|node| attribute(node, "id"))
        .collect::<Result<Vec<u8>>>()?;
    let enemies = match child(root, "enemies") {
        Some(node) => EnemyRules {
            fixed: node.attribute("fixed") == Some("true"),
            exclude: enemy_list(node, "exclude")?,
            spots: children(Some(node), "spot")
                .map(spot)
                .collect::<Result<Vec<_>>>()?,
        },
        None => EnemyRules::default(),
    };
    Ok(RoomRules {
        room,
        name,
        start: has_tag("start"),
        goal: has_tag("goal"),
        second_visit: has_tag("visit2"),
        doors,
        items,
        enemies,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = r#"<random>
        <room name="A ROOM" tags="start"/>
        <doors>
          <door id="0" target="10B:2" kind="normal">
            <entrance x="6000" y="0" z="8000" d="1024" cut="1"/>
          </door>
          <door id="3" kind="stairs"/>
        </doors>
        <items><item id="1" name="CHEMICAL"/></items>
        <enemies exclude="4">
          <spot x="1" y="2" z="3" d="4" include="2 6"/>
          <spot x="5" y="6" z="7" d="8" exclude="2"/>
        </enemies>
    </random>"#;

    #[test]
    fn parses_every_group() {
        let rules = parse(0x118, TEXT).unwrap();
        assert_eq!(rules.name, "A ROOM");
        assert!(rules.start);
        assert!(!rules.goal);
        assert_eq!(rules.items, [1]);
        assert_eq!(rules.connectable_doors().count(), 1);
        assert_eq!(rules.door(0).unwrap().entrance.unwrap().cut, 1);
        assert!(!rules.door(3).unwrap().connectable());
    }

    #[test]
    fn spots_honour_room_and_spot_lists() {
        let rules = parse(0x118, TEXT).unwrap();
        let spots = |enemy| {
            rules
                .enemies
                .spots_for(enemy)
                .map(|(index, _)| index)
                .collect::<Vec<_>>()
        };
        assert_eq!(spots(4), Vec::<usize>::new());
        assert_eq!(spots(2), [0]);
        assert_eq!(spots(6), [0, 1]);
        assert_eq!(spots(0), [1]);
    }

    #[test]
    fn rejects_an_unknown_door_kind() {
        let text = r#"<random><doors><door id="0" kind="portal"/></doors></random>"#;
        assert!(parse(0x118, text).is_err());
    }
}
