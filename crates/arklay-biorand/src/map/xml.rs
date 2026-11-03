//! Reads `biorand/map.xml` and each room's `random.xml` in the schema that
//! `map.rs` documents. Reading is strict: an unknown element or attribute, a
//! value that doesn't parse, and a reference to something that isn't there are
//! all errors, and each names its file, its room and its line.

use std::fmt::Display;
use std::str::FromStr;

use anyhow::{Result, anyhow, bail};
use roxmltree::{Document, Node};

use super::{
    BeginEnd, Door, DoorKind, Enemies, EnemyEntry, EnemyGroup, EnemyPose, Entrance, Filter,
    GraphFlag, Item, ItemDef, Map, Player, Requirement, Room, Spot,
};

/// The map-wide file, relative to the game's directory.
const MAP_FILE: &str = "biorand/map.xml";

impl Map {
    /// Reads the map-wide file and every room's file. `rooms` gives each room
    /// directory's name (`10c`) with the text of its `random.xml`, in the order
    /// the rooms keep.
    pub fn parse<'a>(
        map_xml: &str,
        rooms: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Result<Self> {
        let map_doc = parse_document(MAP_FILE, map_xml)?;
        let mut reader = Reader {
            map: Map::default(),
            links: Vec::new(),
        };
        reader.map_file(&map_doc)?;
        for (dir, text) in rooms {
            let file = format!("room/{dir}/random.xml");
            let doc = parse_document(&file, text)?;
            reader.room_file(&file, dir, &doc)?;
        }
        reader.check_links()?;
        Ok(reader.map)
    }
}

/// A reference to a room, to a door of one, or to a graph flag. These are
/// checked once every room is read, since one may be defined after the
/// reference that names it.
struct Link {
    /// Where the reference is, as an error prefix.
    at: String,
    /// What makes the reference, such as `target 30F:7`.
    what: String,
    target: Target,
}

/// What a reference names.
enum Target {
    Room(String),
    /// A door of a room: the room's key, and the door's id.
    Door(String, u8),
    Flag(String),
}

/// The map as it is read, with the references still to check.
struct Reader {
    map: Map,
    links: Vec<Link>,
}

impl Reader {
    fn map_file(&mut self, doc: &Document) -> Result<()> {
        let root = document_root(MAP_FILE, doc.root_element());
        if root.name() != "map" {
            let found = root.name();
            return Err(root.error(format!("expected the root element <map>, found <{found}>")));
        }
        for section in root.check(&[], &["begin-end", "items", "enemies"])? {
            match section.name() {
                "begin-end" => self.begin_end(section)?,
                "items" => self.items(section)?,
                "enemies" => self.enemies(section)?,
                _ => unreachable!("check() only lets through the names it is given"),
            }
        }
        Ok(())
    }

    fn begin_end(&mut self, el: Elem) -> Result<()> {
        el.check(&["start", "end", "tags", "door-rando", "player"], &[])?;
        let start = el.required("start")?;
        let end = el.required("end")?;
        self.links
            .push(el.link("begin-end start", Target::Room(start.to_string())));
        self.links
            .push(el.link("begin-end end", Target::Room(end.to_string())));
        self.map.begin_end.push(BeginEnd {
            start: start.to_string(),
            end: end.to_string(),
            filter: el.filter()?,
        });
        Ok(())
    }

    fn items(&mut self, el: Elem) -> Result<()> {
        for item in el.check(&[], &["item"])? {
            item.check(
                &[
                    "id", "name", "kind", "max", "ammo", "group", "amount", "discard",
                ],
                &[],
            )?;
            let id: u8 = item.required_number("id")?;
            if self.map.items.contains_key(&id) {
                return Err(item.error(format!("item {id} is defined twice")));
            }
            let def = ItemDef {
                name: item.required("name")?.to_string(),
                kind: item.required("kind")?.to_string(),
                ammo: item.list("ammo")?.unwrap_or_default(),
                max: item.number("max")?.unwrap_or_default(),
                group: item.group("group")?.unwrap_or_default(),
                amount: item.number("amount")?,
                discard: item.boolean("discard")?.unwrap_or_default(),
            };
            self.map.items.insert(id, def);
        }
        Ok(())
    }

    fn enemies(&mut self, el: Elem) -> Result<()> {
        for group in el.check(&[], &["group"])? {
            let mut entries: Vec<EnemyEntry> = Vec::new();
            for entry in group.check(&["name", "background", "foreground"], &["entry"])? {
                let poses = entry.check(&["key", "name", "id", "esp"], &["pose"])?;
                let key = entry.required("key")?;
                let duplicate =
                    self.enemy_key_exists(key) || entries.iter().any(|other| other.key == key);
                if duplicate {
                    return Err(entry.error(format!("enemy key \"{key}\" is defined twice")));
                }
                let mut pose_list = Vec::new();
                for pose in poses {
                    pose.check(&["name", "pose", "probability"], &[])?;
                    pose_list.push(EnemyPose {
                        name: pose.required("name")?.to_string(),
                        pose: pose.required_number("pose")?,
                        probability: pose.required_number("probability")?,
                    });
                }
                entries.push(EnemyEntry {
                    key: key.to_string(),
                    name: entry.required("name")?.to_string(),
                    ids: entry.required_list("id")?,
                    esp: entry.list("esp")?.unwrap_or_default(),
                    poses: pose_list,
                });
            }
            self.map.enemies.push(EnemyGroup {
                name: group.required("name")?.to_string(),
                background: group.required("background")?.to_string(),
                foreground: group.required("foreground")?.to_string(),
                entries,
            });
        }
        Ok(())
    }

    fn room_file(&mut self, file: &str, dir: &str, doc: &Document) -> Result<()> {
        let root = document_root(file, doc.root_element());
        if root.name() != "random" {
            let found = root.name();
            return Err(root.error(format!(
                "expected the root element <random>, found <{found}>"
            )));
        }
        for node in root.check(&[], &["room"])? {
            let room = self.room(dir, node)?;
            if self.map.rooms.contains_key(&room.key) {
                let key = &room.key;
                let outside = Elem { room: None, ..node };
                return Err(outside.error(format!("room \"{key}\" is defined twice")));
            }
            self.map.rooms.insert(room.key.clone(), room);
        }
        Ok(())
    }

    fn room(&mut self, dir: &str, el: Elem) -> Result<Room> {
        let key = el.required("key")?;
        check_room_key(el, dir, key)?;
        let el = Elem {
            room: Some(key),
            ..el
        };
        let children = el.check(
            &["key", "name", "tags", "door-rando", "player"],
            &["door", "flag", "item", "enemies", "spot"],
        )?;
        let mut room = Room {
            key: key.to_string(),
            name: el.value("name").map(str::to_string),
            filter: el.filter()?,
            ..Room::default()
        };
        for child in children {
            match child.name() {
                "door" => {
                    let door = self.door(child)?;
                    let repeated = door.id.filter(|id| {
                        room.doors.iter().any(|other| {
                            other.id == Some(*id) && overlaps(&other.filter, &door.filter)
                        })
                    });
                    if let Some(id) = repeated {
                        return Err(child.error(format!("door id {id} is used twice")));
                    }
                    room.doors.push(door);
                }
                "flag" => room.flags.push(self.flag(child)?),
                "item" => room.items.push(self.item(child)?),
                "enemies" => room.enemies.push(self.rule(child)?),
                "spot" => room.spots.push(self.spot(child)?),
                _ => unreachable!("check() only lets through the names it is given"),
            }
        }
        Ok(room)
    }

    fn door(&mut self, el: Elem) -> Result<Door> {
        let children = el.check(
            &[
                "id",
                "name",
                "target",
                "kind",
                "lock",
                "key",
                "allowed-locks",
                "no-unlock",
                "randomize",
                "requires",
                "ignore-in-graph",
                "tags",
                "door-rando",
                "player",
            ],
            &["entrance"],
        )?;
        let entrance = match children.as_slice() {
            [] => None,
            [entrance] => Some(entrance_at(*entrance)?),
            [_, second, ..] => return Err(second.error("a door has one entrance, not more")),
        };
        let id: Option<u8> = el.number("id")?;
        let target = el.value("target");
        if let Some(text) = target {
            self.target(el, text)?;
        }
        let kind = match el.value("kind") {
            None => None,
            Some(text) => Some(DoorKind::parse(text).ok_or_else(|| el.invalid("kind", text))?),
        };
        let key: Option<u8> = el.number("key")?;
        if let Some(key) = key {
            self.item_id(el, key, format!("key {key}"))?;
        }
        let allowed_locks: Option<Vec<u8>> = el.list("allowed-locks")?;
        if let Some(locks) = &allowed_locks {
            for lock in locks {
                self.item_id(el, *lock, format!("allowed lock {lock}"))?;
            }
        }
        Ok(Door {
            id,
            name: el.value("name").map(str::to_string),
            target: target.map(str::to_string),
            kind,
            lock: el.number("lock")?,
            key,
            allowed_locks,
            no_unlock: el.boolean("no-unlock")?.unwrap_or_default(),
            randomize: el.boolean("randomize")?,
            requires: self.requires(el)?,
            entrance,
            ignore_in_graph: el.boolean("ignore-in-graph")?.unwrap_or_default(),
            filter: el.filter()?,
        })
    }

    /// `10C:0` names a room and its door 0; `301` names a room, as a fixed exit.
    fn target(&mut self, el: Elem, text: &str) -> Result<()> {
        let (room, door) = match text.split_once(':') {
            None => (text, None),
            Some((room, door)) => {
                let door = door.parse::<u8>().map_err(|_| el.invalid("target", text))?;
                (room, Some(door))
            }
        };
        if room.is_empty() {
            return Err(el.invalid("target", text));
        }
        let target = match door {
            None => Target::Room(room.to_string()),
            Some(id) => Target::Door(room.to_string(), id),
        };
        self.links.push(el.link(format!("target {text}"), target));
        Ok(())
    }

    fn flag(&mut self, el: Elem) -> Result<GraphFlag> {
        el.check(&["name", "requires", "tags", "door-rando", "player"], &[])?;
        Ok(GraphFlag {
            name: el.required("name")?.to_string(),
            requires: self.requires(el)?,
            filter: el.filter()?,
        })
    }

    fn item(&mut self, el: Elem) -> Result<Item> {
        el.check(
            &[
                "id",
                "flag",
                "type",
                "amount",
                "group",
                "name",
                "requires",
                "document",
                "hidden",
                "optional",
                "lock",
                "tags",
                "door-rando",
                "player",
            ],
            &[],
        )?;
        // A room's item is an AOT id, which need not be in the catalogue:
        // staging's own data has pickups (ids 23, 131 to 137) outside it.
        let id: u8 = el.required_number("id")?;
        Ok(Item {
            id,
            flag: el.number("flag")?,
            kind: el.number("type")?,
            amount: el.number("amount")?,
            name: el.value("name").map(str::to_string),
            lock: el.number("lock")?,
            group: el.group("group")?.unwrap_or_default(),
            requires: self.requires(el)?,
            document: el.boolean("document")?.unwrap_or_default(),
            hidden: el.boolean("hidden")?.unwrap_or_default(),
            optional: el.boolean("optional")?.unwrap_or_default(),
            filter: el.filter()?,
        })
    }

    fn rule(&mut self, el: Elem) -> Result<Enemies> {
        el.check(
            &[
                "id",
                "flag",
                "kind",
                "include",
                "exclude",
                "max-difficulty",
                "restricted",
                "tags",
                "door-rando",
                "player",
            ],
            &[],
        )?;
        Ok(Enemies {
            id: el.number("id")?,
            flag: el.number("flag")?,
            kind: el.value("kind").map(str::to_string),
            include: el.list("include")?,
            exclude: el.list("exclude")?.unwrap_or_default(),
            max_difficulty: el.number("max-difficulty")?,
            restricted: el.boolean("restricted")?.unwrap_or_default(),
            filter: el.filter()?,
        })
    }

    fn spot(&mut self, el: Elem) -> Result<Spot> {
        el.check(
            &["x", "y", "z", "d", "include", "exclude", "placeholder"],
            &[],
        )?;
        let include = el.words("include");
        let exclude = el.words("exclude");
        for key in include.iter().chain(&exclude) {
            if !self.enemy_key_exists(key) {
                let message = format!("enemy key \"{key}\" is not in the enemy catalogue");
                return Err(el.error(message));
            }
        }
        Ok(Spot {
            x: el.required_number("x")?,
            y: el.required_number("y")?,
            z: el.required_number("z")?,
            d: el.required_number("d")?,
            include,
            exclude,
            placeholder: el.value("placeholder").map(str::to_string),
        })
    }

    /// The requirement atoms of `requires`: `item(52) flag(LIFT) room(513)`.
    fn requires(&mut self, el: Elem) -> Result<Vec<Requirement>> {
        let mut atoms = Vec::new();
        for word in el.value("requires").unwrap_or_default().split_whitespace() {
            let atom = Requirement::parse(word).ok_or_else(|| el.invalid("requires", word))?;
            match &atom {
                Requirement::Item(id) => self.item_id(el, *id, format!("requirement {word}"))?,
                Requirement::Room(room) => self
                    .links
                    .push(el.link(format!("requirement {word}"), Target::Room(room.clone()))),
                Requirement::Flag(name) => self
                    .links
                    .push(el.link(format!("requirement {word}"), Target::Flag(name.clone()))),
            }
            atoms.push(atom);
        }
        Ok(atoms)
    }

    /// Fails unless `id` is an item of the catalogue.
    fn item_id(&self, el: Elem, id: u8, what: impl Display) -> Result<()> {
        if self.map.items.contains_key(&id) {
            Ok(())
        } else {
            Err(el.error(format!("{what} is not in the item catalogue")))
        }
    }

    fn flag_exists(&self, name: &str) -> bool {
        self.map
            .rooms
            .values()
            .flat_map(|room| &room.flags)
            .any(|flag| flag.name == name)
    }

    fn enemy_key_exists(&self, key: &str) -> bool {
        self.map
            .enemies
            .iter()
            .flat_map(|group| &group.entries)
            .any(|entry| entry.key == key)
    }

    /// Fails on the first reference to a room, a door or a flag that isn't
    /// there.
    fn check_links(&self) -> Result<()> {
        for link in &self.links {
            let missing = match &link.target {
                Target::Room(room) if !self.map.rooms.contains_key(room) => {
                    format!("no room {room}")
                }
                Target::Door(room, id) => match self.map.rooms.get(room) {
                    None => format!("no room {room}"),
                    Some(found) if !found.doors.iter().any(|door| door.id == Some(*id)) => {
                        format!("no door {id} in room {room}")
                    }
                    Some(_) => continue,
                },
                Target::Flag(name) if !self.flag_exists(name) => format!("no flag {name}"),
                _ => continue,
            };
            bail!("{}{}: {missing}", link.at, link.what);
        }
        Ok(())
    }
}

/// An element being read, with the file and room its errors name.
#[derive(Clone, Copy)]
struct Elem<'a> {
    file: &'a str,
    room: Option<&'a str>,
    node: Node<'a, 'a>,
}

impl<'a> Elem<'a> {
    fn name(self) -> &'a str {
        self.node.tag_name().name()
    }

    fn location(self) -> String {
        location(self.file, self.room, self.node)
    }

    fn error(self, message: impl Display) -> anyhow::Error {
        anyhow!("{}{message}", self.location())
    }

    fn invalid(self, name: &str, value: &str) -> anyhow::Error {
        self.error(format!("invalid {name} \"{value}\" on <{}>", self.name()))
    }

    fn missing(self, name: &str) -> anyhow::Error {
        self.error(format!("missing attribute \"{name}\" on <{}>", self.name()))
    }

    fn link(self, what: impl Display, target: Target) -> Link {
        Link {
            at: self.location(),
            what: what.to_string(),
            target,
        }
    }

    /// Fails on an attribute not in `attributes`, a child element not in
    /// `children`, or text or a processing instruction. Returns the child
    /// elements in order. Comments may appear anywhere.
    fn check(self, attributes: &[&str], children: &[&str]) -> Result<Vec<Elem<'a>>> {
        for attr in self.node.attributes() {
            if !attributes.contains(&attr.name()) {
                let message = format!("unknown attribute \"{}\" on <{}>", attr.name(), self.name());
                return Err(self.error(message));
            }
        }
        let mut elements = Vec::new();
        for node in self.node.children() {
            if node.is_element() {
                let name = node.tag_name().name();
                if !children.contains(&name) {
                    let message = format!("unknown element <{name}> in <{}>", self.name());
                    return Err(Elem { node, ..self }.error(message));
                }
                elements.push(Elem { node, ..self });
            } else if node.is_text() {
                if node.text().is_some_and(|text| !text.trim().is_empty()) {
                    return Err(self.error(format!("unexpected text in <{}>", self.name())));
                }
            } else if !node.is_comment() {
                let message = format!("unexpected processing instruction in <{}>", self.name());
                return Err(self.error(message));
            }
        }
        Ok(elements)
    }

    fn value(self, name: &str) -> Option<&'a str> {
        self.node.attribute(name)
    }

    fn required(self, name: &str) -> Result<&'a str> {
        self.value(name).ok_or_else(|| self.missing(name))
    }

    fn number<T: FromStr>(self, name: &str) -> Result<Option<T>> {
        match self.value(name) {
            None => Ok(None),
            Some(text) => text.parse().map(Some).map_err(|_| self.invalid(name, text)),
        }
    }

    fn required_number<T: FromStr>(self, name: &str) -> Result<T> {
        self.number(name)?.ok_or_else(|| self.missing(name))
    }

    fn list<T: FromStr>(self, name: &str) -> Result<Option<Vec<T>>> {
        let Some(text) = self.value(name) else {
            return Ok(None);
        };
        let mut items = Vec::new();
        for word in text.split_whitespace() {
            items.push(word.parse::<T>().map_err(|_| self.invalid(name, word))?);
        }
        Ok(Some(items))
    }

    fn required_list<T: FromStr>(self, name: &str) -> Result<Vec<T>> {
        self.list(name)?.ok_or_else(|| self.missing(name))
    }

    fn boolean(self, name: &str) -> Result<Option<bool>> {
        match self.value(name) {
            None => Ok(None),
            Some("true") => Ok(Some(true)),
            Some("false") => Ok(Some(false)),
            Some(text) => Err(self.invalid(name, text)),
        }
    }

    /// A placement group. `-1` is staging's `GROUP_ALL`, every bit, and is
    /// stored as all the bits of a `u32`.
    fn group(self, name: &str) -> Result<Option<u32>> {
        match self.value(name) {
            None => Ok(None),
            Some("-1") => Ok(Some(u32::MAX)),
            Some(text) => text.parse().map(Some).map_err(|_| self.invalid(name, text)),
        }
    }

    fn words(self, name: &str) -> Vec<String> {
        self.value(name)
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_string)
            .collect()
    }

    fn filter(self) -> Result<Filter> {
        let player = match self.value("player") {
            None => None,
            Some("chris") => Some(Player::Chris),
            Some("jill") => Some(Player::Jill),
            Some(text) => return Err(self.invalid("player", text)),
        };
        Ok(Filter {
            tags: self.words("tags"),
            door_rando: self.boolean("door-rando")?,
            player,
        })
    }
}

/// Whether one game could include both filtered entries. Where both filters
/// name door randomization, or a player, they must name the same one: Chris's
/// and Jill's door 1 in room 201 never meet.
fn overlaps(a: &Filter, b: &Filter) -> bool {
    let door_rando =
        a.door_rando.is_none() || b.door_rando.is_none() || a.door_rando == b.door_rando;
    let player = a.player.is_none() || b.player.is_none() || a.player == b.player;
    door_rando && player
}

/// A door's `<entrance>`: where a player arriving through it stands.
fn entrance_at(el: Elem) -> Result<Entrance> {
    el.check(&["x", "y", "z", "d", "cut"], &[])?;
    Ok(Entrance {
        x: el.required_number("x")?,
        y: el.required_number("y")?,
        z: el.required_number("z")?,
        d: el.required_number("d")?,
        cut: el.number("cut")?,
    })
}

/// A room key is `10C` or `10C/BACK`, and its room belongs in the directory the
/// file is in: `10C` is in `room/10c`.
fn check_room_key(el: Elem, dir: &str, key: &str) -> Result<()> {
    let (room, part) = match key.split_once('/') {
        Some((room, part)) => (room, Some(part)),
        None => (key, None),
    };
    if room.is_empty() || part.is_some_and(|part| part.is_empty() || part.contains('/')) {
        return Err(el.error(format!("invalid room key \"{key}\"")));
    }
    if !room.eq_ignore_ascii_case(dir) {
        return Err(el.error(format!("room key \"{key}\" is not in room/{dir}")));
    }
    Ok(())
}

/// The root element of a file, from which every file is read.
fn document_root<'a>(file: &'a str, node: Node<'a, 'a>) -> Elem<'a> {
    Elem {
        file,
        room: None,
        node,
    }
}

/// Where a node is, as the prefix of an error: `file:line: room KEY: `.
fn location(file: &str, room: Option<&str>, node: Node) -> String {
    let line = node.document().text_pos_at(node.range().start).row;
    match room {
        Some(room) => format!("{file}:{line}: room {room}: "),
        None => format!("{file}:{line}: "),
    }
}

fn parse_document<'a>(file: &str, text: &'a str) -> Result<Document<'a>> {
    Document::parse(text).map_err(|err| anyhow!("{file}:{}: malformed XML: {err}", err.pos().row))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAP: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<map>
  <begin-end start="10C" end="10C"/>
  <items>
    <item id="52" name="Armor Key" kind="key/reusable" group="1" discard="true"/>
  </items>
  <enemies>
    <group name="Zombie" background="#66f" foreground="#fff">
      <entry key="zombie" name="Zombie" id="0 1 17" esp="3 4">
        <pose name="Walking" pose="0" probability="0.4"/>
      </entry>
    </group>
  </enemies>
</map>
"##;

    const GOOD_ROOMS: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<random>
  <room key="10C" name="Hall" tags="mansion mansion2able">
    <door id="0" name="TO BACK" target="10C/BACK:1" kind="locked" lock="11" key="52" allowed-locks="" no-unlock="true" randomize="false" requires="item(52) flag(LIFT)" ignore-in-graph="true" tags="segment-end" door-rando="true" player="jill">
      <entrance x="3000" y="0" z="3600" d="3072" cut="7"/>
    </door>
    <flag name="LIFT" requires="item(52)"/>
    <item id="52" flag="175" type="5" amount="6" group="-1" name="RED EYE" requires="room(10C/BACK)" document="true" hidden="true" optional="true" lock="35" player="chris" door-rando="false"/>
    <enemies id="0" flag="28" kind="npc" include="11 12" exclude="4 12" max-difficulty="0" restricted="true" player="chris"/>
    <spot x="11764" y="0" z="11442" d="2014" include="zombie" exclude="" placeholder="A"/>
  </room>
  <room key="10C/BACK">
    <door id="1" target="10C:0" kind="unblock"/>
  </room>
</random>
"##;

    /// The error for a map and one room file in directory `dir`.
    fn error_in(dir: &str, map: &str, room_file: &str) -> String {
        let result = Map::parse(map, [(dir, room_file)]);
        format!("{:#}", result.expect_err("the input should be rejected"))
    }

    fn error_for(map: &str, room_file: &str) -> String {
        error_in("10c", map, room_file)
    }

    /// A room file with one room, `10C`, holding `body`. The body starts on
    /// line 3.
    fn room(body: &str) -> String {
        format!("<random>\n  <room key=\"10C\" tags=\"mansion\">\n{body}\n  </room>\n</random>")
    }

    #[test]
    fn reads_a_good_file() {
        let map = Map::parse(MAP, [("10c", GOOD_ROOMS)]).unwrap();
        assert_eq!(
            map.begin_end,
            [BeginEnd {
                start: "10C".into(),
                end: "10C".into(),
                filter: Filter::default(),
            }]
        );
        let armor = &map.items[&52];
        assert_eq!(armor.name, "Armor Key");
        assert_eq!(armor.kind, "key/reusable");
        assert_eq!((armor.max, armor.group, armor.discard), (0, 1, true));
        let zombie = &map.enemies[0].entries[0];
        assert_eq!(zombie.ids, [0, 1, 17]);
        assert_eq!(zombie.esp, [3, 4]);
        assert_eq!(
            zombie.poses[0],
            EnemyPose {
                name: "Walking".into(),
                pose: 0,
                probability: 0.4,
            }
        );

        let keys: Vec<&str> = map.rooms.keys().map(String::as_str).collect();
        assert_eq!(keys, ["10C", "10C/BACK"]);
        let hall = &map.rooms["10C"];
        assert_eq!(hall.name.as_deref(), Some("Hall"));
        assert!(hall.filter.has_tag("mansion2able"));

        let door = &hall.doors[0];
        assert_eq!(door.target_room(), Some("10C/BACK"));
        assert_eq!(door.target_id(), Some(1));
        assert_eq!(door.kind, Some(DoorKind::Locked));
        assert_eq!((door.lock, door.key), (Some(11), Some(52)));
        assert_eq!(door.allowed_locks, Some(Vec::new()));
        assert_eq!(
            door.requires,
            [Requirement::Item(52), Requirement::Flag("LIFT".into())]
        );
        assert_eq!(
            door.entrance,
            Some(Entrance {
                x: 3000,
                y: 0,
                z: 3600,
                d: 3072,
                cut: Some(7),
            })
        );
        assert_eq!(door.randomize, Some(false));
        assert!(door.no_unlock && door.ignore_in_graph);
        assert_eq!(
            door.filter,
            Filter {
                tags: vec!["segment-end".into()],
                door_rando: Some(true),
                player: Some(Player::Jill),
            }
        );

        let flag = &hall.flags[0];
        assert_eq!(flag.name, "LIFT");
        assert_eq!(flag.requires, [Requirement::Item(52)]);

        let item = &hall.items[0];
        assert_eq!(
            (item.id, item.flag, item.kind, item.amount, item.lock),
            (52, Some(175), Some(5), Some(6), Some(35))
        );
        assert_eq!(item.group, u32::MAX);
        assert!(item.document && item.hidden && item.optional);
        assert_eq!(item.requires, [Requirement::Room("10C/BACK".into())]);
        assert_eq!(item.filter.player, Some(Player::Chris));
        assert_eq!(item.filter.door_rando, Some(false));

        let rule = &hall.enemies[0];
        assert_eq!(
            (rule.id, rule.flag, rule.kind.as_deref()),
            (Some(0), Some(28), Some("npc"))
        );
        assert_eq!(rule.include, Some(vec![11, 12]));
        assert_eq!(rule.exclude, [4, 12]);
        assert_eq!((rule.max_difficulty, rule.restricted), (Some(0), true));

        let spot = &hall.spots[0];
        assert_eq!((spot.x, spot.y, spot.z, spot.d), (11764, 0, 11442, 2014));
        assert_eq!(spot.include, ["zombie"]);
        assert!(spot.exclude.is_empty());
        assert_eq!(spot.placeholder.as_deref(), Some("A"));

        assert_eq!(map.rooms["10C/BACK"].doors[0].kind, Some(DoorKind::Unblock));
    }

    #[test]
    fn rejects_malformed_xml() {
        let err = error_for(MAP, "<random><room key=\"10C\">");
        assert!(
            err.starts_with("room/10c/random.xml:1: malformed XML: "),
            "{err}"
        );
    }

    #[test]
    fn rejects_an_unknown_element_in_a_room() {
        let err = error_for(MAP, &room("    <trap/>"));
        assert_eq!(
            err,
            "room/10c/random.xml:3: room 10C: unknown element <trap> in <room>"
        );
    }

    #[test]
    fn rejects_an_unknown_element_in_the_map() {
        let map = MAP.replace("  </items>", "    <thing/>\n  </items>");
        assert_eq!(
            error_for(&map, &room("")),
            "biorand/map.xml:6: unknown element <thing> in <items>"
        );
    }

    #[test]
    fn rejects_an_unknown_attribute() {
        let err = error_for(MAP, &room("    <flag name=\"LIFT\" bogus=\"1\"/>"));
        assert_eq!(
            err,
            "room/10c/random.xml:3: room 10C: unknown attribute \"bogus\" on <flag>"
        );
    }

    #[test]
    fn rejects_a_value_that_does_not_parse() {
        let err = error_for(MAP, &room("    <item id=\"x\" name=\"A\" type=\"5\"/>"));
        assert_eq!(
            err,
            "room/10c/random.xml:3: room 10C: invalid id \"x\" on <item>"
        );
        let err = error_for(MAP, &room("    <flag name=\"LIFT\" requires=\"item(x)\"/>"));
        assert_eq!(
            err,
            "room/10c/random.xml:3: room 10C: invalid requires \"item(x)\" on <flag>"
        );
    }

    #[test]
    fn rejects_a_missing_attribute() {
        let err = error_for(MAP, &room("    <flag requires=\"item(52)\"/>"));
        assert_eq!(
            err,
            "room/10c/random.xml:3: room 10C: missing attribute \"name\" on <flag>"
        );
    }

    #[test]
    fn rejects_a_room_outside_its_directory() {
        let err = error_in("10d", MAP, &room(""));
        assert_eq!(
            err,
            "room/10d/random.xml:2: room key \"10C\" is not in room/10d"
        );
    }

    #[test]
    fn rejects_a_room_defined_twice() {
        let result = Map::parse(MAP, [("10c", GOOD_ROOMS), ("10c", GOOD_ROOMS)]);
        let err = format!("{:#}", result.expect_err("a room is defined once"));
        assert_eq!(err, "room/10c/random.xml:3: room \"10C\" is defined twice");
    }

    #[test]
    fn rejects_a_wrong_root_element() {
        let err = error_for(MAP, "<rooms/>");
        assert_eq!(
            err,
            "room/10c/random.xml:1: expected the root element <random>, found <rooms>"
        );
    }

    #[test]
    fn rejects_a_door_target_with_no_room() {
        let err = error_for(MAP, &room("    <door id=\"0\" target=\"999:0\"/>"));
        assert_eq!(
            err,
            "room/10c/random.xml:3: room 10C: target 999:0: no room 999"
        );
    }

    #[test]
    fn rejects_a_door_target_with_no_door() {
        let err = error_for(MAP, &room("    <door id=\"0\" target=\"10C:5\"/>"));
        assert_eq!(
            err,
            "room/10c/random.xml:3: room 10C: target 10C:5: no door 5 in room 10C"
        );
    }

    #[test]
    fn rejects_a_requirement_on_an_item_that_is_not_catalogued() {
        let err = error_for(
            MAP,
            &room("    <flag name=\"LIFT\" requires=\"item(99)\"/>"),
        );
        assert_eq!(
            err,
            "room/10c/random.xml:3: room 10C: requirement item(99) is not in the item catalogue"
        );
    }

    #[test]
    fn rejects_a_door_key_that_is_not_catalogued() {
        let err = error_for(
            MAP,
            &room("    <door id=\"0\" target=\"10C:0\" key=\"99\"/>"),
        );
        assert_eq!(
            err,
            "room/10c/random.xml:3: room 10C: key 99 is not in the item catalogue"
        );
    }

    #[test]
    fn accepts_a_room_item_outside_the_catalogue() {
        let body = room("    <item id=\"99\" name=\"A\"/>");
        let map = Map::parse(MAP, [("10c", body.as_str())]).unwrap();
        assert_eq!(map.rooms["10C"].items[0].id, 99);
    }

    #[test]
    fn rejects_two_doors_with_one_id() {
        let body = "    <door id=\"0\" target=\"10C:0\"/>\n    <door id=\"0\" target=\"10C:0\"/>";
        assert_eq!(
            error_for(MAP, &room(body)),
            "room/10c/random.xml:4: room 10C: door id 0 is used twice"
        );
    }

    #[test]
    fn accepts_two_doors_with_one_id_for_different_players() {
        let body = "    <door id=\"0\" target=\"10C:0\" player=\"chris\"/>\n    <door id=\"0\" target=\"10C:0\" player=\"jill\"/>";
        let map = Map::parse(MAP, [("10c", room(body).as_str())]).unwrap();
        assert_eq!(map.rooms["10C"].doors.len(), 2);
    }

    #[test]
    fn accepts_two_doors_with_one_id_in_different_randomizations() {
        let body = "    <door id=\"0\" target=\"10C:0\" door-rando=\"true\"/>\n    <door id=\"0\" target=\"10C:0\" door-rando=\"false\"/>";
        let map = Map::parse(MAP, [("10c", room(body).as_str())]).unwrap();
        assert_eq!(map.rooms["10C"].doors.len(), 2);
    }

    #[test]
    fn rejects_two_doors_with_one_id_when_one_player_can_have_both() {
        let body = "    <door id=\"0\" target=\"10C:0\"/>\n    <door id=\"0\" target=\"10C:0\" player=\"jill\"/>";
        assert_eq!(
            error_for(MAP, &room(body)),
            "room/10c/random.xml:4: room 10C: door id 0 is used twice"
        );
    }

    #[test]
    fn rejects_a_requirement_on_a_flag_that_no_room_defines() {
        let err = error_for(
            MAP,
            &room("    <door id=\"0\" target=\"10C:0\" requires=\"flag(NOPE)\"/>"),
        );
        assert_eq!(
            err,
            "room/10c/random.xml:3: room 10C: requirement flag(NOPE): no flag NOPE"
        );
    }

    #[test]
    fn rejects_a_spot_naming_no_enemy() {
        let err = error_for(
            MAP,
            &room("    <spot x=\"1\" y=\"2\" z=\"3\" d=\"4\" include=\"ghost\"/>"),
        );
        assert_eq!(
            err,
            "room/10c/random.xml:3: room 10C: enemy key \"ghost\" is not in the enemy catalogue"
        );
    }

    #[test]
    fn rejects_a_begin_end_naming_no_room() {
        let map = MAP.replace("end=\"10C\"", "end=\"999\"");
        assert_eq!(
            error_for(&map, &room("")),
            "biorand/map.xml:3: begin-end end: no room 999"
        );
    }

    #[test]
    fn rejects_a_catalogue_item_defined_twice() {
        let map = MAP.replace(
            "  </items>",
            "    <item id=\"52\" name=\"Again\" kind=\"k\"/>\n  </items>",
        );
        assert_eq!(
            error_for(&map, &room("")),
            "biorand/map.xml:6: item 52 is defined twice"
        );
    }

    #[test]
    fn rejects_text_inside_an_element() {
        let err = error_for(MAP, &room("    <flag name=\"LIFT\">oops</flag>"));
        assert_eq!(
            err,
            "room/10c/random.xml:3: room 10C: unexpected text in <flag>"
        );
    }
}
