//! The bot's per-game combat knowledge: `knowledge/<game>.xml`.
//!
//! One file per base game describes its enemies (fight, pass or avoid, the
//! aim pose that hits them, the distance to keep, their reach and whether they
//! grab) and its weapons (the aim cone, the cadence, the shotgun's headshot
//! distance). The Lua planner reads it through `bot:knowledge()`, keyed by
//! the enemy and weapon ids the snapshot reports, so the Lua holds no id.
//!
//! The file is found by game id: the merged pack's own manifest id first,
//! then the `base` of each mod layer (the bot mod names `re1`), so a
//! randomised pack generated from RE1 reads `knowledge/re1.xml` too.
//! Values that were not measured are written `unknown` and reach Lua as nil.
//! The parser refuses unknown elements and attributes.

use anyhow::{Context, Result, bail};
use roxmltree::{Document, Node};

use super::meta::{at, attr, attr_text, check_attributes, check_leaf, elements, integer};
use crate::budget;
use crate::pack::Pack;

/// The newest `<knowledge format="...">` this parser reads.
pub const FORMAT: u32 = 1;

const MAX_BYTES: usize = 1 << 20;

/// What the bot does about an enemy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    /// Fight it when the engagement rule allows.
    Fight,
    /// Never stop for it: walk on around it.
    Pass,
    /// Keep away from it; never engage.
    Avoid,
}

impl Policy {
    fn name(self) -> &'static str {
        match self {
            Self::Fight => "fight",
            Self::Pass => "pass",
            Self::Avoid => "avoid",
        }
    }
}

/// An aim pose: the pad direction held with aim, and the status bit of the
/// enemy it must match (`apply_weapon_damage_with`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pose {
    /// No direction: aim flag 0x40.
    Torso,
    /// Down held: aim flag 0x20.
    Down,
    /// Up held: aim flag 0x80.
    Up,
}

impl Pose {
    fn name(self) -> &'static str {
        match self {
            Self::Torso => "torso",
            Self::Down => "down",
            Self::Up => "up",
        }
    }
}

/// Which weapons an aim-up shot can hit the enemy with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpRule {
    Any,
    /// Only the shotgun (`detect_gun` rejects the other guns).
    Shotgun,
    Never,
}

/// The distance band to hold from an enemy with one weapon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Keep {
    pub weapon: u8,
    pub min: i32,
    pub max: i32,
    /// Health one hit takes off, when known.
    pub damage: Option<i32>,
    /// Rounds of the weapon (every loading) the band leaves held: a reserve
    /// gun opens such a fight only with more than these.
    pub spare: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Enemy {
    pub id: u8,
    pub name: String,
    pub policy: Policy,
    /// The pose that hits it standing or walking; `None` when unknown.
    pub stand: Option<Pose>,
    /// The pose that hits it downed or crawling; `None` when it has no such
    /// state or it is unknown.
    pub downed: Option<Pose>,
    pub up: Option<UpRule>,
    /// The distance at which it starts an attack.
    pub reach: Option<i32>,
    /// The same for a downed, crawling one (optional attribute).
    pub crawl_reach: Option<i32>,
    pub grabs: Option<bool>,
    /// Whether Jill's run is faster than its chase.
    pub outrun: Option<bool>,
    /// Its chase speed, units a tick.
    pub speed: Option<i32>,
    pub health: Option<i32>,
    /// A boss made of parts (Plant 42's body and vines): a shot hits the
    /// free part nearest the aim, so the bot fires on through its target's
    /// own hit reaction (optional attribute, default no).
    pub parts: bool,
    /// The weapon to fight it with whenever it is held with rounds (a
    /// hunter takes the shotgun), over the equipped one; it must have a
    /// `<keep>` (optional attribute).
    pub prefer: Option<u8>,
    /// The weapon that opens the fight while the `prefer`red one is not
    /// held (303's Tyrant: the grenade launcher takes it to the 200 health
    /// where Brad drops the rocket launcher); it must have a `<keep>`
    /// (optional attribute).
    pub open: Option<u8>,
    /// A torso or up shotgun hit inside the weapon's `headshot` distance
    /// pops its head (zombie_reaction: the three zombies), so the bot walks
    /// in for it (optional attribute).
    pub head: bool,
    /// Launcher rounds kept for this enemy's fight while it is undone: no
    /// other fight spends them (303's Tyrant needs four before Brad drops
    /// the rocket launcher, and its launcher damage is left `unknown`)
    /// (optional attribute; needs `open`).
    pub save: Option<i32>,
    pub keeps: Vec<Keep>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Weapon {
    pub id: u8,
    pub name: String,
    /// The aim cone starts this far ahead of the player.
    pub near: i32,
    /// The cone's half width beyond the enemy's radius.
    pub side: i32,
    /// Ticks from one shot to the next.
    pub cadence: Option<i32>,
    /// Inside this distance a torso or up shot pops a zombie's head.
    pub headshot: Option<i32>,
    /// Beyond this distance a hit may do less damage.
    pub weak: Option<i32>,
    /// Whether the aim direction must match the enemy's status bit.
    pub status_bit: bool,
    /// A hand weapon (the knife): no rounds, so it never runs dry, and it
    /// is held only for an enemy that `prefer`s it.
    pub melee: bool,
    /// The staple firearm (the handgun): its ammunition is the most common,
    /// so a full inventory never boxes it to make room.
    pub staple: bool,
    /// A reserve weapon (the grenade launcher): its rounds are scarce, so it
    /// is picked only for an enemy that `prefer`s it or when no other gun
    /// covers the kill.
    pub reserve: bool,
    /// The weapon this one is another loading of (the grenade launcher
    /// with acid or flame rounds is item 0x08 or 0x09, the explosive one
    /// 0x07): a fight that prefers either takes the other.
    pub variant_of: Option<u8>,
}

/// The poison a `<cure>` item clears: the engine's health-status bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Poison {
    /// The `0x02` drain (a blue herb's `cure_poison`).
    Poison,
    /// The `0x20` poison (the serum's `cure_serious_poison`).
    Serious,
}

impl Poison {
    fn name(self) -> &'static str {
        match self {
            Self::Poison => "poison",
            Self::Serious => "serious",
        }
    }
}

/// `<cure item poison heals>`: an item whose menu USE clears that poison,
/// and whether it also restores health.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cure {
    pub item: u8,
    pub poison: Poison,
    pub heals: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Knowledge {
    /// The pack path it was read from.
    pub source: String,
    pub enemies: Vec<Enemy>,
    pub weapons: Vec<Weapon>,
    /// Poison cures in document order (the planner prefers the first held).
    pub cures: Vec<Cure>,
}

/// The pack path of a game's knowledge file.
pub fn path_for(game: &str) -> String {
    format!("knowledge/{game}.xml")
}

/// Load the knowledge file for the pack's game, or `None` when the pack
/// carries none.
pub fn load(pack: &Pack) -> Result<Option<Knowledge>> {
    let mut games: Vec<String> = Vec::new();
    if let Some(manifest) = pack.manifest() {
        games.push(manifest.id.clone());
        games.extend(manifest.base.clone());
    }
    for layer in pack.overrides() {
        if let Some(manifest) = layer.manifest() {
            games.extend(manifest.base.clone());
        }
    }
    for game in games {
        let path = path_for(&game);
        if !pack.contains(&path) {
            continue;
        }
        let bytes = pack
            .read(&path)
            .with_context(|| format!("cannot read {path}"))?;
        let text =
            std::str::from_utf8(&bytes).with_context(|| format!("{path} is not valid UTF-8"))?;
        let mut knowledge = parse(text).with_context(|| path.clone())?;
        knowledge.source = path;
        return Ok(Some(knowledge));
    }
    Ok(None)
}

/// Parse one knowledge document.
pub fn parse(text: &str) -> Result<Knowledge> {
    budget::check_len(text.len(), MAX_BYTES, "bot knowledge XML size")?;
    let document = Document::parse(text).map_err(|err| {
        anyhow::anyhow!(
            "knowledge XML is not well-formed at line {}: {err}",
            err.pos().row
        )
    })?;
    let root = document.root_element();
    if root.tag_name().name() != "knowledge" {
        bail!("{}: the root must be <knowledge>", at(root));
    }
    check_attributes(root, &["format", "game"])?;
    let format: u32 = attr(root, "format")?;
    if format != FORMAT {
        bail!("{}: knowledge format {format} is not {FORMAT}", at(root));
    }
    let mut knowledge = Knowledge::default();
    budget::check_len(
        elements(root).count(),
        budget::MAX_RECORDS,
        "knowledge element count",
    )?;
    for child in elements(root) {
        match child.tag_name().name() {
            "enemy" => {
                let enemy = parse_enemy(child)?;
                if knowledge.enemies.iter().any(|e| e.id == enemy.id) {
                    bail!("{}: duplicate enemy id {:#04x}", at(child), enemy.id);
                }
                knowledge.enemies.push(enemy);
            }
            "weapon" => {
                let weapon = parse_weapon(child)?;
                if knowledge.weapons.iter().any(|w| w.id == weapon.id) {
                    bail!("{}: duplicate weapon id {:#04x}", at(child), weapon.id);
                }
                knowledge.weapons.push(weapon);
            }
            "cure" => {
                let cure = parse_cure(child)?;
                if knowledge.cures.iter().any(|c| c.item == cure.item) {
                    bail!("{}: duplicate cure item {:#04x}", at(child), cure.item);
                }
                knowledge.cures.push(cure);
            }
            other => bail!("{}: unknown <knowledge> element <{other}>", at(child)),
        }
    }
    for enemy in &knowledge.enemies {
        for keep in &enemy.keeps {
            if !knowledge.weapons.iter().any(|w| w.id == keep.weapon) {
                bail!(
                    "enemy {:#04x} keeps a distance for weapon {:#04x}, which has no <weapon>",
                    enemy.id,
                    keep.weapon
                );
            }
        }
    }
    Ok(knowledge)
}

/// A non-negative distance, or `unknown`.
fn distance(node: Node<'_, '_>, name: &str) -> Result<Option<i32>> {
    let raw = attr_text(node, name)?;
    if raw == "unknown" || raw == "none" {
        return Ok(None);
    }
    match integer(raw).and_then(|v| i32::try_from(v).ok()) {
        Some(value) if value >= 0 => Ok(Some(value)),
        _ => bail!(
            "{}: `{name}` value `{raw}` is not a distance, `none` or `unknown`",
            at(node)
        ),
    }
}

fn pose(node: Node<'_, '_>, name: &str) -> Result<Option<Pose>> {
    Ok(match attr_text(node, name)? {
        "torso" => Some(Pose::Torso),
        "down" => Some(Pose::Down),
        "up" => Some(Pose::Up),
        "unknown" | "none" => None,
        other => bail!(
            "{}: `{name}` value `{other}` is not torso, down, up, none or unknown",
            at(node)
        ),
    })
}

fn yes_no(node: Node<'_, '_>, name: &str) -> Result<Option<bool>> {
    Ok(match attr_text(node, name)? {
        "yes" => Some(true),
        "no" => Some(false),
        "unknown" => None,
        other => bail!(
            "{}: `{name}` value `{other}` is not yes, no or unknown",
            at(node)
        ),
    })
}

fn name_attr(node: Node<'_, '_>) -> Result<String> {
    let name = attr_text(node, "name")?.trim().to_string();
    if name.is_empty() {
        bail!("{}: `name` must not be empty", at(node));
    }
    Ok(name)
}

fn parse_enemy(node: Node<'_, '_>) -> Result<Enemy> {
    check_attributes(
        node,
        &[
            "id",
            "name",
            "policy",
            "stand",
            "downed",
            "up",
            "reach",
            "crawl_reach",
            "grabs",
            "outrun",
            "speed",
            "health",
            "parts",
            "head",
            "prefer",
            "open",
            "save",
        ],
    )?;
    let policy = match attr_text(node, "policy")? {
        "fight" => Policy::Fight,
        "pass" => Policy::Pass,
        "avoid" => Policy::Avoid,
        other => bail!(
            "{}: `policy` value `{other}` is not fight, pass or avoid",
            at(node)
        ),
    };
    let up = match attr_text(node, "up")? {
        "yes" => Some(UpRule::Any),
        "shotgun" => Some(UpRule::Shotgun),
        "no" => Some(UpRule::Never),
        "unknown" => None,
        other => bail!(
            "{}: `up` value `{other}` is not yes, shotgun, no or unknown",
            at(node)
        ),
    };
    let mut keeps = Vec::new();
    for child in elements(node) {
        if child.tag_name().name() != "keep" {
            bail!(
                "{}: unknown <enemy> element <{}>",
                at(child),
                child.tag_name().name()
            );
        }
        check_attributes(child, &["weapon", "min", "max", "damage", "spare"])?;
        check_leaf(child)?;
        let keep = Keep {
            weapon: attr(child, "weapon")?,
            min: attr(child, "min")?,
            max: attr(child, "max")?,
            damage: distance(child, "damage")?,
            spare: child
                .attribute("spare")
                .map(|_| attr(child, "spare"))
                .transpose()?,
        };
        if keep.spare.is_some_and(|spare| spare < 0) {
            bail!("{}: needs spare >= 0", at(child));
        }
        if keep.min < 0 || keep.max <= keep.min {
            bail!("{}: needs 0 <= min < max", at(child));
        }
        if keeps.iter().any(|k: &Keep| k.weapon == keep.weapon) {
            bail!("{}: duplicate weapon {:#04x}", at(child), keep.weapon);
        }
        keeps.push(keep);
    }
    let prefer: Option<u8> = node
        .attribute("prefer")
        .map(|_| attr(node, "prefer"))
        .transpose()?;
    if let Some(weapon) = prefer
        && !keeps.iter().any(|keep| keep.weapon == weapon)
    {
        bail!("{}: `prefer` weapon {weapon:#04x} has no <keep>", at(node));
    }
    let open: Option<u8> = node
        .attribute("open")
        .map(|_| attr(node, "open"))
        .transpose()?;
    if let Some(weapon) = open
        && !keeps.iter().any(|keep| keep.weapon == weapon)
    {
        bail!("{}: `open` weapon {weapon:#04x} has no <keep>", at(node));
    }
    let save = match node.attribute("save") {
        Some(_) => distance(node, "save")?,
        None => None,
    };
    if save.is_some() && open.is_none() {
        bail!("{}: `save` needs an `open` weapon", at(node));
    }
    Ok(Enemy {
        id: attr(node, "id")?,
        name: name_attr(node)?,
        policy,
        stand: pose(node, "stand")?,
        downed: pose(node, "downed")?,
        up,
        reach: distance(node, "reach")?,
        crawl_reach: match node.attribute("crawl_reach") {
            Some(_) => distance(node, "crawl_reach")?,
            None => None,
        },
        grabs: yes_no(node, "grabs")?,
        outrun: yes_no(node, "outrun")?,
        speed: distance(node, "speed")?,
        health: distance(node, "health")?,
        parts: match node.attribute("parts") {
            Some(_) => yes_no(node, "parts")? == Some(true),
            None => false,
        },
        head: match node.attribute("head") {
            Some(_) => yes_no(node, "head")? == Some(true),
            None => false,
        },
        prefer,
        open,
        save,
        keeps,
    })
}

fn parse_cure(node: Node<'_, '_>) -> Result<Cure> {
    check_attributes(node, &["item", "poison", "heals"])?;
    check_leaf(node)?;
    let item: u8 = attr(node, "item")?;
    if item == 0 {
        bail!("{}: <cure> `item` must not be 0 (no item)", at(node));
    }
    let poison = match attr_text(node, "poison")? {
        "poison" => Poison::Poison,
        "serious" => Poison::Serious,
        other => bail!(
            "{}: `poison` value `{other}` is not poison or serious",
            at(node)
        ),
    };
    let Some(heals) = yes_no(node, "heals")? else {
        bail!("{}: `heals` must be yes or no", at(node));
    };
    Ok(Cure {
        item,
        poison,
        heals,
    })
}

fn parse_weapon(node: Node<'_, '_>) -> Result<Weapon> {
    check_attributes(
        node,
        &[
            "id",
            "name",
            "near",
            "side",
            "cadence",
            "headshot",
            "weak",
            "status_bit",
            "melee",
            "staple",
            "reserve",
            "variant_of",
        ],
    )?;
    check_leaf(node)?;
    Ok(Weapon {
        id: attr(node, "id")?,
        name: name_attr(node)?,
        near: attr(node, "near")?,
        side: attr(node, "side")?,
        cadence: distance(node, "cadence")?,
        headshot: distance(node, "headshot")?,
        weak: distance(node, "weak")?,
        status_bit: yes_no(node, "status_bit")?.unwrap_or(true),
        melee: match node.attribute("melee") {
            Some(_) => yes_no(node, "melee")? == Some(true),
            None => false,
        },
        staple: match node.attribute("staple") {
            Some(_) => yes_no(node, "staple")? == Some(true),
            None => false,
        },
        reserve: match node.attribute("reserve") {
            Some(_) => yes_no(node, "reserve")? == Some(true),
            None => false,
        },
        variant_of: match node.attribute("variant_of") {
            Some(_) => Some(attr(node, "variant_of")?),
            None => None,
        },
    })
}

impl Knowledge {
    /// The `bot:knowledge()` table: `{source, enemies = {[id] = row},
    /// weapons = {[id] = row}}`. Unknown values are absent (nil).
    pub fn to_lua(&self, lua: &mlua::Lua) -> mlua::Result<mlua::Table> {
        let table = lua.create_table()?;
        table.set("source", self.source.as_str())?;
        let enemies = lua.create_table()?;
        for enemy in &self.enemies {
            let row = lua.create_table()?;
            row.set("id", enemy.id)?;
            row.set("name", enemy.name.as_str())?;
            row.set("policy", enemy.policy.name())?;
            row.set("stand", enemy.stand.map(Pose::name))?;
            row.set("downed", enemy.downed.map(Pose::name))?;
            row.set(
                "up",
                enemy.up.map(|rule| match rule {
                    UpRule::Any => "yes",
                    UpRule::Shotgun => "shotgun",
                    UpRule::Never => "no",
                }),
            )?;
            row.set("reach", enemy.reach)?;
            row.set("crawl_reach", enemy.crawl_reach)?;
            row.set("grabs", enemy.grabs)?;
            row.set("outrun", enemy.outrun)?;
            row.set("speed", enemy.speed)?;
            row.set("health", enemy.health)?;
            row.set("parts", enemy.parts)?;
            row.set("prefer", enemy.prefer)?;
            row.set("open", enemy.open)?;
            row.set("head", enemy.head)?;
            row.set("save", enemy.save)?;
            let keeps = lua.create_table()?;
            for keep in &enemy.keeps {
                let band = lua.create_table()?;
                band.set("min", keep.min)?;
                band.set("max", keep.max)?;
                band.set("damage", keep.damage)?;
                band.set("spare", keep.spare)?;
                keeps.set(keep.weapon, band)?;
            }
            row.set("keep", keeps)?;
            enemies.set(enemy.id, row)?;
        }
        table.set("enemies", enemies)?;
        let weapons = lua.create_table()?;
        for weapon in &self.weapons {
            let row = lua.create_table()?;
            row.set("id", weapon.id)?;
            row.set("name", weapon.name.as_str())?;
            row.set("near", weapon.near)?;
            row.set("side", weapon.side)?;
            row.set("cadence", weapon.cadence)?;
            row.set("headshot", weapon.headshot)?;
            row.set("weak", weapon.weak)?;
            row.set("status_bit", weapon.status_bit)?;
            row.set("melee", weapon.melee)?;
            row.set("staple", weapon.staple)?;
            row.set("reserve", weapon.reserve)?;
            row.set("variant_of", weapon.variant_of)?;
            weapons.set(weapon.id, row)?;
        }
        table.set("weapons", weapons)?;
        let cures = lua.create_table()?;
        for cure in &self.cures {
            let row = lua.create_table()?;
            row.set("item", cure.item)?;
            row.set("poison", cure.poison.name())?;
            row.set("heals", cure.heals)?;
            cures.push(row)?;
        }
        table.set("cures", cures)?;
        Ok(table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn the_checked_in_re1_knowledge_parses() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mods/bot/knowledge/re1.xml");
        let text = std::fs::read_to_string(&path).unwrap();
        let knowledge = parse(&text).unwrap();
        assert!(knowledge.weapons.iter().any(|w| w.name == "handgun"));
        assert!(knowledge.weapons.iter().any(|w| w.name == "shotgun"));
        assert!(
            knowledge
                .weapons
                .iter()
                .any(|w| w.name == "knife" && w.melee)
        );
        assert!(knowledge.enemies.len() >= 5);
    }

    /// Each `<cure>` names an item whose engine definition clears that
    /// poison, and every engine cure item of the game is listed.
    #[test]
    fn the_re1_cures_match_the_item_catalog() {
        use crate::item_catalog::StatusCure;
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mods/bot/knowledge/re1.xml");
        let knowledge = parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let catalog = crate::item_catalog::ItemCatalog::embedded();
        let mut listed = 0;
        for item in 0..=u8::MAX {
            let heal = catalog.get(item).heal;
            let heals = heal.amount != crate::item_catalog::HealAmount::None;
            let want = match heal.cure {
                StatusCure::None => None,
                StatusCure::Poison02 => Some((Poison::Poison, heals)),
                StatusCure::Poison20 => Some((Poison::Serious, heals)),
            };
            let have = knowledge
                .cures
                .iter()
                .find(|c| c.item == item)
                .map(|c| (c.poison, c.heals));
            assert_eq!(have, want, "item {item:#04x}");
            listed += usize::from(want.is_some());
        }
        assert_eq!(listed, knowledge.cures.len());
        assert!(listed >= 2);
    }

    #[test]
    fn cures_parse_and_reject_typos() {
        let text = r#"<knowledge format="1">
            <cure item="0x45" poison="poison" heals="no"/>
            <cure item="0x42" poison="serious" heals="no"/>
        </knowledge>"#;
        let k = parse(text).unwrap();
        assert_eq!(
            k.cures,
            vec![
                Cure {
                    item: 0x45,
                    poison: Poison::Poison,
                    heals: false,
                },
                Cure {
                    item: 0x42,
                    poison: Poison::Serious,
                    heals: false,
                },
            ]
        );
        assert!(parse(&text.replace("poison=\"serious\"", "poison=\"gas\"")).is_err());
        assert!(parse(&text.replace("item=\"0x42\"", "item=\"0x45\"")).is_err());
        assert!(parse(&text.replace("item=\"0x42\"", "item=\"0\"")).is_err());
        assert!(parse(&text.replace("heals=\"no\"/>", "heals=\"no\" n=\"1\"/>")).is_err());
        assert!(parse(&text.replace("heals=\"no\"", "heals=\"unknown\"")).is_err());
    }

    #[test]
    fn unknown_values_are_none_and_typos_fail() {
        let text = r#"<knowledge format="1">
            <weapon id="2" name="gun" near="650" side="1300" cadence="unknown"
                headshot="none" weak="none" status_bit="yes"/>
            <enemy id="5" name="crow" policy="pass" stand="unknown" downed="none"
                up="unknown" reach="unknown" grabs="no" outrun="unknown" speed="unknown" health="unknown">
                <keep weapon="2" min="1000" max="2000" damage="unknown"/>
            </enemy>
        </knowledge>"#;
        let k = parse(text).unwrap();
        assert_eq!(k.enemies[0].stand, None);
        assert_eq!(k.enemies[0].grabs, Some(false));
        assert_eq!(k.enemies[0].policy, Policy::Pass);
        assert_eq!(k.weapons[0].cadence, None);
        assert!(parse(&text.replace("policy=\"pass\"", "policy=\"flee\"")).is_err());
        assert!(parse(&text.replace("grabs=", "grab=")).is_err());
        assert!(parse(&text.replace("weapon=\"2\"", "weapon=\"3\"")).is_err());
        assert!(parse(&text.replace("min=\"1000\"", "min=\"3000\"")).is_err());
        assert_eq!(k.enemies[0].keeps[0].spare, None);
        let spare =
            parse(&text.replace("damage=\"unknown\"/>", "damage=\"unknown\" spare=\"6\"/>"))
                .unwrap();
        assert_eq!(spare.enemies[0].keeps[0].spare, Some(6));
        assert!(
            parse(&text.replace("damage=\"unknown\"/>", "damage=\"unknown\" spare=\"-1\"/>"))
                .is_err()
        );
        assert!(!k.enemies[0].parts);
        let parts =
            parse(&text.replace("health=\"unknown\">", "health=\"unknown\" parts=\"yes\">"))
                .unwrap();
        assert!(parts.enemies[0].parts);
        assert!(!k.enemies[0].head);
        let head = parse(&text.replace("health=\"unknown\">", "health=\"unknown\" head=\"yes\">"))
            .unwrap();
        assert!(head.enemies[0].head);
        assert!(!k.weapons[0].melee);
        let melee =
            parse(&text.replace("status_bit=\"yes\"", "status_bit=\"yes\" melee=\"yes\"")).unwrap();
        assert!(melee.weapons[0].melee);
        assert!(!k.weapons[0].staple);
        let staple =
            parse(&text.replace("status_bit=\"yes\"", "status_bit=\"yes\" staple=\"yes\""))
                .unwrap();
        assert!(staple.weapons[0].staple);
        assert!(!k.weapons[0].reserve);
        let reserve =
            parse(&text.replace("status_bit=\"yes\"", "status_bit=\"yes\" reserve=\"yes\""))
                .unwrap();
        assert!(reserve.weapons[0].reserve);
        assert_eq!(k.weapons[0].variant_of, None);
        let variant = parse(&text.replace(
            "status_bit=\"yes\"",
            "status_bit=\"yes\" variant_of=\"0x07\"",
        ))
        .unwrap();
        assert_eq!(variant.weapons[0].variant_of, Some(0x07));
        assert!(
            parse(&text.replace("health=\"unknown\">", "health=\"unknown\" parts=\"maybe\">"))
                .is_err()
        );
        assert_eq!(k.enemies[0].prefer, None);
        let prefer =
            parse(&text.replace("health=\"unknown\">", "health=\"unknown\" prefer=\"2\">"))
                .unwrap();
        assert_eq!(prefer.enemies[0].prefer, Some(2));
        assert!(
            parse(&text.replace("health=\"unknown\">", "health=\"unknown\" prefer=\"3\">"))
                .is_err(),
            "a preferred weapon needs a <keep>"
        );
        assert_eq!(k.enemies[0].open, None);
        let open =
            parse(&text.replace("health=\"unknown\">", "health=\"unknown\" open=\"2\">")).unwrap();
        assert_eq!(open.enemies[0].open, Some(2));
        assert!(
            parse(&text.replace("health=\"unknown\">", "health=\"unknown\" open=\"3\">")).is_err(),
            "an opening weapon needs a <keep>"
        );
        assert_eq!(open.enemies[0].save, None);
        let save = parse(&text.replace(
            "health=\"unknown\">",
            "health=\"unknown\" open=\"2\" save=\"4\">",
        ))
        .unwrap();
        assert_eq!(save.enemies[0].save, Some(4));
        assert!(
            parse(&text.replace("health=\"unknown\">", "health=\"unknown\" save=\"4\">")).is_err(),
            "rounds saved for a fight need its opening weapon"
        );
    }
}
