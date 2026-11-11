//! Author-authored room metadata from `room/<id>/bot.xml`.
//!
//! The restricted bot VM sees this read-only through `bot:room_meta()`; the
//! Lua route still owns execution policy. The file is optional: a room without
//! one has no metadata. The parser refuses unknown elements and attributes, so
//! a typo fails loudly instead of silently dropping a puzzle step.
//!
//! Format 2 adds room facts (never coordinates or walk paths) a generic
//! planner reads: `<interact>` (an AOT slot and the flag that shows it is
//! done; `done="clear"` when the flag clearing shows it, as bank 7's item
//! bits do; with `after_bank`/`after_bit`, offered only once that flag is
//! set), `<camera>` (a camera cut to enter, which starts a room scene),
//! `<use>` (an item to USE from the menu while standing in an AOT slot's
//! use-item zone), `<answer>` (the choice to make at a yes/no prompt) and
//! `<avoid>` (an AOT slot the planner should not use; with `bank`/`bit` and
//! `while="set"|"clear"`, only while that flag is in that state; with
//! `unless_held`, never while that item is held) and `<push>` (push a
//! room object until it stands in an AOT slot's zone, or, with a compass
//! `direction`, push it that way until it clears the zone) and `<climb>`
//! (push a room object under an AOT slot's item zone, a high shelf, climb
//! it and take the item; its flag shows the item taken). `<interact>`,
//! `<camera>`, `<push>`, `<climb>` and `<fight>` (stay and fight an enemy
//! id until a flag is set) share one ordered step list. `<watch>` names a
//! scene the bot lets play instead of skipping: while its `bank`/`bit` flag
//! is clear and, with `after_bank`/`after_bit`, that flag is set. `<holds>`
//! says what the room holds (an item and a label): a location fact the
//! planner reads from every room (`bot:item_facts()`), so it can value a
//! room it has not entered. `<door slot via>` names a door the room parks at
//! its origin until AOT `via` is pressed (`bot:door_facts()`, read from
//! every room for routing). `<keep_out slot>` names an AOT slot's zone the
//! bot never walks through (with a `bank`/`bit`/`while` gate, only while
//! that flag is in that state): a floor probe that springs a trap, as
//! 507's plate starts the gas. `<leave slot bank bit>` sends the bot out
//! through door AOT `slot` as soon as the flag is set: a room event that
//! keeps hurting until the player leaves (507's gas, after the air shaft).
//! With `until_bank`/`until_bit` it stops once that flag is set (30B's
//! boulder has come to rest).
//! `<revisit bank bit while>` brings the bot back into the room once the
//! flag reaches that state, if it last left the room before then: a door
//! the room's init adds only later (50B's cell door to 512, live only in
//! the self-destruct), which the bot's map cannot know about from outside.
//! `<reach slot bank bit while [entry="same"]>` says when door AOT `slot`
//! can be walked to at all: only while the flag is in that state (and with
//! `entry="same"`, only on the entry it got there: 20C's door to 21A
//! stands in the pit Jill drops into). A failed approach to such a door is
//! retried once the flag changes, not on every puzzle step anywhere.
//! With `here_bank`/`here_bit`, that flag decides instead while the bot is
//! in the door's room (216's door to 218 is live while the room's object
//! probe sees the bookshelf on its spot; from elsewhere, scenario 0x4F says
//! the shelf starts there).
//! A format-1 document may not use them.
//!
//! Every fact element takes an optional `character="jill"|"chris"`: the fact
//! holds only for that character (player flag bit 0: 1 Jill, 0 Chris), as
//! when the two characters' variants of a room number their AOT slots
//! differently. [`load`] drops the other character's facts.
//!
//! Every fact element also takes an optional `when="NAME=VALUE ..."`: the
//! fact holds only while the run's story parameters (`params/<game>.xml`,
//! `--bot-param`) match, so one room file can steer each branch of the
//! story. See [`super::params`].
//!
//! The merged pack resolves the file next to the room's other entries, so a
//! mod's `room/117/bot.xml` overlays the base pack's copy (or adds one when the
//! base has none).

use anyhow::{Context, Result, bail};
use roxmltree::{Document, Node};

use super::params::{Params, When};
use crate::budget;
use crate::game::FLAG_BANK_COUNT;
use crate::pack::Pack;
use crate::state::RoomId;

/// The newest `<bot format="...">` version this parser reads. Format 1 (the
/// default when the attribute is absent) has only `<puzzle>`; format 2 adds
/// the fact elements.
pub const FORMAT: u32 = 2;

/// The format a document without a `format` attribute is read as.
const DEFAULT_FORMAT: u32 = 1;

/// The largest metadata file accepted, matching the room XML budget.
const MAX_BYTES: usize = 4 << 20;

/// One room's bot metadata: a format version, its puzzles and its facts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoomMeta {
    pub format: u32,
    pub puzzles: Vec<PuzzleMeta>,
    pub interacts: Vec<Interact>,
    pub uses: Vec<Use>,
    pub answers: Vec<Answer>,
    pub avoids: Vec<Avoid>,
    pub watches: Vec<Watch>,
    pub holds: Vec<Holds>,
    pub errands: Vec<Errand>,
    pub doors: Vec<DoorVia>,
    pub rides: Vec<Ride>,
    pub revisits: Vec<Revisit>,
    pub reaches: Vec<Reach>,
    pub keep_outs: Vec<KeepOut>,
    pub leaves: Vec<Leave>,
    pub kites: Vec<KiteLoop>,
    pub taps: Vec<Tap>,
    pub brews: Vec<Brew>,
}

/// `<tap slot item>`: pressing AOT `slot` (an event that asks first) fills
/// one held empty bottle with `item`: 409's water and UMB taps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tap {
    pub slot: u8,
    pub item: u8,
    pub label: Option<String>,
}

/// `<brew item bottle bank bit [after_bank after_bit after_while]>`: make
/// `item` here by filling `bottle`s at the room's taps and mixing them in
/// the menu, done once flag `bank`/`bit` is set or `item` is held. With an
/// `after` flag the brew waits for it (set, or clear with
/// `after_while="clear"`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Brew {
    pub item: u8,
    pub bottle: u8,
    pub flag: (u8, u8),
    pub after: Option<(u8, u8)>,
    pub after_clear: bool,
    pub label: Option<String>,
}

/// `<kite enemy x z radius>`: the loop the bot runs while it keeps enemy
/// `enemy` (a knowledge `<kite>`) at a distance in this room: centre
/// (`x`, `z`) and `radius`, inside the room's open floor (303's deck).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KiteLoop {
    pub enemy: u8,
    pub centre: (i32, i32),
    pub radius: i32,
}

/// `<leave slot bank bit [label]>`: once flag `bank`/`bit` is set, the
/// bot's next goal is door AOT `slot`, before anything else in the room.
/// 507's gas drains health until the room changes, so once the air shaft
/// has taken the player to the west half (room bit 7) the way out comes
/// first; the computer there waits for the next entry.
/// `escape="yes"` marks a chase rather than a drain: the flag is set only
/// while it runs (30F's boulder, room bit 0x13), so the steps leading into
/// it wait for no heal-up. `until_bank`/`until_bit` name a flag that ends
/// it: 30B's boulder sets a scenario flag for good as it starts, and a room
/// flag once it has come to rest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Leave {
    pub slot: u8,
    pub flag: (u8, u8),
    /// The leave no longer applies once this flag is set.
    pub until: Option<(u8, u8)>,
    pub escape: bool,
    pub label: Option<String>,
}

/// `<keep_out slot reason [bank bit while]>`: the bot's walks plan around
/// AOT `slot`'s zone, as if it were a wall, while the gate holds (always
/// without one). 507's plate starts the gas when the player steps on it,
/// and the only way to the box's far side runs past it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeepOut {
    pub slot: u8,
    pub reason: String,
    pub gate: Option<Gate>,
}

/// `<ride slot from [label]>`: the room is a ride (515's lift). Entered
/// from room `from`, it carries the player out through door AOT `slot`, a
/// zone parked at the room origin, by itself. Routes may pass through it
/// that way (`bot:ride_facts()`); in the room the bot only waits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ride {
    pub slot: u8,
    /// The room id as `bot:room().id` spells it (uppercase).
    pub from: String,
    pub label: Option<String>,
}

/// `<revisit bank bit while [label]>`: the bot comes back into the room
/// once the flag is in the `while` state, unless it has been in the room
/// with the flag in that state since (`bot:revisit_facts()`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Revisit {
    pub gate: Gate,
    pub label: Option<String>,
}

/// `<reach slot bank bit while [entry="same"] [label]>`: door AOT `slot`
/// can be walked to only while the flag is in the `while` state; with
/// `entry="same"`, only on the room entry the flag reached that state
/// (`bot:reach_facts()`). With `here_bank`/`here_bit`, that flag (in the
/// same `while` state) decides while the bot is in the room.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reach {
    pub slot: u8,
    pub gate: Gate,
    pub same_entry: bool,
    pub here: Option<(u8, u8)>,
    pub label: Option<String>,
}

/// `<door slot via [bank bit while] [label]>`: door AOT `slot` is parked at
/// the room origin until the player presses AOT `via`, whose event sets the
/// door around the player (406's ladder hatch to 40D). The planner routes
/// through such a door by pressing `via` (`bot:door_facts()`). With a
/// `bank`/`bit`/`while` gate the fact applies wherever the door sits, but
/// only while that flag is in that state: 302's door to 307 never fires by
/// itself, and its ladder event (AOT 10) turns it into an auto door once
/// the waterfall is off (scenario 0x2F clear).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoorVia {
    pub slot: u8,
    pub via: u8,
    pub gate: Option<Gate>,
    pub label: Option<String>,
}

/// `<deliver>` or `<fetch>`: one end of an item errand between rooms.
///
/// `<deliver item cut bank bit [after_bank after_bit]>`: the room wants
/// `item` carried into camera cut `cut` (a room script keyed on the camera
/// takes it there) while flag `bank`/`bit` is clear and, if given, the
/// `after` flag is set; the flag set means done. `<fetch item slot bank
/// bit>`: pressing AOT `slot` hands `item` over while flag `bank`/`bit` is
/// set (the offer).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Errand {
    pub kind: ErrandKind,
    pub item: u8,
    pub flag: (u8, u8),
    pub after: Option<(u8, u8)>,
    pub label: Option<String>,
}

/// Which end of an [`Errand`] a fact is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrandKind {
    /// Carry the item into this camera cut.
    Deliver { cut: u8 },
    /// Press this AOT slot to receive the item.
    Fetch { slot: u8 },
}

/// `<holds>`: the room holds `item` (a location fact; it names no slot,
/// order or route).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Holds {
    pub item: u8,
    pub label: Option<String>,
}

/// `<watch>`: a scene to let play rather than skip, while the done flag
/// `bank`/`bit` is clear and, if given, the `after` flag is set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Watch {
    pub flag: (u8, u8),
    pub after: Option<(u8, u8)>,
    pub label: Option<String>,
}

/// One step of the room's ordered sequence: `<interact>` (press the AOT
/// slot; its flag set means done) or `<camera>` (enter the camera cut; done
/// once the optional flag is set, or on entering the cut without one).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interact {
    pub target: Target,
    pub flag: Option<(u8, u8)>,
    /// `<interact done="clear">`: the step is done once its flag is clear,
    /// not set. Bank 7 runs this way: a set bit means the room's item is
    /// still there, and taking it clears the bit.
    pub done_clear: bool,
    /// `<interact after_bank after_bit>` (or `<wait>`, `<push>`): the step
    /// is offered only while this flag is set; until then the sequence
    /// waits (and a `<use>` may run). 40A's bookcase gap is an event only
    /// once the red book moved the bookcase; 306's box is pushed on once
    /// the crank's slab has shoved it off the wall.
    pub after: Option<(u8, u8)>,
    /// `<interact after_while="clear">`: the step waits for the `after`
    /// flag to be clear instead. Bank 7 runs this way: 508's reader lights
    /// a pass code's lamp only once that code's file (its item bit) is taken.
    pub after_clear: bool,
    /// `<interact reveals>`: the step is also done once this AOT slot is a
    /// live item or event zone (40C's fireplace turns slot 3 into the helmet
    /// key; 30B's corridor end arms slot 5, the boulder's trigger).
    pub reveals: Option<u8>,
    /// `<interact opens>`: the step is also done once this AOT slot is a
    /// live door zone (513 in Barry's branch: the event after the Tyrant
    /// turns door 0 from a message back into a door, and sets no flag).
    pub opens: Option<u8>,
    pub label: Option<String>,
}

/// What an [`Interact`] step goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Press action in this AOT slot's zone.
    Slot(u8),
    /// Walk into a camera switch zone that leads to this cut.
    Camera(u8),
    /// Push room object `object` until its position lies in AOT `slot`'s
    /// zone, or, with a `direction`, push it that way until it no longer
    /// covers that zone (done by position, or once the optional flag is set:
    /// a zone event that takes the object away, like 202's falling statue).
    /// With `clear_full` the trailing face must pass the zone's far edge, not
    /// just its far quarter (406's west crate, out of the east crate's way);
    /// with `until_stop` the push runs on until the object stalls against a
    /// wall. Without a direction, `axis` lines the object up on that axis
    /// only (40D's third crate, under the ladder corridor).
    Push {
        object: u8,
        slot: u8,
        direction: Option<Direction>,
        clear_full: bool,
        until_stop: bool,
        axis: Option<Axis>,
    },
    /// Push room object `object` under AOT `slot`'s item zone until it
    /// covers the zone, climb it, take the item from on top and climb down
    /// (11B's square crank on its shelf). Done once the item flag says so
    /// and the player is back on the floor.
    Climb { object: u8, slot: u8 },
    /// Stay in the room and fight enemy `enemy` (an entity id) until the
    /// step's flag is set: the planner keeps the player facing the nearest
    /// one so the combat layer engages (40C's Plant 42, which never comes
    /// to the player). `stage` (a room id's first character) is another
    /// stage the box loadout readies it from (303's Tyrant, met from the
    /// lab's escape lift: `stage="5"`). `optional` (`<fight optional="yes">`):
    /// the sequence passes the step while no held gun has a band for the
    /// enemy (30C's Black Tiger, dodged while the knife cuts the web).
    Fight {
        enemy: u8,
        stage: Option<u8>,
        optional: bool,
    },
    /// Stand still in the room until the step's flag is set: a timed room
    /// event that must play out (20C's pit, where Barry comes back with the
    /// rope after 600 frames).
    Wait,
}

/// A room axis a `<push>` lines its object up on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Z,
}

impl Axis {
    /// The attribute spelling.
    pub fn name(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Z => "z",
        }
    }
}

/// A compass direction in room coordinates: east is `+x`, north is `+z`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    North,
    South,
    East,
    West,
}

impl Direction {
    /// The attribute spelling.
    pub fn name(self) -> &'static str {
        match self {
            Self::North => "north",
            Self::South => "south",
            Self::East => "east",
            Self::West => "west",
        }
    }

    /// The unit step `(dx, dz)` in room coordinates.
    pub fn step(self) -> (i32, i32) {
        match self {
            Self::North => (0, 1),
            Self::South => (0, -1),
            Self::East => (1, 0),
            Self::West => (-1, 0),
        }
    }
}

/// `<use>`: stand where the room's use-item probe in AOT `slot` (a
/// `flag_bank_set` action) arms `item`, then USE `item` from the inventory
/// menu; the flag `bank`/`bit` set means done (clear with `done="clear"`).
/// With `after_bank`/`after_bit` it is offered only once that flag is set,
/// with `until_bank`/`until_bit` only while that flag is clear: 301's crank
/// drains the pool until Plant 42 dies, then refills it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Use {
    pub slot: u8,
    pub item: u8,
    pub flag: (u8, u8),
    pub done_clear: bool,
    pub after: Option<(u8, u8)>,
    pub until: Option<(u8, u8)>,
    pub label: Option<String>,
}

/// `<answer>`: the choice to make at a yes/no prompt, identified by the AOT
/// `slot` that opens it, the flag `bank`/`bit`, or both.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    /// `true` for yes, `false` for no.
    pub yes: bool,
    pub slot: Option<u8>,
    pub flag: Option<(u8, u8)>,
    /// The prompt's message id (`message`, without the fast-reveal bit): a
    /// prompt a scene opens, with no press and no flag to tell it apart
    /// (308's "Will you go with him?", 23, then 24 or 25).
    pub message: Option<u8>,
    pub label: Option<String>,
}

/// `<avoid>`: an AOT slot the planner should not use, and why. With a
/// [`Gate`], only while the gate's flag is in its state (a slot blocked by
/// a scene, like Barry standing at 105's fireplace until a flag is set).
/// With `unless_held`, not while that item is held (116's shotgun rack is
/// safe to lift while the broken shotgun to hang in its place is held).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Avoid {
    pub slot: u8,
    pub reason: String,
    pub gate: Option<Gate>,
    pub unless_held: Option<u8>,
}

/// The flag condition an `<avoid>` holds under: `bank`/`bit` and whether it
/// applies while that flag is set (`while="set"`) or clear (`while="clear"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gate {
    pub flag: (u8, u8),
    pub set: bool,
}

/// One named, ordered puzzle in a room.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PuzzleMeta {
    pub id: String,
    pub title: Option<String>,
    pub steps: Vec<PuzzleStep>,
}

/// One step of a puzzle: the action slot and flag it proves, the points to
/// walk through, and the point to face before pressing action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PuzzleStep {
    pub slot: u8,
    pub bank: u8,
    pub bit: u8,
    pub label: String,
    pub walks: Vec<[i32; 2]>,
    pub face: Option<[i32; 2]>,
}

/// Load `room/<id>/bot.xml` from the merged pack.
///
/// An absent file is `Ok(None)`. A present file that is not UTF-8, not
/// well-formed XML, or does not match the schema is an error naming the path.
pub fn load(pack: &Pack, id: RoomId, params: &Params) -> Result<Option<RoomMeta>> {
    load_for(pack, id, Some(jill(id.player_flag)), Some(params))
}

/// Whether a player flag is Jill's (bit 0 set), the value a fact's
/// `character` attribute is matched against.
pub fn jill(player_flag: u8) -> bool {
    player_flag & 1 == 1
}

/// [`load`] keeping only the facts for `jill` (`None`: every fact).
fn load_for(
    pack: &Pack,
    id: RoomId,
    jill: Option<bool>,
    params: Option<&Params>,
) -> Result<Option<RoomMeta>> {
    let path = format!("{}/bot.xml", id.dir());
    if !pack.contains(&path) {
        return Ok(None);
    }
    let bytes = pack
        .read(&path)
        .with_context(|| format!("cannot read {path}"))?;
    let text = std::str::from_utf8(&bytes).with_context(|| format!("{path} is not valid UTF-8"))?;
    parse_for(text, jill, params)
        .map(Some)
        .with_context(|| path)
}

/// One room's `<use>` fact, with the room it belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomUse {
    /// The room id as `bot:room().id` spells it (`"10C"`).
    pub room: String,
    pub fact: Use,
}

/// Every room's `<use>` facts, by room id then document order: the
/// planner's index of where a held item has a use (`bot:use_facts()`).
/// A file that does not parse is an error, as for [`load`]. With `jill`,
/// only that character's facts (see [`parse_for`]).
pub fn load_uses(pack: &Pack, jill: Option<bool>, params: Option<&Params>) -> Result<Vec<RoomUse>> {
    let mut uses = Vec::new();
    for (id, meta) in load_all(pack, jill, params)? {
        uses.extend(meta.uses.into_iter().map(|fact| RoomUse {
            room: id.clone(),
            fact,
        }));
    }
    Ok(uses)
}

/// One room's `<fight>` fact, with the room it belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomFight {
    /// The room id as `bot:room().id` spells it (`"513"`).
    pub room: String,
    /// The knowledge enemy id the fight is against.
    pub enemy: u8,
    /// The flag its death sets, when the fact names one.
    pub flag: Option<(u8, u8)>,
    /// The other stage the loadout readies it from (`<fight stage>`), as
    /// the ASCII byte of the room id's first character.
    pub stage: Option<u8>,
    /// `<fight optional="yes">`: passed while no held gun can fight it.
    pub optional: bool,
}

/// Every room's `<fight>` facts, by room id then document order
/// (`bot:fight_facts()`): the box loadout's index of the fights ahead.
pub fn load_fights(
    pack: &Pack,
    jill: Option<bool>,
    params: Option<&Params>,
) -> Result<Vec<RoomFight>> {
    let mut fights = Vec::new();
    for (id, meta) in load_all(pack, jill, params)? {
        for fact in meta.interacts {
            if let Target::Fight {
                enemy,
                stage,
                optional,
            } = fact.target
            {
                fights.push(RoomFight {
                    room: id.clone(),
                    enemy,
                    flag: fact.flag,
                    stage,
                    optional,
                });
            }
        }
    }
    Ok(fights)
}

/// One room's `<holds>` fact, with the room it belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomHolds {
    /// The room id as `bot:room().id` spells it (`"102"`).
    pub room: String,
    pub fact: Holds,
}

/// Every room's `<holds>` facts, by room id then document order: the
/// planner's index of where an item lies (`bot:item_facts()`).
pub fn load_holds(
    pack: &Pack,
    jill: Option<bool>,
    params: Option<&Params>,
) -> Result<Vec<RoomHolds>> {
    let mut holds = Vec::new();
    for (id, meta) in load_all(pack, jill, params)? {
        holds.extend(meta.holds.into_iter().map(|fact| RoomHolds {
            room: id.clone(),
            fact,
        }));
    }
    Ok(holds)
}

/// One room's `<ride>` fact, with the room it belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomRide {
    /// The room id as `bot:room().id` spells it (`"515"`).
    pub room: String,
    pub fact: Ride,
}

/// Every room's `<ride>` facts, by room id then document order
/// (`bot:ride_facts()`).
pub fn load_rides(
    pack: &Pack,
    jill: Option<bool>,
    params: Option<&Params>,
) -> Result<Vec<RoomRide>> {
    let mut rides = Vec::new();
    for (id, meta) in load_all(pack, jill, params)? {
        rides.extend(meta.rides.into_iter().map(|fact| RoomRide {
            room: id.clone(),
            fact,
        }));
    }
    Ok(rides)
}

/// One room's `<revisit>` fact, with the room it belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomRevisit {
    /// The room id as `bot:room().id` spells it (`"50B"`).
    pub room: String,
    pub fact: Revisit,
}

/// Every room's `<revisit>` facts, by room id then document order
/// (`bot:revisit_facts()`).
pub fn load_revisits(
    pack: &Pack,
    jill: Option<bool>,
    params: Option<&Params>,
) -> Result<Vec<RoomRevisit>> {
    let mut revisits = Vec::new();
    for (id, meta) in load_all(pack, jill, params)? {
        revisits.extend(meta.revisits.into_iter().map(|fact| RoomRevisit {
            room: id.clone(),
            fact,
        }));
    }
    Ok(revisits)
}

/// One room's `<reach>` fact, with the room it belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomReach {
    /// The room id as `bot:room().id` spells it (`"20C"`).
    pub room: String,
    pub fact: Reach,
}

/// Every room's `<reach>` facts, by room id then document order
/// (`bot:reach_facts()`).
pub fn load_reaches(
    pack: &Pack,
    jill: Option<bool>,
    params: Option<&Params>,
) -> Result<Vec<RoomReach>> {
    let mut reaches = Vec::new();
    for (id, meta) in load_all(pack, jill, params)? {
        reaches.extend(meta.reaches.into_iter().map(|fact| RoomReach {
            room: id.clone(),
            fact,
        }));
    }
    Ok(reaches)
}

/// One room's `<door>` fact, with the room it belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomDoorVia {
    /// The room id as `bot:room().id` spells it (`"406"`).
    pub room: String,
    pub fact: DoorVia,
}

/// Every room's `<door>` facts, by room id then document order
/// (`bot:door_facts()`).
pub fn load_door_vias(
    pack: &Pack,
    jill: Option<bool>,
    params: Option<&Params>,
) -> Result<Vec<RoomDoorVia>> {
    let mut doors = Vec::new();
    for (id, meta) in load_all(pack, jill, params)? {
        doors.extend(meta.doors.into_iter().map(|fact| RoomDoorVia {
            room: id.clone(),
            fact,
        }));
    }
    Ok(doors)
}

/// One room's `<avoid>` fact, with the room it belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomAvoid {
    /// The room id as `bot:room().id` spells it (`"302"`).
    pub room: String,
    pub fact: Avoid,
}

/// Every room's `<avoid>` facts, by room id then document order
/// (`bot:avoid_facts()`): remote planning skips a door a gated avoid holds
/// shut.
pub fn load_avoids(
    pack: &Pack,
    jill: Option<bool>,
    params: Option<&Params>,
) -> Result<Vec<RoomAvoid>> {
    let mut avoids = Vec::new();
    for (id, meta) in load_all(pack, jill, params)? {
        avoids.extend(meta.avoids.into_iter().map(|fact| RoomAvoid {
            room: id.clone(),
            fact,
        }));
    }
    Ok(avoids)
}

/// One room's `<deliver>` or `<fetch>` fact, with the room it belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomErrand {
    /// The room id as `bot:room().id` spells it (`"20D"`).
    pub room: String,
    pub fact: Errand,
}

/// Every room's errand facts, by room id then document order: the planner's
/// index of which room wants an item and which hands it over
/// (`bot:errand_facts()`).
pub fn load_errands(
    pack: &Pack,
    jill: Option<bool>,
    params: Option<&Params>,
) -> Result<Vec<RoomErrand>> {
    let mut errands = Vec::new();
    for (id, meta) in load_all(pack, jill, params)? {
        errands.extend(meta.errands.into_iter().map(|fact| RoomErrand {
            room: id.clone(),
            fact,
        }));
    }
    Ok(errands)
}

/// One room's `<brew>` fact, with the room it belongs to and that room's
/// `<tap>` facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomBrew {
    /// The room id as `bot:room().id` spells it (`"409"`).
    pub room: String,
    pub fact: Brew,
    pub taps: Vec<Tap>,
}

/// Every room's `<brew>` facts, by room id then document order
/// (`bot:brew_facts()`): what the inventory keeps for a brew from any room.
pub fn load_brews(
    pack: &Pack,
    jill: Option<bool>,
    params: Option<&Params>,
) -> Result<Vec<RoomBrew>> {
    let mut brews = Vec::new();
    for (id, meta) in load_all(pack, jill, params)? {
        let taps = meta.taps;
        brews.extend(meta.brews.into_iter().map(|fact| RoomBrew {
            room: id.clone(),
            fact,
            taps: taps.clone(),
        }));
    }
    Ok(brews)
}

/// Every room's parsed `bot.xml`, by room id (upper case).
fn load_all(
    pack: &Pack,
    jill: Option<bool>,
    params: Option<&Params>,
) -> Result<Vec<(String, RoomMeta)>> {
    let mut rooms: Vec<String> = pack
        .paths()
        .filter_map(|path| {
            let lower = path.to_ascii_lowercase();
            let room = lower.strip_prefix("room/")?.strip_suffix("/bot.xml")?;
            (room.len() == 3 && room.bytes().all(|b| b.is_ascii_hexdigit()))
                .then(|| room.to_string())
        })
        .collect();
    rooms.sort();
    rooms.dedup();
    let mut all = Vec::new();
    for room in rooms {
        let path = format!("room/{room}/bot.xml");
        let bytes = pack
            .read(&path)
            .with_context(|| format!("cannot read {path}"))?;
        let text =
            std::str::from_utf8(&bytes).with_context(|| format!("{path} is not valid UTF-8"))?;
        let meta = parse_for(text, jill, params).with_context(|| path.clone())?;
        all.push((room.to_ascii_uppercase(), meta));
    }
    Ok(all)
}

/// Parse one `bot.xml` document, keeping every fact.
#[cfg(test)]
pub fn parse(text: &str) -> Result<RoomMeta> {
    parse_for(text, None, None)
}

/// Parse one `bot.xml` document. With `jill`, a fact whose `character`
/// names the other character is checked and then dropped. With `params`, a
/// fact whose `when` does not hold is dropped too (an undeclared name or
/// value in it is an error); without, every `when` is only checked for
/// syntax and the fact kept.
pub fn parse_for(text: &str, jill: Option<bool>, params: Option<&Params>) -> Result<RoomMeta> {
    budget::check_len(text.len(), MAX_BYTES, "bot XML size")?;
    let document = Document::parse(text).map_err(|err| {
        anyhow::anyhow!(
            "bot XML is not well-formed at line {}: {err}",
            err.pos().row
        )
    })?;
    let root = document.root_element();
    if root.tag_name().name() != "bot" {
        bail!(
            "line {} <{}>: bot XML root must be <bot>",
            line(root),
            root.tag_name().name()
        );
    }
    check_attributes(root, &["format"])?;
    let format: u32 = attr_or(root, "format", DEFAULT_FORMAT)?;
    if !(1..=FORMAT).contains(&format) {
        bail!(
            "{}: bot XML format {format} is not supported (expected 1..={FORMAT})",
            at(root)
        );
    }

    let mut meta = RoomMeta {
        format,
        ..RoomMeta::default()
    };
    budget::check_len(
        elements(root).count(),
        budget::MAX_RECORDS,
        "bot XML element count",
    )?;
    for child in elements(root) {
        let name = child.tag_name().name();
        let fact = matches!(
            name,
            "interact"
                | "camera"
                | "push"
                | "climb"
                | "use"
                | "answer"
                | "avoid"
                | "watch"
                | "holds"
                | "deliver"
                | "fetch"
                | "fight"
                | "wait"
                | "door"
                | "keep_out"
                | "leave"
                | "kite"
                | "tap"
                | "brew"
                | "ride"
                | "revisit"
                | "reach"
        );
        if fact && format < 2 {
            bail!("{}: <{name}> needs <bot format=\"2\">", at(child));
        }
        let character = match child.attribute("character") {
            None => None,
            Some("jill") => Some(true),
            Some("chris") => Some(false),
            Some(other) => bail!(
                "{}: `character` value `{other}` is not `jill` or `chris`",
                at(child)
            ),
        };
        if character.is_some() && !fact {
            bail!("{}: unknown attribute `character`", at(child));
        }
        let when = match child.attribute("when") {
            None => None,
            Some(text) => Some(When::parse(text).with_context(|| at(child))?),
        };
        if when.is_some() && !fact {
            bail!("{}: unknown attribute `when`", at(child));
        }
        let mut keep = match (character, jill) {
            (Some(only), Some(jill)) => only == jill,
            _ => true,
        };
        if let (Some(when), Some(params)) = (&when, params) {
            keep &= params.holds(when).with_context(|| at(child))?;
        }
        match name {
            "puzzle" => {
                let puzzle = parse_puzzle(child)?;
                if meta.puzzles.iter().any(|held| held.id == puzzle.id) {
                    bail!("{}: duplicate puzzle id `{}`", at(child), puzzle.id);
                }
                meta.puzzles.push(puzzle);
            }
            "interact" | "camera" | "push" | "climb" | "fight" | "wait" => {
                let fact = match name {
                    "interact" => parse_interact(child)?,
                    "camera" => parse_camera(child)?,
                    "climb" => parse_climb(child)?,
                    "fight" => parse_fight(child)?,
                    "wait" => parse_wait(child)?,
                    _ => parse_push(child)?,
                };
                if keep {
                    meta.interacts.push(fact);
                }
            }
            "use" => {
                let fact = parse_use(child)?;
                if keep {
                    meta.uses.push(fact);
                }
            }
            "answer" => {
                let fact = parse_answer(child)?;
                if keep {
                    meta.answers.push(fact);
                }
            }
            "avoid" => {
                let fact = parse_avoid(child)?;
                if keep {
                    meta.avoids.push(fact);
                }
            }
            "watch" => {
                let fact = parse_watch(child)?;
                if keep {
                    meta.watches.push(fact);
                }
            }
            "holds" => {
                let fact = parse_holds(child)?;
                if keep {
                    meta.holds.push(fact);
                }
            }
            "deliver" | "fetch" => {
                let fact = parse_errand(child)?;
                if keep {
                    meta.errands.push(fact);
                }
            }
            "door" => {
                let fact = parse_door(child)?;
                if keep {
                    meta.doors.push(fact);
                }
            }
            "ride" => {
                let fact = parse_ride(child)?;
                if keep {
                    meta.rides.push(fact);
                }
            }
            "revisit" => {
                let fact = parse_revisit(child)?;
                if keep {
                    meta.revisits.push(fact);
                }
            }
            "reach" => {
                let fact = parse_reach(child)?;
                if keep {
                    meta.reaches.push(fact);
                }
            }
            "keep_out" => {
                let fact = parse_keep_out(child)?;
                if keep {
                    meta.keep_outs.push(fact);
                }
            }
            "tap" => {
                let fact = parse_tap(child)?;
                if keep {
                    meta.taps.push(fact);
                }
            }
            "brew" => {
                let fact = parse_brew(child)?;
                if keep {
                    meta.brews.push(fact);
                }
            }
            "leave" => {
                let fact = parse_leave(child)?;
                if keep {
                    meta.leaves.push(fact);
                }
            }
            "kite" => {
                let fact = parse_kite_loop(child)?;
                if keep {
                    meta.kites.push(fact);
                }
            }
            other => bail!("{}: unknown <bot> element <{other}>", at(child)),
        }
    }
    Ok(meta)
}

/// A fact element has no children.
pub(super) fn check_leaf(node: Node<'_, '_>) -> Result<()> {
    if let Some(child) = elements(node).next() {
        bail!(
            "{}: unknown <{}> element <{}>",
            at(child),
            node.tag_name().name(),
            child.tag_name().name()
        );
    }
    Ok(())
}

/// A `bank` attribute: below [`FLAG_BANK_COUNT`].
fn bank_attr(node: Node<'_, '_>) -> Result<u8> {
    let bank: u8 = attr(node, "bank")?;
    if usize::from(bank) >= FLAG_BANK_COUNT {
        bail!(
            "{}: bank {bank} is out of range 0..={}",
            at(node),
            FLAG_BANK_COUNT - 1
        );
    }
    Ok(bank)
}

fn parse_done(node: Node<'_, '_>) -> Result<bool> {
    match node.attribute("done") {
        None | Some("set") => Ok(false),
        Some("clear") => Ok(true),
        Some(other) => bail!(
            "{}: <{}> `done` value `{other}` is not `set` or `clear`",
            at(node),
            node.tag_name().name()
        ),
    }
}

fn parse_climb(node: Node<'_, '_>) -> Result<Interact> {
    check_attributes(
        node,
        &[
            "object",
            "slot",
            "bank",
            "bit",
            "done",
            "label",
            "character",
            "when",
        ],
    )?;
    check_leaf(node)?;
    let done_clear = parse_done(node)?;
    Ok(Interact {
        target: Target::Climb {
            object: attr(node, "object")?,
            slot: attr(node, "slot")?,
        },
        flag: Some((bank_attr(node)?, attr(node, "bit")?)),
        done_clear,
        after: None,
        after_clear: false,
        reveals: None,
        opens: None,
        label: node.attribute("label").map(str::to_string),
    })
}

fn parse_interact(node: Node<'_, '_>) -> Result<Interact> {
    check_attributes(
        node,
        &[
            "slot",
            "bank",
            "bit",
            "done",
            "after_bank",
            "after_bit",
            "after_while",
            "reveals",
            "opens",
            "label",
            "character",
            "when",
        ],
    )?;
    check_leaf(node)?;
    let done_clear = parse_done(node)?;
    Ok(Interact {
        target: Target::Slot(attr(node, "slot")?),
        flag: Some((bank_attr(node)?, attr(node, "bit")?)),
        done_clear,
        after: after_flag(node)?,
        after_clear: parse_after_while(node)?,
        reveals: node
            .attribute("reveals")
            .map(|_| attr(node, "reveals"))
            .transpose()?,
        opens: node
            .attribute("opens")
            .map(|_| attr(node, "opens"))
            .transpose()?,
        label: node.attribute("label").map(str::to_string),
    })
}

fn parse_use(node: Node<'_, '_>) -> Result<Use> {
    check_attributes(
        node,
        &[
            "slot",
            "item",
            "bank",
            "bit",
            "done",
            "after_bank",
            "after_bit",
            "until_bank",
            "until_bit",
            "label",
            "character",
            "when",
        ],
    )?;
    check_leaf(node)?;
    let item: u8 = attr(node, "item")?;
    if item == 0 {
        bail!("{}: <use> `item` must not be 0 (no item)", at(node));
    }
    let until = match (node.attribute("until_bank"), node.attribute("until_bit")) {
        (None, None) => None,
        (Some(_), Some(_)) => {
            let bank: u8 = attr(node, "until_bank")?;
            if usize::from(bank) >= FLAG_BANK_COUNT {
                bail!(
                    "{}: until_bank {bank} is out of range 0..={}",
                    at(node),
                    FLAG_BANK_COUNT - 1
                );
            }
            Some((bank, attr(node, "until_bit")?))
        }
        _ => bail!(
            "{}: <use> needs both `until_bank` and `until_bit`, or neither",
            at(node)
        ),
    };
    Ok(Use {
        slot: attr(node, "slot")?,
        item,
        flag: (bank_attr(node)?, attr(node, "bit")?),
        done_clear: parse_done(node)?,
        after: after_flag(node)?,
        until,
        label: node.attribute("label").map(str::to_string),
    })
}

/// The optional `bank`/`bit`/`while` gate of an `<avoid>` or a `<door>`:
/// all three or none.
fn parse_gate(node: Node<'_, '_>) -> Result<Option<Gate>> {
    let tag = node.tag_name().name();
    match (
        node.attribute("bank"),
        node.attribute("bit"),
        node.attribute("while"),
    ) {
        (None, None, None) => Ok(None),
        (Some(_), Some(_), Some(state)) => Ok(Some(Gate {
            flag: (bank_attr(node)?, attr(node, "bit")?),
            set: match state {
                "set" => true,
                "clear" => false,
                other => bail!(
                    "{}: <{tag}> `while` value `{other}` is not `set` or `clear`",
                    at(node)
                ),
            },
        })),
        _ => bail!(
            "{}: <{tag}> needs all of `bank`, `bit` and `while`, or none",
            at(node)
        ),
    }
}

fn parse_door(node: Node<'_, '_>) -> Result<DoorVia> {
    check_attributes(
        node,
        &[
            "slot",
            "via",
            "bank",
            "bit",
            "while",
            "label",
            "character",
            "when",
        ],
    )?;
    check_leaf(node)?;
    let slot: u8 = attr(node, "slot")?;
    let via: u8 = attr(node, "via")?;
    if slot == via {
        bail!(
            "{}: <door> `via` must be another slot than `slot`",
            at(node)
        );
    }
    Ok(DoorVia {
        slot,
        via,
        gate: parse_gate(node)?,
        label: node.attribute("label").map(str::to_string),
    })
}

fn parse_ride(node: Node<'_, '_>) -> Result<Ride> {
    check_attributes(node, &["slot", "from", "label", "character", "when"])?;
    check_leaf(node)?;
    let slot: u8 = attr(node, "slot")?;
    let from = node
        .attribute("from")
        .filter(|from| !from.is_empty())
        .ok_or_else(|| anyhow::anyhow!("{}: <ride> needs `from`", at(node)))?;
    Ok(Ride {
        slot,
        from: from.to_ascii_uppercase(),
        label: node.attribute("label").map(str::to_string),
    })
}

fn parse_revisit(node: Node<'_, '_>) -> Result<Revisit> {
    check_attributes(
        node,
        &["bank", "bit", "while", "label", "character", "when"],
    )?;
    check_leaf(node)?;
    let gate = parse_gate(node)?.ok_or_else(|| {
        anyhow::anyhow!("{}: <revisit> needs `bank`, `bit` and `while`", at(node))
    })?;
    Ok(Revisit {
        gate,
        label: node.attribute("label").map(str::to_string),
    })
}

fn parse_reach(node: Node<'_, '_>) -> Result<Reach> {
    check_attributes(
        node,
        &[
            "slot",
            "bank",
            "bit",
            "while",
            "entry",
            "here_bank",
            "here_bit",
            "label",
            "character",
            "when",
        ],
    )?;
    check_leaf(node)?;
    let here = match (node.attribute("here_bank"), node.attribute("here_bit")) {
        (None, None) => None,
        (Some(_), Some(_)) => {
            let bank: u8 = attr(node, "here_bank")?;
            if usize::from(bank) >= FLAG_BANK_COUNT {
                bail!(
                    "{}: here_bank {bank} is out of range 0..={}",
                    at(node),
                    FLAG_BANK_COUNT - 1
                );
            }
            Some((bank, attr(node, "here_bit")?))
        }
        _ => bail!(
            "{}: <reach> needs both `here_bank` and `here_bit`, or neither",
            at(node)
        ),
    };
    let gate = parse_gate(node)?
        .ok_or_else(|| anyhow::anyhow!("{}: <reach> needs `bank`, `bit` and `while`", at(node)))?;
    let same_entry = match node.attribute("entry") {
        None => false,
        Some("same") => true,
        Some(other) => bail!(
            "{}: <reach> `entry` value `{other}` is not `same`",
            at(node)
        ),
    };
    Ok(Reach {
        slot: attr(node, "slot")?,
        gate,
        same_entry,
        here,
        label: node.attribute("label").map(str::to_string),
    })
}

fn parse_holds(node: Node<'_, '_>) -> Result<Holds> {
    check_attributes(node, &["item", "label", "character", "when"])?;
    check_leaf(node)?;
    let item: u8 = attr(node, "item")?;
    if item == 0 {
        bail!("{}: <holds> `item` must not be 0 (no item)", at(node));
    }
    Ok(Holds {
        item,
        label: node.attribute("label").map(str::to_string),
    })
}

fn parse_push(node: Node<'_, '_>) -> Result<Interact> {
    check_attributes(
        node,
        &[
            "object",
            "slot",
            "direction",
            "clear",
            "until",
            "axis",
            "bank",
            "bit",
            "after_bank",
            "after_bit",
            "reveals",
            "label",
            "character",
            "when",
        ],
    )?;
    check_leaf(node)?;
    let flag = match (node.attribute("bank"), node.attribute("bit")) {
        (Some(_), Some(_)) => Some((bank_attr(node)?, attr(node, "bit")?)),
        (None, None) => None,
        _ => bail!(
            "{}: <push> needs both `bank` and `bit`, or neither",
            at(node)
        ),
    };
    let direction = match node.attribute("direction") {
        None => None,
        Some("north") => Some(Direction::North),
        Some("south") => Some(Direction::South),
        Some("east") => Some(Direction::East),
        Some("west") => Some(Direction::West),
        Some(other) => bail!(
            "{}: <push> `direction` must be north, south, east or west, not `{other}`",
            at(node)
        ),
    };
    let clear_full = match node.attribute("clear") {
        None => false,
        Some("full") if direction.is_some() => true,
        Some("full") => bail!("{}: <push> `clear` needs a `direction`", at(node)),
        Some(other) => bail!("{}: <push> `clear` must be `full`, not `{other}`", at(node)),
    };
    let until_stop = match node.attribute("until") {
        None => false,
        Some("stop") if direction.is_some() => true,
        Some("stop") => bail!("{}: <push> `until` needs a `direction`", at(node)),
        Some(other) => bail!("{}: <push> `until` must be `stop`, not `{other}`", at(node)),
    };
    let axis = match node.attribute("axis") {
        None => None,
        Some(_) if direction.is_some() => {
            bail!("{}: <push> takes `axis` or `direction`, not both", at(node))
        }
        Some("x") => Some(Axis::X),
        Some("z") => Some(Axis::Z),
        Some(other) => bail!("{}: <push> `axis` must be x or z, not `{other}`", at(node)),
    };
    Ok(Interact {
        target: Target::Push {
            object: attr(node, "object")?,
            slot: attr(node, "slot")?,
            direction,
            clear_full,
            until_stop,
            axis,
        },
        flag,
        done_clear: false,
        after: after_flag(node)?,
        after_clear: false,
        reveals: node
            .attribute("reveals")
            .map(|_| attr(node, "reveals"))
            .transpose()?,
        opens: None,
        label: node.attribute("label").map(str::to_string),
    })
}

fn parse_fight(node: Node<'_, '_>) -> Result<Interact> {
    check_attributes(
        node,
        &[
            "enemy",
            "bank",
            "bit",
            "after_bank",
            "after_bit",
            "after_while",
            "stage",
            "optional",
            "label",
            "character",
            "when",
        ],
    )?;
    check_leaf(node)?;
    let optional = match node.attribute("optional") {
        None | Some("no") => false,
        Some("yes") => true,
        Some(text) => bail!("{}: `optional` must be yes or no, not `{text}`", at(node)),
    };
    let stage = match node.attribute("stage") {
        None => None,
        Some(text) => match text.as_bytes() {
            [byte] if byte.is_ascii_alphanumeric() => Some(byte.to_ascii_uppercase()),
            _ => bail!(
                "{}: `stage` must be one room-id character, not `{text}`",
                at(node)
            ),
        },
    };
    Ok(Interact {
        target: Target::Fight {
            enemy: attr(node, "enemy")?,
            stage,
            optional,
        },
        flag: Some((bank_attr(node)?, attr(node, "bit")?)),
        done_clear: false,
        after: after_flag(node)?,
        after_clear: parse_after_while(node)?,
        reveals: None,
        opens: None,
        label: node.attribute("label").map(str::to_string),
    })
}

fn parse_wait(node: Node<'_, '_>) -> Result<Interact> {
    check_attributes(
        node,
        &[
            "bank",
            "bit",
            "after_bank",
            "after_bit",
            "label",
            "character",
            "when",
        ],
    )?;
    check_leaf(node)?;
    Ok(Interact {
        target: Target::Wait,
        flag: Some((bank_attr(node)?, attr(node, "bit")?)),
        done_clear: false,
        after: after_flag(node)?,
        after_clear: false,
        reveals: None,
        opens: None,
        label: node.attribute("label").map(str::to_string),
    })
}

fn parse_camera(node: Node<'_, '_>) -> Result<Interact> {
    check_attributes(node, &["cut", "bank", "bit", "label", "character", "when"])?;
    check_leaf(node)?;
    let flag = match (node.attribute("bank"), node.attribute("bit")) {
        (Some(_), Some(_)) => Some((bank_attr(node)?, attr(node, "bit")?)),
        (None, None) => None,
        _ => bail!(
            "{}: <camera> needs both `bank` and `bit`, or neither",
            at(node)
        ),
    };
    Ok(Interact {
        target: Target::Camera(attr(node, "cut")?),
        flag,
        done_clear: false,
        after: None,
        after_clear: false,
        reveals: None,
        opens: None,
        label: node.attribute("label").map(str::to_string),
    })
}

fn parse_answer(node: Node<'_, '_>) -> Result<Answer> {
    check_attributes(
        node,
        &[
            "choice",
            "slot",
            "bank",
            "bit",
            "message",
            "label",
            "character",
            "when",
        ],
    )?;
    check_leaf(node)?;
    let yes = match attr_text(node, "choice")? {
        "yes" => true,
        "no" => false,
        other => bail!(
            "{}: <answer> `choice` value `{other}` is not `yes` or `no`",
            at(node)
        ),
    };
    let slot = match node.attribute("slot") {
        Some(_) => Some(attr(node, "slot")?),
        None => None,
    };
    let flag = match (node.attribute("bank"), node.attribute("bit")) {
        (Some(_), Some(_)) => Some((bank_attr(node)?, attr(node, "bit")?)),
        (None, None) => None,
        _ => bail!(
            "{}: <answer> needs both `bank` and `bit`, or neither",
            at(node)
        ),
    };
    let message = match node.attribute("message") {
        Some(_) => Some(attr(node, "message")?),
        None => None,
    };
    if slot.is_none() && flag.is_none() && message.is_none() {
        bail!(
            "{}: <answer> needs a `slot`, a `bank`/`bit` flag or a `message`",
            at(node)
        );
    }
    Ok(Answer {
        yes,
        slot,
        flag,
        message,
        label: node.attribute("label").map(str::to_string),
    })
}

fn parse_avoid(node: Node<'_, '_>) -> Result<Avoid> {
    check_attributes(
        node,
        &[
            "slot",
            "reason",
            "bank",
            "bit",
            "while",
            "unless_held",
            "character",
            "when",
        ],
    )?;
    check_leaf(node)?;
    let reason = attr_text(node, "reason")?.to_string();
    if reason.trim().is_empty() {
        bail!("{}: <avoid> reason must not be empty", at(node));
    }
    let gate = parse_gate(node)?;
    let unless_held = match node.attribute("unless_held") {
        None => None,
        Some(_) => Some(attr(node, "unless_held")?),
    };
    Ok(Avoid {
        slot: attr(node, "slot")?,
        reason,
        gate,
        unless_held,
    })
}

fn parse_keep_out(node: Node<'_, '_>) -> Result<KeepOut> {
    check_attributes(
        node,
        &[
            "slot",
            "reason",
            "bank",
            "bit",
            "while",
            "character",
            "when",
        ],
    )?;
    check_leaf(node)?;
    let reason = attr_text(node, "reason")?.to_string();
    if reason.trim().is_empty() {
        bail!("{}: <keep_out> reason must not be empty", at(node));
    }
    Ok(KeepOut {
        slot: attr(node, "slot")?,
        reason,
        gate: parse_gate(node)?,
    })
}

fn parse_kite_loop(node: Node<'_, '_>) -> Result<KiteLoop> {
    check_attributes(node, &["enemy", "x", "z", "radius", "character", "when"])?;
    check_leaf(node)?;
    let radius: i32 = attr(node, "radius")?;
    if radius <= 0 {
        bail!("{}: <kite> radius must be > 0", at(node));
    }
    Ok(KiteLoop {
        enemy: attr(node, "enemy")?,
        centre: (attr(node, "x")?, attr(node, "z")?),
        radius,
    })
}

fn parse_tap(node: Node<'_, '_>) -> Result<Tap> {
    check_attributes(node, &["slot", "item", "label", "character", "when"])?;
    check_leaf(node)?;
    let item: u8 = attr(node, "item")?;
    if item == 0 {
        bail!("{}: <tap> `item` must not be 0 (no item)", at(node));
    }
    Ok(Tap {
        slot: attr(node, "slot")?,
        item,
        label: node.attribute("label").map(str::to_string),
    })
}

fn parse_brew(node: Node<'_, '_>) -> Result<Brew> {
    check_attributes(
        node,
        &[
            "item",
            "bottle",
            "bank",
            "bit",
            "after_bank",
            "after_bit",
            "after_while",
            "label",
            "character",
            "when",
        ],
    )?;
    check_leaf(node)?;
    let item: u8 = attr(node, "item")?;
    let bottle: u8 = attr(node, "bottle")?;
    if item == 0 || bottle == 0 {
        bail!(
            "{}: <brew> `item` and `bottle` must not be 0 (no item)",
            at(node)
        );
    }
    Ok(Brew {
        item,
        bottle,
        flag: (bank_attr(node)?, attr(node, "bit")?),
        after: after_flag(node)?,
        after_clear: parse_after_while(node)?,
        label: node.attribute("label").map(str::to_string),
    })
}

fn parse_leave(node: Node<'_, '_>) -> Result<Leave> {
    check_attributes(
        node,
        &[
            "slot",
            "bank",
            "bit",
            "until_bank",
            "until_bit",
            "escape",
            "label",
            "character",
            "when",
        ],
    )?;
    check_leaf(node)?;
    let until = match (node.attribute("until_bank"), node.attribute("until_bit")) {
        (Some(_), Some(_)) => {
            let bank: u8 = attr(node, "until_bank")?;
            if usize::from(bank) >= FLAG_BANK_COUNT {
                bail!(
                    "{}: until_bank {bank} is out of range 0..={}",
                    at(node),
                    FLAG_BANK_COUNT - 1
                );
            }
            Some((bank, attr(node, "until_bit")?))
        }
        (None, None) => None,
        _ => bail!(
            "{}: <leave> needs both `until_bank` and `until_bit`, or neither",
            at(node)
        ),
    };
    let escape = match node.attribute("escape") {
        None | Some("no") => false,
        Some("yes") => true,
        Some(other) => bail!(
            "{}: <leave> `escape` value `{other}` is not `yes` or `no`",
            at(node)
        ),
    };
    Ok(Leave {
        slot: attr(node, "slot")?,
        flag: (bank_attr(node)?, attr(node, "bit")?),
        until,
        escape,
        label: node.attribute("label").map(str::to_string),
    })
}

fn parse_errand(node: Node<'_, '_>) -> Result<Errand> {
    let name = node.tag_name().name();
    let deliver = name == "deliver";
    let place = if deliver { "cut" } else { "slot" };
    let mut allowed = vec![place, "item", "bank", "bit", "label", "character", "when"];
    if deliver {
        allowed.extend(["after_bank", "after_bit"]);
    }
    check_attributes(node, &allowed)?;
    check_leaf(node)?;
    let item: u8 = attr(node, "item")?;
    if item == 0 {
        bail!("{}: <{name}> `item` must not be 0 (no item)", at(node));
    }
    let kind = if deliver {
        ErrandKind::Deliver {
            cut: attr(node, "cut")?,
        }
    } else {
        ErrandKind::Fetch {
            slot: attr(node, "slot")?,
        }
    };
    Ok(Errand {
        kind,
        item,
        flag: (bank_attr(node)?, attr(node, "bit")?),
        after: after_flag(node)?,
        label: node.attribute("label").map(str::to_string),
    })
}

/// The optional `after_bank`/`after_bit` pair: both or neither.
/// `after_while`: `"set"` (the default) or `"clear"`, the sense the
/// `after` flag waits for. It needs `after_bank`/`after_bit`.
fn parse_after_while(node: Node<'_, '_>) -> Result<bool> {
    match node.attribute("after_while") {
        None | Some("set") => Ok(false),
        Some("clear") if node.attribute("after_bank").is_some() => Ok(true),
        Some("clear") => bail!("{}: `after_while` needs `after_bank`/`after_bit`", at(node)),
        Some(other) => bail!(
            "{}: `after_while` must be \"set\" or \"clear\", not {other:?}",
            at(node)
        ),
    }
}

fn after_flag(node: Node<'_, '_>) -> Result<Option<(u8, u8)>> {
    match (node.attribute("after_bank"), node.attribute("after_bit")) {
        (Some(_), Some(_)) => {
            let bank: u8 = attr(node, "after_bank")?;
            if usize::from(bank) >= FLAG_BANK_COUNT {
                bail!(
                    "{}: after_bank {bank} is out of range 0..={}",
                    at(node),
                    FLAG_BANK_COUNT - 1
                );
            }
            Ok(Some((bank, attr(node, "after_bit")?)))
        }
        (None, None) => Ok(None),
        _ => bail!(
            "{}: <{}> needs both `after_bank` and `after_bit`, or neither",
            at(node),
            node.tag_name().name()
        ),
    }
}

fn parse_watch(node: Node<'_, '_>) -> Result<Watch> {
    check_attributes(
        node,
        &[
            "bank",
            "bit",
            "after_bank",
            "after_bit",
            "label",
            "character",
            "when",
        ],
    )?;
    check_leaf(node)?;
    let after = after_flag(node)?;
    Ok(Watch {
        flag: (bank_attr(node)?, attr(node, "bit")?),
        after,
        label: node.attribute("label").map(str::to_string),
    })
}

fn parse_puzzle(node: Node<'_, '_>) -> Result<PuzzleMeta> {
    check_attributes(node, &["id", "title"])?;
    let id = attr_text(node, "id")?.to_string();
    if id.trim().is_empty() {
        bail!("{}: puzzle id must not be empty", at(node));
    }
    let title = node.attribute("title").map(str::to_string);
    let count = elements(node).count();
    budget::check_len(count, budget::MAX_RECORDS, "bot XML step count")?;
    let mut steps = Vec::with_capacity(count);
    for child in elements(node) {
        if child.tag_name().name() != "step" {
            bail!(
                "{}: unknown <puzzle> element <{}>",
                at(child),
                child.tag_name().name()
            );
        }
        steps.push(parse_step(child)?);
    }
    Ok(PuzzleMeta { id, title, steps })
}

fn parse_step(node: Node<'_, '_>) -> Result<PuzzleStep> {
    check_attributes(node, &["slot", "bank", "bit", "label"])?;
    let slot: u8 = attr(node, "slot")?;
    let bank = bank_attr(node)?;
    let bit: u8 = attr(node, "bit")?;
    let label = attr_text(node, "label")?.to_string();
    let count = elements(node).count();
    budget::check_len(count, budget::MAX_RECORDS, "bot XML action count")?;
    let mut walks = Vec::new();
    let mut face = None;
    for child in elements(node) {
        match child.tag_name().name() {
            "walk" => {
                check_attributes(child, &["x", "z"])?;
                walks.push([attr(child, "x")?, attr(child, "z")?]);
            }
            "face" => {
                if face.is_some() {
                    bail!("{}: step has more than one <face>", at(child));
                }
                check_attributes(child, &["x", "z"])?;
                face = Some([attr(child, "x")?, attr(child, "z")?]);
            }
            other => bail!("{}: unknown <step> element <{other}>", at(child)),
        }
    }
    Ok(PuzzleStep {
        slot,
        bank,
        bit,
        label,
        walks,
        face,
    })
}

/// The element children of `node`.
pub(super) fn elements<'a, 'input>(
    node: Node<'a, 'input>,
) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(Node::is_element)
}

/// Refuse any attribute of `node` not in `known`.
pub(super) fn check_attributes(node: Node<'_, '_>, known: &[&str]) -> Result<()> {
    for attribute in node.attributes() {
        if !known.contains(&attribute.name()) {
            bail!("{}: unknown attribute `{}`", at(node), attribute.name());
        }
    }
    Ok(())
}

pub(super) fn attr_text<'a>(node: Node<'a, '_>, name: &str) -> Result<&'a str> {
    node.attribute(name).with_context(|| {
        format!(
            "{}: <{}> needs a `{name}` attribute",
            at(node),
            node.tag_name().name()
        )
    })
}

/// Parse a decimal or `0x` hex integer.
pub(super) fn integer(text: &str) -> Option<i64> {
    let text = text.trim();
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let value = match digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        Some(hex) => i64::from_str_radix(hex, 16).ok()?,
        None => digits.parse().ok()?,
    };
    Some(if negative { -value } else { value })
}

pub(super) fn attr<T: TryFrom<i64>>(node: Node<'_, '_>, name: &str) -> Result<T> {
    let raw = attr_text(node, name)?;
    integer(raw)
        .and_then(|value| T::try_from(value).ok())
        .with_context(|| {
            format!(
                "{}: <{}> `{name}` value `{raw}` is not a valid {}",
                at(node),
                node.tag_name().name(),
                std::any::type_name::<T>()
            )
        })
}

fn attr_or<T: TryFrom<i64>>(node: Node<'_, '_>, name: &str, default: T) -> Result<T> {
    match node.attribute(name) {
        Some(_) => attr(node, name),
        None => Ok(default),
    }
}

/// The source line of `node`, for parse errors.
fn line(node: Node<'_, '_>) -> u32 {
    node.document().text_pos_at(node.range().start).row
}

/// The source line and element name of `node`, for parse errors.
pub(super) fn at(node: Node<'_, '_>) -> String {
    format!("line {} <{}>", line(node), node.tag_name().name())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("arklay-bot-meta-{}-{label}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn room_117() -> RoomId {
        RoomId {
            stage: 1,
            room: 0x17,
            player_flag: 1,
            mansion2: false,
        }
    }

    fn checked_in() -> RoomMeta {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mods/bot/room/117/bot.xml");
        let text = fs::read_to_string(&path).unwrap();
        parse(&text).unwrap()
    }

    #[test]
    fn checked_in_117_metadata_has_the_seven_paintings() {
        let meta = checked_in();
        assert_eq!(meta.format, 2);
        assert!(meta.puzzles.is_empty(), "format 2 has no walk steps");
        let interact = |slot, bank, bit, label: &str| Interact {
            target: Target::Slot(slot),
            flag: Some((bank, bit)),
            done_clear: false,
            after: None,
            after_clear: false,
            reveals: None,
            opens: None,
            label: Some(label.to_string()),
        };
        let expected = vec![
            interact(3, 4, 0x1A, "painting 1/7 newborn"),
            interact(5, 4, 0x1C, "painting 2/7 infant"),
            interact(6, 4, 0x1D, "painting 3/7 lively boy"),
            interact(4, 4, 0x1B, "painting 4/7 young man"),
            interact(2, 4, 0x19, "painting 5/7 tired middle-aged man"),
            interact(7, 4, 0x1E, "painting 6/7 bold old man"),
            interact(8, 0, 0x03, "painting 7/7 people mourning"),
        ];
        assert_eq!(meta.interacts, expected);
    }

    #[test]
    fn unknown_elements_and_attributes_are_rejected() {
        let unknown_step = r#"<bot format="1">
            <puzzle id="paintings"><step slot="1" bank="0" bit="1" label="x">
                <jump x="1" z="2"/>
            </step></puzzle>
        </bot>"#;
        let error = parse(unknown_step).unwrap_err().to_string();
        assert!(error.contains("unknown <step> element <jump>"), "{error}");
        assert!(error.contains("line 3"), "{error}");

        let unknown_root = r#"<bot format="1"><record/></bot>"#;
        let error = parse(unknown_root).unwrap_err().to_string();
        assert!(error.contains("unknown <bot> element <record>"), "{error}");

        let unknown_attribute = r#"<bot format="1" version="1"/>"#;
        let error = parse(unknown_attribute).unwrap_err().to_string();
        assert!(error.contains("unknown attribute `version`"), "{error}");
    }

    #[test]
    fn step_ranges_are_validated() {
        let bad_bank = r#"<bot format="1">
            <puzzle id="paintings"><step slot="1" bank="10" bit="1" label="x"/></puzzle>
        </bot>"#;
        let error = parse(bad_bank).unwrap_err().to_string();
        assert!(error.contains("bank 10 is out of range"), "{error}");

        let bad_bit = r#"<bot format="1">
            <puzzle id="paintings"><step slot="1" bank="0" bit="0x100" label="x"/></puzzle>
        </bot>"#;
        let error = parse(bad_bit).unwrap_err().to_string();
        assert!(error.contains("is not a valid u8"), "{error}");
    }

    #[test]
    fn format_2_reads_the_fact_elements() {
        let text = r#"<bot format="2">
            <puzzle id="p"><step slot="1" bank="0" bit="1" label="x"/></puzzle>
            <interact slot="4" bank="1" bit="0x22" label="push the statue"/>
            <interact slot="5" bank="0" bit="3"/>
            <camera cut="3" label="north hall"/>
            <camera cut="2" bank="1" bit="0x05"/>
            <push object="1" slot="2" label="armor onto the vent"/>
            <push object="3" slot="5" direction="east" label="shelf off the score"/>
            <answer choice="no" slot="6" label="do not drink"/>
            <answer choice="yes" bank="4" bit="0x10"/>
            <avoid slot="7" reason="the plant stuns the player"/>
            <avoid slot="8" bank="1" bit="0xA1" while="clear" reason="Barry stands there"/>
            <avoid slot="1" unless_held="0x1C" reason="the rack springs the trap"/>
        </bot>"#;
        let meta = parse(text).unwrap();
        assert_eq!(meta.format, 2);
        assert_eq!(meta.puzzles.len(), 1);
        assert_eq!(
            meta.interacts,
            vec![
                Interact {
                    target: Target::Slot(4),
                    flag: Some((1, 0x22)),
                    done_clear: false,
                    after: None,
                    after_clear: false,
                    reveals: None,
                    opens: None,
                    label: Some("push the statue".to_string()),
                },
                Interact {
                    target: Target::Slot(5),
                    flag: Some((0, 3)),
                    done_clear: false,
                    after: None,
                    after_clear: false,
                    reveals: None,
                    opens: None,
                    label: None,
                },
                Interact {
                    target: Target::Camera(3),
                    flag: None,
                    done_clear: false,
                    after: None,
                    after_clear: false,
                    reveals: None,
                    opens: None,
                    label: Some("north hall".to_string()),
                },
                Interact {
                    target: Target::Camera(2),
                    flag: Some((1, 0x05)),
                    done_clear: false,
                    after: None,
                    after_clear: false,
                    reveals: None,
                    opens: None,
                    label: None,
                },
                Interact {
                    target: Target::Push {
                        object: 1,
                        slot: 2,
                        direction: None,
                        clear_full: false,
                        until_stop: false,
                        axis: None,
                    },
                    flag: None,
                    done_clear: false,
                    after: None,
                    after_clear: false,
                    reveals: None,
                    opens: None,
                    label: Some("armor onto the vent".to_string()),
                },
                Interact {
                    target: Target::Push {
                        object: 3,
                        slot: 5,
                        direction: Some(Direction::East),
                        clear_full: false,
                        until_stop: false,
                        axis: None,
                    },
                    flag: None,
                    done_clear: false,
                    after: None,
                    after_clear: false,
                    reveals: None,
                    opens: None,
                    label: Some("shelf off the score".to_string()),
                },
            ]
        );
        assert_eq!(
            meta.answers,
            vec![
                Answer {
                    yes: false,
                    slot: Some(6),
                    flag: None,
                    message: None,
                    label: Some("do not drink".to_string()),
                },
                Answer {
                    yes: true,
                    slot: None,
                    flag: Some((4, 0x10)),
                    message: None,
                    label: None,
                },
            ]
        );
        assert_eq!(
            meta.avoids,
            vec![
                Avoid {
                    slot: 7,
                    reason: "the plant stuns the player".to_string(),
                    gate: None,
                    unless_held: None,
                },
                Avoid {
                    slot: 8,
                    reason: "Barry stands there".to_string(),
                    gate: Some(Gate {
                        flag: (1, 0xA1),
                        set: false,
                    }),
                    unless_held: None,
                },
                Avoid {
                    slot: 1,
                    reason: "the rack springs the trap".to_string(),
                    gate: None,
                    unless_held: Some(0x1C),
                },
            ]
        );
    }

    #[test]
    fn keep_out_and_leave_name_a_zone_and_a_door() {
        let text = r#"<bot format="2">
            <keep_out slot="8" bank="4" bit="3" while="clear" reason="the plate"/>
            <keep_out slot="9" reason="always"/>
            <leave slot="0" bank="4" bit="7" label="out"/>
            <leave slot="1" bank="4" bit="0x13" escape="yes"/>
            <leave slot="2" bank="0" bit="0x3D" until_bank="4" until_bit="0x17" escape="yes"/>
        </bot>"#;
        let meta = parse(text).unwrap();
        assert_eq!(
            meta.keep_outs,
            vec![
                KeepOut {
                    slot: 8,
                    reason: "the plate".to_string(),
                    gate: Some(Gate {
                        flag: (4, 3),
                        set: false
                    }),
                },
                KeepOut {
                    slot: 9,
                    reason: "always".to_string(),
                    gate: None,
                },
            ]
        );
        assert_eq!(
            meta.leaves,
            vec![
                Leave {
                    slot: 0,
                    flag: (4, 7),
                    until: None,
                    escape: false,
                    label: Some("out".to_string()),
                },
                Leave {
                    slot: 1,
                    flag: (4, 0x13),
                    until: None,
                    escape: true,
                    label: None,
                },
                Leave {
                    slot: 2,
                    flag: (0, 0x3D),
                    until: Some((4, 0x17)),
                    escape: true,
                    label: None,
                },
            ]
        );
        for bad in [
            r#"<bot format="2"><keep_out slot="8"/></bot>"#,
            r#"<bot format="2"><keep_out slot="8" reason=" "/></bot>"#,
            r#"<bot format="2"><keep_out slot="8" bank="4" bit="3" reason="x"/></bot>"#,
            r#"<bot format="2"><leave slot="0"/></bot>"#,
            r#"<bot format="2"><leave slot="0" bank="4" bit="7" escape="maybe"/></bot>"#,
            r#"<bot format="1"><leave slot="0" bank="4" bit="7"/></bot>"#,
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn tap_and_brew_name_the_drug_store_chemistry() {
        let text = r#"<bot format="2">
            <tap slot="8" item="0x14" label="water"/>
            <brew item="0x1B" bottle="0x13" bank="0" bit="0x13"
                  after_bank="7" after_bit="0x4E" after_while="clear" label="V-JOLT"/>
            <brew item="0x1A" bottle="0x13" bank="0" bit="0x13"/>
        </bot>"#;
        let meta = parse(text).unwrap();
        assert_eq!(
            meta.taps,
            vec![Tap {
                slot: 8,
                item: 0x14,
                label: Some("water".to_string()),
            }]
        );
        assert_eq!(
            meta.brews,
            vec![
                Brew {
                    item: 0x1B,
                    bottle: 0x13,
                    flag: (0, 0x13),
                    after: Some((7, 0x4E)),
                    after_clear: true,
                    label: Some("V-JOLT".to_string()),
                },
                Brew {
                    item: 0x1A,
                    bottle: 0x13,
                    flag: (0, 0x13),
                    after: None,
                    after_clear: false,
                    label: None,
                },
            ]
        );
        for bad in [
            r#"<bot format="2"><tap slot="8"/></bot>"#,
            r#"<bot format="2"><tap slot="8" item="0"/></bot>"#,
            r#"<bot format="1"><tap slot="8" item="0x14"/></bot>"#,
            r#"<bot format="2"><brew item="0x1B" bank="0" bit="1"/></bot>"#,
            r#"<bot format="2"><brew item="0x1B" bottle="0x13" bank="0" bit="1" after_bank="7"/></bot>"#,
            r#"<bot format="2"><brew item="0x1B" bottle="0x13" bank="0" bit="1" after_while="clear"/></bot>"#,
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn kite_names_an_enemy_and_its_loop() {
        let text = r#"<bot format="2">
            <kite enemy="0x10" x="19650" z="14000" radius="8000"/>
        </bot>"#;
        let meta = parse(text).unwrap();
        assert_eq!(
            meta.kites,
            vec![KiteLoop {
                enemy: 0x10,
                centre: (19650, 14000),
                radius: 8000,
            }]
        );
        for bad in [
            r#"<bot format="2"><kite enemy="16" x="0" z="0" radius="0"/></bot>"#,
            r#"<bot format="2"><kite enemy="16" x="0" radius="10"/></bot>"#,
            r#"<bot format="2"><kite enemy="16" x="0" z="0" radius="10" r="1"/></bot>"#,
            r#"<bot format="1"><kite enemy="16" x="0" z="0" radius="10"/></bot>"#,
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn climb_names_an_object_an_item_slot_and_its_flag() {
        let text = r#"<bot format="2">
            <climb object="0" slot="3" bank="7" bit="35" done="clear" label="the crank"/>
        </bot>"#;
        let meta = parse(text).unwrap();
        assert_eq!(
            meta.interacts,
            vec![Interact {
                target: Target::Climb { object: 0, slot: 3 },
                flag: Some((7, 35)),
                done_clear: true,
                after: None,
                after_clear: false,
                reveals: None,
                opens: None,
                label: Some("the crank".to_string()),
            }]
        );
        let flagless = r#"<bot format="2"><climb object="0" slot="3"/></bot>"#;
        assert!(parse(flagless).is_err());
        let bad =
            r#"<bot format="2"><climb object="0" slot="3" bank="7" bit="1" done="gone"/></bot>"#;
        let error = parse(bad).unwrap_err().to_string();
        assert!(error.contains("<climb> `done`"), "{error}");
    }

    #[test]
    fn push_takes_an_optional_done_flag() {
        let text = r#"<bot format="2">
            <push object="0" slot="3" bank="0" bit="0x0B" label="statue off the balcony"/>
        </bot>"#;
        let meta = parse(text).unwrap();
        assert_eq!(
            meta.interacts,
            vec![Interact {
                target: Target::Push {
                    object: 0,
                    slot: 3,
                    direction: None,
                    clear_full: false,
                    until_stop: false,
                    axis: None,
                },
                flag: Some((0, 0x0B)),
                done_clear: false,
                after: None,
                after_clear: false,
                reveals: None,
                opens: None,
                label: Some("statue off the balcony".to_string()),
            }]
        );
        let half = r#"<bot format="2"><push object="0" slot="3" bank="0"/></bot>"#;
        let error = parse(half).unwrap_err().to_string();
        assert!(error.contains("both `bank` and `bit`"), "{error}");
    }

    #[test]
    fn interact_done_may_be_the_flag_clearing() {
        let text = r#"<bot format="2">
            <interact slot="4" bank="7" bit="0x39" done="clear" label="battery"/>
            <interact slot="5" bank="1" bit="2" done="set"/>
        </bot>"#;
        let meta = parse(text).unwrap();
        assert!(meta.interacts[0].done_clear);
        assert_eq!(meta.interacts[0].flag, Some((7, 0x39)));
        assert!(!meta.interacts[1].done_clear);
        let bad = r#"<bot format="2"><interact slot="4" bank="7" bit="1" done="maybe"/></bot>"#;
        let error = parse(bad).unwrap_err().to_string();
        assert!(error.contains("is not `set` or `clear`"), "{error}");
        // Only <interact> takes it.
        let camera = r#"<bot format="2"><camera cut="1" bank="1" bit="2" done="clear"/></bot>"#;
        let error = parse(camera).unwrap_err().to_string();
        assert!(error.contains("unknown attribute `done`"), "{error}");
    }

    #[test]
    fn fight_names_an_enemy_and_its_done_flag() {
        let text = r#"<bot format="2">
            <fight enemy="0x08" bank="3" bit="96" label="Plant 42"/>
        </bot>"#;
        let meta = parse(text).unwrap();
        assert_eq!(
            meta.interacts[0].target,
            Target::Fight {
                enemy: 0x08,
                stage: None,
                optional: false,
            }
        );
        let optional =
            r#"<bot format="2"><fight enemy="0x04" bank="3" bit="137" optional="yes"/></bot>"#;
        assert_eq!(
            parse(optional).unwrap().interacts[0].target,
            Target::Fight {
                enemy: 0x04,
                stage: None,
                optional: true,
            }
        );
        assert_eq!(meta.interacts[0].flag, Some((3, 96)));
        let flagless = r#"<bot format="2"><fight enemy="0x08"/></bot>"#;
        assert!(parse(flagless).is_err());
        let staged = r#"<bot format="2"><fight enemy="0x10" bank="3" bit="0x59" stage="5"/></bot>"#;
        assert_eq!(
            parse(staged).unwrap().interacts[0].target,
            Target::Fight {
                enemy: 0x10,
                stage: Some(b'5'),
                optional: false,
            }
        );
        let long = r#"<bot format="2"><fight enemy="0x10" bank="3" bit="1" stage="55"/></bot>"#;
        assert!(parse(long).is_err());
    }

    #[test]
    fn fight_takes_an_after_flag() {
        let text = r#"<bot format="2">
            <fight enemy="0x10" bank="3" bit="0x59" after_bank="7" after_bit="0x49"
                   after_while="clear"/>
        </bot>"#;
        let meta = parse(text).unwrap();
        assert_eq!(meta.interacts[0].after, Some((7, 0x49)));
        assert!(meta.interacts[0].after_clear);
    }

    #[test]
    fn ride_names_the_room_it_is_entered_from() {
        let text = r#"<bot format="2">
            <ride slot="1" from="50c" label="down to 514"/>
        </bot>"#;
        let meta = parse(text).unwrap();
        assert_eq!(meta.rides.len(), 1);
        assert_eq!(meta.rides[0].slot, 1);
        assert_eq!(meta.rides[0].from, "50C");
        assert_eq!(meta.rides[0].label.as_deref(), Some("down to 514"));
        let fromless = r#"<bot format="2"><ride slot="1"/></bot>"#;
        let error = parse(fromless).unwrap_err().to_string();
        assert!(error.contains("needs `from`"), "{error}");
    }

    #[test]
    fn reach_takes_a_gate_and_an_optional_same_entry() {
        let text = r#"<bot format="2">
            <reach slot="1" bank="1" bit="0x26" while="set" entry="same" label="the pit"/>
            <reach slot="2" bank="0" bit="0x2F" while="clear"/>
        </bot>"#;
        let meta = parse(text).unwrap();
        assert_eq!(meta.reaches.len(), 2);
        assert_eq!(meta.reaches[0].slot, 1);
        assert_eq!(
            meta.reaches[0].gate,
            Gate {
                flag: (1, 0x26),
                set: true,
            }
        );
        assert!(meta.reaches[0].same_entry);
        assert!(!meta.reaches[1].same_entry);
        assert!(!meta.reaches[1].gate.set);
        assert_eq!(meta.reaches[0].here, None);
        let here = r#"<bot format="2">
            <reach slot="2" bank="0" bit="0x4F" while="set" here_bank="4" here_bit="0x18"/>
        </bot>"#;
        assert_eq!(parse(here).unwrap().reaches[0].here, Some((4, 0x18)));
        let half =
            r#"<bot format="2"><reach slot="2" bank="0" bit="1" while="set" here_bank="4"/></bot>"#;
        assert!(parse(half).is_err());
        let gateless = r#"<bot format="2"><reach slot="1"/></bot>"#;
        assert!(parse(gateless).is_err());
        let entry =
            r#"<bot format="2"><reach slot="1" bank="1" bit="2" while="set" entry="any"/></bot>"#;
        assert!(parse(entry).is_err());
    }

    #[test]
    fn revisit_needs_a_flag_gate() {
        let text = r#"<bot format="2">
            <revisit bank="1" bit="0x4C" while="set" label="the cell door"/>
        </bot>"#;
        let meta = parse(text).unwrap();
        assert_eq!(meta.revisits.len(), 1);
        assert_eq!(
            meta.revisits[0].gate,
            Gate {
                flag: (1, 0x4C),
                set: true,
            }
        );
        assert_eq!(meta.revisits[0].label.as_deref(), Some("the cell door"));
        let gateless = r#"<bot format="2"><revisit label="x"/></bot>"#;
        let error = parse(gateless).unwrap_err().to_string();
        assert!(error.contains("needs `bank`"), "{error}");
    }

    #[test]
    fn wait_names_its_flag_and_an_optional_after() {
        let text = r#"<bot format="2">
            <wait bank="1" bit="0x25" after_bank="1" after_bit="0x26" label="rope"/>
        </bot>"#;
        let meta = parse(text).unwrap();
        assert_eq!(meta.interacts[0].target, Target::Wait);
        assert_eq!(meta.interacts[0].flag, Some((1, 0x25)));
        assert_eq!(meta.interacts[0].after, Some((1, 0x26)));
        let flagless = r#"<bot format="2"><wait label="rope"/></bot>"#;
        assert!(parse(flagless).is_err());
    }

    #[test]
    fn door_names_the_slot_that_opens_it() {
        let text = r#"<bot format="2">
            <door slot="2" via="5" label="the hatch"/>
        </bot>"#;
        let meta = parse(text).unwrap();
        assert_eq!(
            meta.doors,
            vec![DoorVia {
                slot: 2,
                via: 5,
                gate: None,
                label: Some("the hatch".to_string()),
            }]
        );
        let gated = r#"<bot format="2">
            <door slot="3" via="10" bank="0" bit="0x2F" while="clear"/>
        </bot>"#;
        assert_eq!(
            parse(gated).unwrap().doors[0].gate,
            Some(Gate {
                flag: (0, 0x2F),
                set: false
            })
        );
        let half = r#"<bot format="2"><door slot="3" via="10" bank="0" bit="1"/></bot>"#;
        assert!(parse(half).is_err(), "a gate needs bank, bit and while");
        let same = r#"<bot format="2"><door slot="2" via="2"/></bot>"#;
        assert!(parse(same).is_err());
        let old = r#"<bot><door slot="2" via="5"/></bot>"#;
        assert!(parse(old).is_err());
        let jill = r#"<bot format="2"><door slot="2" via="5" character="chris"/></bot>"#;
        assert!(parse_for(jill, Some(true), None).unwrap().doors.is_empty());
    }

    #[test]
    fn interact_may_name_the_item_slot_it_reveals() {
        let text = r#"<bot format="2">
            <interact slot="2" bank="7" bit="0x58" done="clear" reveals="3"/>
            <interact slot="5" bank="1" bit="2" opens="0"/>
        </bot>"#;
        let meta = parse(text).unwrap();
        assert_eq!(meta.interacts[0].reveals, Some(3));
        assert_eq!(meta.interacts[1].reveals, None);
        assert_eq!(meta.interacts[0].opens, None);
        assert_eq!(meta.interacts[1].opens, Some(0));
        let bad = r#"<bot format="2"><interact slot="2" bank="1" bit="2" reveals="x"/></bot>"#;
        assert!(parse(bad).is_err());
    }

    #[test]
    fn interact_may_wait_for_an_after_flag() {
        let text = r#"<bot format="2">
            <interact slot="7" bank="0" bit="0x44" after_bank="0" after_bit="0x25"/>
            <interact slot="5" bank="1" bit="2"/>
        </bot>"#;
        let meta = parse(text).unwrap();
        assert_eq!(meta.interacts[0].after, Some((0, 0x25)));
        assert_eq!(meta.interacts[1].after, None);
        assert!(!meta.interacts[0].after_clear);
        // 508: a lamp waits for its pass code's item bit to clear.
        let clear = r#"<bot format="2"><interact slot="4" bank="0" bit="52"
            after_bank="7" after_bit="82" after_while="clear"/></bot>"#;
        assert!(parse(clear).unwrap().interacts[0].after_clear);
        let lone = r#"<bot format="2"><interact slot="4" bank="0" bit="52"
            after_while="clear"/></bot>"#;
        assert!(
            parse(lone)
                .unwrap_err()
                .to_string()
                .contains("needs `after_bank`")
        );
        let bad = r#"<bot format="2"><interact slot="4" bank="0" bit="52"
            after_bank="7" after_bit="82" after_while="gone"/></bot>"#;
        assert!(
            parse(bad)
                .unwrap_err()
                .to_string()
                .contains("\"set\" or \"clear\"")
        );
        // 306: the box is pushed on only once the crank's slab shoved it.
        let push =
            r#"<bot format="2"><push object="2" slot="5" after_bank="0" after_bit="60"/></bot>"#;
        assert_eq!(parse(push).unwrap().interacts[0].after, Some((0, 60)));
        let half = r#"<bot format="2"><interact slot="7" bank="0" bit="1" after_bit="2"/></bot>"#;
        let error = parse(half).unwrap_err().to_string();
        assert!(
            error.contains("both `after_bank` and `after_bit`"),
            "{error}"
        );
    }

    #[test]
    fn push_clear_full_needs_a_direction() {
        let text =
            r#"<bot format="2"><push object="1" slot="5" direction="north" clear="full"/></bot>"#;
        let meta = parse(text).unwrap();
        assert_eq!(
            meta.interacts[0].target,
            Target::Push {
                object: 1,
                slot: 5,
                direction: Some(Direction::North),
                clear_full: true,
                until_stop: false,
                axis: None,
            }
        );
        let bare = r#"<bot format="2"><push object="1" slot="5" clear="full"/></bot>"#;
        assert!(parse(bare).is_err());
        let bad =
            r#"<bot format="2"><push object="1" slot="5" direction="north" clear="half"/></bot>"#;
        assert!(parse(bad).is_err());
    }

    #[test]
    fn push_until_stop_needs_a_direction_and_axis_none() {
        let text = r#"<bot format="2">
            <push object="2" slot="10" axis="x"/>
            <push object="2" slot="2" direction="east" until="stop"/>
        </bot>"#;
        let meta = parse(text).unwrap();
        assert_eq!(
            meta.interacts[0].target,
            Target::Push {
                object: 2,
                slot: 10,
                direction: None,
                clear_full: false,
                until_stop: false,
                axis: Some(Axis::X),
            }
        );
        assert_eq!(
            meta.interacts[1].target,
            Target::Push {
                object: 2,
                slot: 2,
                direction: Some(Direction::East),
                clear_full: false,
                until_stop: true,
                axis: None,
            }
        );
        for bad in [
            r#"<bot format="2"><push object="2" slot="2" until="stop"/></bot>"#,
            r#"<bot format="2"><push object="2" slot="2" direction="east" until="wall"/></bot>"#,
            r#"<bot format="2"><push object="2" slot="2" direction="east" axis="x"/></bot>"#,
            r#"<bot format="2"><push object="2" slot="2" axis="y"/></bot>"#,
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn push_direction_is_a_compass_point() {
        let bad = r#"<bot format="2"><push object="1" slot="5" direction="up"/></bot>"#;
        let error = parse(bad).unwrap_err().to_string();
        assert!(error.contains("north, south, east or west"), "{error}");
        assert_eq!(Direction::East.step(), (1, 0));
        assert_eq!(Direction::North.step(), (0, 1));
        assert_eq!(Direction::West.name(), "west");
    }

    #[test]
    fn format_2_reads_use_facts_in_order() {
        let text = r#"<bot format="2">
            <interact slot="4" bank="1" bit="0x22"/>
            <use slot="2" item="0x26" bank="1" bit="0xA9" label="herbicide at the pump"/>
            <use slot="11" item="32" bank="0" bit="0x6A"/>
        </bot>"#;
        let meta = parse(text).unwrap();
        assert_eq!(meta.interacts.len(), 1, "a use is not an interact step");
        assert_eq!(
            meta.uses,
            vec![
                Use {
                    slot: 2,
                    item: 0x26,
                    flag: (1, 0xA9),
                    done_clear: false,
                    after: None,
                    until: None,
                    label: Some("herbicide at the pump".to_string()),
                },
                Use {
                    slot: 11,
                    item: 0x20,
                    flag: (0, 0x6A),
                    done_clear: false,
                    after: None,
                    until: None,
                    label: None,
                },
            ]
        );
        let refill = r#"<bot format="2">
            <use slot="4" item="0x1D" bank="0" bit="0x2F" done="clear"
                 after_bank="3" after_bit="96" until_bank="0" until_bit="9"/>
        </bot>"#;
        let fact = &parse(refill).unwrap().uses[0];
        assert!(fact.done_clear);
        assert_eq!(fact.after, Some((3, 96)));
        assert_eq!(fact.until, Some((0, 9)));
        let half =
            r#"<bot format="2"><use slot="4" item="0x1D" bank="0" bit="1" until_bank="0"/></bot>"#;
        assert!(parse(half).is_err(), "until needs both halves");
    }

    #[test]
    fn use_facts_are_validated() {
        let error = |text: &str| parse(text).unwrap_err().to_string();
        let v1 = r#"<bot format="1"><use slot="2" item="0x26" bank="1" bit="1"/></bot>"#;
        assert!(error(v1).contains("<use> needs <bot format=\"2\">"));
        let no_item = r#"<bot format="2"><use slot="2" bank="1" bit="1"/></bot>"#;
        assert!(error(no_item).contains("<use> needs a `item` attribute"));
        let zero_item = r#"<bot format="2"><use slot="2" item="0" bank="1" bit="1"/></bot>"#;
        assert!(error(zero_item).contains("`item` must not be 0"));
        let big_item = r#"<bot format="2"><use slot="2" item="0x100" bank="1" bit="1"/></bot>"#;
        assert!(error(big_item).contains("`item` value `0x100` is not a valid u8"));
        let no_flag = r#"<bot format="2"><use slot="2" item="0x26"/></bot>"#;
        assert!(error(no_flag).contains("<use> needs a `bank` attribute"));
        let half_flag = r#"<bot format="2"><use slot="2" item="0x26" bank="1"/></bot>"#;
        assert!(error(half_flag).contains("<use> needs a `bit` attribute"));
        let bad_bank = r#"<bot format="2"><use slot="2" item="0x26" bank="10" bit="1"/></bot>"#;
        assert!(error(bad_bank).contains("bank 10 is out of range"));
        let unknown = r#"<bot format="2"><use slot="2" item="0x26" bank="1" bit="1" x="1"/></bot>"#;
        assert!(error(unknown).contains("unknown attribute `x`"));
        let child = r#"<bot format="2"><use slot="2" item="0x26" bank="1" bit="1"><walk x="1" z="2"/></use></bot>"#;
        assert!(error(child).contains("unknown <use> element <walk>"));
    }

    #[test]
    fn the_brew_index_carries_each_rooms_taps() {
        let pack = Pack::open(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mods/bot")).unwrap();
        let shipped = fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mods/bot/params/re1.xml"),
        )
        .unwrap();
        let mut params = Params::new(super::super::params::parse(&shipped).unwrap());
        // The brew is the V-JOLT branch's: the default drops it.
        assert!(
            load_brews(&pack, Some(true), Some(&params))
                .unwrap()
                .is_empty()
        );
        params.set("vjolt", "yes").unwrap();
        let brews = load_brews(&pack, Some(true), Some(&params)).unwrap();
        let rows: Vec<(&str, u8, Vec<u8>)> = brews
            .iter()
            .map(|entry| {
                let taps = entry.taps.iter().map(|tap| tap.item).collect();
                (entry.room.as_str(), entry.fact.item, taps)
            })
            .collect();
        assert_eq!(rows, vec![("409", 0x1B, vec![0x14, 0x15, 0x16])]);
    }

    #[test]
    fn checked_in_use_facts_parse() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mods/bot/room");
        let mut seen = 0;
        for entry in fs::read_dir(&root).unwrap() {
            let path = entry.unwrap().path().join("bot.xml");
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let meta = parse(&text).unwrap_or_else(|err| panic!("{}: {err:#}", path.display()));
            seen += meta.uses.len();
        }
        assert!(seen > 0, "no checked-in <use> facts were found");
    }

    #[test]
    fn the_use_index_lists_every_rooms_use_facts_by_room() {
        let pack = Pack::open(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mods/bot")).unwrap();
        let uses = load_uses(&pack, Some(true), None).unwrap();
        let rows: Vec<(&str, u8, u8)> = uses
            .iter()
            .map(|entry| (entry.room.as_str(), entry.fact.slot, entry.fact.item))
            .collect();
        // Sorted by room id, document order inside a room.
        assert_eq!(
            rows,
            vec![
                ("105", 11, 0x20),
                ("10C", 2, 0x26),
                ("10D", 2, 0x21),
                ("10D", 2, 0x22),
                ("10F", 13, 0x23),
                ("10F", 6, 0x1F),
                ("116", 5, 0x1C),
                ("11A", 5, 0x2D),
                ("11A", 3, 0x2E),
                ("11A", 4, 0x2C),
                ("11A", 6, 0x29),
                ("301", 4, 0x1D),
                ("301", 4, 0x1D),
                ("302", 4, 0x27),
                ("303", 6, 0x2A),
                ("305", 6, 0x24),
                ("305", 5, 0x25),
                ("306", 1, 0x1E),
                ("307", 4, 0x1E),
                ("30F", 6, 0x1E),
                ("30F", 6, 0x1E),
                // Both branches' red book (vjolt=no and vjolt=yes): no
                // parameters were given, so every `when` holds.
                ("40A", 5, 0x3E),
                ("40A", 5, 0x3E),
                ("40F", 2, 0x1B),
                ("501", 4, 0x27),
                ("501", 4, 0x27),
                ("507", 12, 0x28),
                ("509", 3, 0x28),
                ("510", 3, 0x28),
            ]
        );
        assert_eq!(uses[1].fact.flag, (1, 0xA9));
        // Chris's 105 numbers the fireplace's gold-emblem zone 8, not 11.
        let chris = load_uses(&pack, Some(false), None).unwrap();
        let chris_105: Vec<(u8, u8, (u8, u8))> = chris
            .iter()
            .filter(|entry| entry.room == "105")
            .map(|entry| (entry.fact.slot, entry.fact.item, entry.fact.flag))
            .collect();
        assert_eq!(chris_105, vec![(8, 0x20, (1, 0xA8))]);
        assert_eq!(load_uses(&pack, None, None).unwrap().len(), uses.len() + 1);
    }

    #[test]
    fn holds_names_an_item_a_room_holds() {
        let meta = parse(
            r#"<bot format="2"><holds item="0x1C" label="broken shotgun"/><holds item="3"/></bot>"#,
        )
        .unwrap();
        assert_eq!(
            meta.holds,
            vec![
                Holds {
                    item: 0x1C,
                    label: Some("broken shotgun".to_string()),
                },
                Holds {
                    item: 3,
                    label: None,
                },
            ]
        );
        for bad in [
            r#"<bot format="2"><holds item="0"/></bot>"#,
            r#"<bot format="2"><holds item="5" slot="1"/></bot>"#,
            r#"<bot format="2"><holds/></bot>"#,
            r#"<bot><holds item="5"/></bot>"#,
        ] {
            assert!(parse(bad).is_err(), "{bad} was accepted");
        }
        // The index over the checked-in files: 102's broken shotgun.
        let pack = Pack::open(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mods/bot")).unwrap();
        let holds = load_holds(&pack, Some(true), None).unwrap();
        assert!(
            holds
                .iter()
                .any(|entry| entry.room == "102" && entry.fact.item == 0x1C),
            "{holds:?}"
        );
    }

    #[test]
    fn when_keeps_a_fact_for_matching_story_parameters() {
        let decls = super::super::params::parse(
            r#"<params format="1">
                <param name="richard" values="save|leave" default="save" label="l"/>
                <param name="first" values="yes|no" default="yes" label="l"/>
            </params>"#,
        )
        .unwrap();
        let mut params = Params::new(decls);
        let text = r#"<bot format="2">
            <answer choice="yes" message="23" when="first=yes"/>
            <answer choice="no" message="23" when="first=no"/>
            <deliver item="0x42" cut="3" bank="1" bit="0x61" when="richard=save" character="jill"/>
            <avoid slot="1" reason="x" when="richard!=save first=no"/>
            <ride slot="0" from="21C" when="richard=leave"/>
        </bot>"#;
        // Without parameters every fact is kept (syntax checked only).
        let all = parse(text).unwrap();
        assert_eq!(
            (all.answers.len(), all.errands.len(), all.avoids.len()),
            (2, 1, 1)
        );
        let defaults = parse_for(text, Some(true), Some(&params)).unwrap();
        assert_eq!(defaults.answers.len(), 1);
        assert!(defaults.answers[0].yes);
        assert_eq!(defaults.answers[0].message, Some(23));
        assert_eq!((defaults.errands.len(), defaults.avoids.len()), (1, 0));
        assert!(defaults.rides.is_empty());
        // `character` and `when` both have to hold.
        assert!(
            parse_for(text, Some(false), Some(&params))
                .unwrap()
                .errands
                .is_empty()
        );
        params.set("richard", "leave").unwrap();
        params.set("first", "no").unwrap();
        let other = parse_for(text, Some(true), Some(&params)).unwrap();
        assert!(!other.answers[0].yes);
        assert_eq!((other.errands.len(), other.avoids.len()), (0, 1));
        assert_eq!(other.rides.len(), 1);
        // An undeclared name or value fails, even on a fact the character
        // drops; a bad `when` fails without parameters too.
        for bad in [
            r#"<bot format="2"><avoid slot="1" reason="x" when="richrd=save"/></bot>"#,
            r#"<bot format="2"><avoid slot="1" reason="x" when="richard=sav" character="chris"/></bot>"#,
        ] {
            assert!(parse_for(bad, Some(true), Some(&params)).is_err(), "{bad}");
        }
        assert!(
            parse(r#"<bot format="2"><avoid slot="1" reason="x" when="richard"/></bot>"#).is_err()
        );
        assert!(parse(r#"<bot><puzzle id="p" when="a=b"/></bot>"#).is_err());
    }

    #[test]
    fn character_keeps_a_fact_for_one_character() {
        let text = r#"<bot format="2">
            <interact slot="3" bank="1" bit="0xA2" character="jill"/>
            <use slot="11" item="0x20" bank="1" bit="0xAB" character="jill"/>
            <use slot="8" item="0x20" bank="1" bit="0xA8" character="chris"/>
            <avoid slot="7" reason="x" character="jill"/>
            <answer choice="yes" slot="2" character="chris"/>
            <camera cut="1"/>
        </bot>"#;
        let all = parse(text).unwrap();
        assert_eq!((all.interacts.len(), all.uses.len()), (2, 2));
        let jills = parse_for(text, Some(true), None).unwrap();
        assert_eq!(jills.interacts.len(), 2);
        assert_eq!(
            jills.uses.iter().map(|u| u.slot).collect::<Vec<_>>(),
            vec![11]
        );
        assert_eq!((jills.avoids.len(), jills.answers.len()), (1, 0));
        let chris = parse_for(text, Some(false), None).unwrap();
        // The unconditioned camera step holds for both.
        assert_eq!(chris.interacts.len(), 1);
        assert_eq!(
            chris.uses.iter().map(|u| u.slot).collect::<Vec<_>>(),
            vec![8]
        );
        assert_eq!((chris.avoids.len(), chris.answers.len()), (0, 1));
        assert!(jill(1) && jill(3) && !jill(0) && !jill(2));

        let error = |text: &str| parse(text).unwrap_err().to_string();
        let bad = r#"<bot format="2"><avoid slot="1" reason="x" character="barry"/></bot>"#;
        assert!(
            error(bad).contains("`character` value `barry`"),
            "{}",
            error(bad)
        );
        let puzzle = r#"<bot><puzzle id="p" character="jill"/></bot>"#;
        assert!(
            error(puzzle).contains("unknown attribute `character`"),
            "{}",
            error(puzzle)
        );
        // A dropped fact is still checked.
        let typo =
            r#"<bot format="2"><use slot="1" item="0" bank="1" bit="1" character="chris"/></bot>"#;
        assert!(parse_for(typo, Some(true), None).is_err());
    }

    #[test]
    fn errand_facts_name_both_ends_of_an_item_errand() {
        let text = r#"<bot format="2">
            <deliver item="0x42" cut="3" bank="1" bit="0x61" after_bank="1" after_bit="0x62"
                     label="serum"/>
            <fetch item="0x42" slot="5" bank="1" bit="0x23"/>
        </bot>"#;
        let meta = parse(text).unwrap();
        assert_eq!(
            meta.errands,
            vec![
                Errand {
                    kind: ErrandKind::Deliver { cut: 3 },
                    item: 0x42,
                    flag: (1, 0x61),
                    after: Some((1, 0x62)),
                    label: Some("serum".to_string()),
                },
                Errand {
                    kind: ErrandKind::Fetch { slot: 5 },
                    item: 0x42,
                    flag: (1, 0x23),
                    after: None,
                    label: None,
                },
            ]
        );
        let error = |text: &str| parse(text).unwrap_err().to_string();
        let no_item = r#"<bot format="2"><fetch item="0" slot="5" bank="1" bit="1"/></bot>"#;
        assert!(error(no_item).contains("must not be 0"));
        let slot_on_deliver =
            r#"<bot format="2"><deliver item="1" slot="5" cut="1" bank="1" bit="1"/></bot>"#;
        assert!(error(slot_on_deliver).contains("slot"));
        let after_on_fetch = r#"<bot format="2"><fetch item="1" slot="5" bank="1" bit="1"
            after_bank="1" after_bit="2"/></bot>"#;
        assert!(error(after_on_fetch).contains("after_bank"));
        let v1 = r#"<bot><deliver item="1" cut="1" bank="1" bit="1"/></bot>"#;
        assert!(error(v1).contains("needs <bot format=\"2\">"));
    }

    #[test]
    fn the_errand_index_lists_the_serum_errand() {
        let pack = Pack::open(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mods/bot")).unwrap();
        let errands = load_errands(&pack, Some(true), None).unwrap();
        let rooms: Vec<(&str, ErrandKind)> = errands
            .iter()
            .map(|entry| (entry.room.as_str(), entry.fact.kind))
            .collect();
        assert_eq!(
            rooms,
            vec![
                ("100", ErrandKind::Fetch { slot: 5 }),
                // The shelf's second serum, the cure after Yawn's bite.
                ("100", ErrandKind::Fetch { slot: 5 }),
                ("20D", ErrandKind::Deliver { cut: 3 }),
            ]
        );
        assert!(
            load_errands(&pack, Some(false), None).unwrap().is_empty(),
            "Jill's only"
        );
    }

    #[test]
    fn watch_names_a_scene_to_let_play() {
        let text = r#"<bot format="2">
            <watch bank="1" bit="0xAD" after_bank="1" after_bit="0xAC" label="zombie"/>
            <watch bank="0" bit="3"/>
        </bot>"#;
        let meta = parse(text).unwrap();
        assert_eq!(
            meta.watches,
            vec![
                Watch {
                    flag: (1, 0xAD),
                    after: Some((1, 0xAC)),
                    label: Some("zombie".to_string()),
                },
                Watch {
                    flag: (0, 3),
                    after: None,
                    label: None,
                },
            ]
        );
        let error = |text: &str| parse(text).unwrap_err().to_string();
        let half = r#"<bot format="2"><watch bank="1" bit="1" after_bank="1"/></bot>"#;
        assert!(error(half).contains("both `after_bank` and `after_bit`"));
        let v1 = r#"<bot><watch bank="1" bit="1"/></bot>"#;
        assert!(error(v1).contains("needs <bot format=\"2\">"));
        let bad_bank =
            r#"<bot format="2"><watch bank="1" bit="1" after_bank="99" after_bit="1"/></bot>"#;
        assert!(error(bad_bank).contains("after_bank 99 is out of range"));
    }

    #[test]
    fn format_2_facts_are_validated() {
        let error = |text: &str| parse(text).unwrap_err().to_string();

        let v1 = r#"<bot format="1"><avoid slot="1" reason="x"/></bot>"#;
        assert!(error(v1).contains("<avoid> needs <bot format=\"2\">"));
        let v1_default = r#"<bot><interact slot="1" bank="0" bit="1"/></bot>"#;
        assert!(error(v1_default).contains("needs <bot format=\"2\">"));
        assert!(error(r#"<bot format="3"/>"#).contains("format 3 is not supported"));

        let unknown = r#"<bot format="2"><prompt slot="1"/></bot>"#;
        assert!(error(unknown).contains("unknown <bot> element <prompt>"));
        let child =
            r#"<bot format="2"><avoid slot="1" reason="x"><walk x="1" z="2"/></avoid></bot>"#;
        assert!(error(child).contains("unknown <avoid> element <walk>"));

        let bad_attribute = r#"<bot format="2"><interact slot="1" bank="0" bit="1" x="5"/></bot>"#;
        assert!(error(bad_attribute).contains("unknown attribute `x`"));
        let bad_choice = r#"<bot format="2"><answer choice="maybe" slot="1"/></bot>"#;
        assert!(error(bad_choice).contains("is not `yes` or `no`"));
        let no_target = r#"<bot format="2"><answer choice="yes"/></bot>"#;
        assert!(error(no_target).contains("needs a `slot`, a `bank`/`bit` flag or a `message`"));
        let half_flag = r#"<bot format="2"><answer choice="yes" bank="1"/></bot>"#;
        assert!(error(half_flag).contains("both `bank` and `bit`"));
        let no_reason = r#"<bot format="2"><avoid slot="1"/></bot>"#;
        assert!(error(no_reason).contains("needs a `reason` attribute"));
        let blank_reason = r#"<bot format="2"><avoid slot="1" reason=" "/></bot>"#;
        assert!(error(blank_reason).contains("reason must not be empty"));

        let bad_bit = r#"<bot format="2"><interact slot="1" bank="0" bit="256"/></bot>"#;
        assert!(error(bad_bit).contains("`bit` value `256` is not a valid u8"));
        let bad_bank = r#"<bot format="2"><interact slot="1" bank="10" bit="1"/></bot>"#;
        assert!(error(bad_bank).contains("bank 10 is out of range"));
        let bad_answer_bank = r#"<bot format="2"><answer choice="no" bank="12" bit="1"/></bot>"#;
        assert!(error(bad_answer_bank).contains("bank 12 is out of range"));
        let half_gate = r#"<bot format="2"><avoid slot="1" bank="1" bit="2" reason="x"/></bot>"#;
        assert!(error(half_gate).contains("all of `bank`, `bit` and `while`"));
        let bad_while =
            r#"<bot format="2"><avoid slot="1" bank="1" bit="2" while="on" reason="x"/></bot>"#;
        assert!(error(bad_while).contains("`while` value `on` is not `set` or `clear`"));
        let bad_gate_bank =
            r#"<bot format="2"><avoid slot="1" bank="10" bit="2" while="set" reason="x"/></bot>"#;
        assert!(error(bad_gate_bank).contains("bank 10 is out of range"));
        let bad_held = r#"<bot format="2"><avoid slot="1" unless_held="300" reason="x"/></bot>"#;
        assert!(error(bad_held).contains("`unless_held` value `300` is not a valid u8"));
        let bad_slot = r#"<bot format="2"><avoid slot="-1" reason="x"/></bot>"#;
        assert!(error(bad_slot).contains("`slot` value `-1` is not a valid u8"));
    }

    #[test]
    fn load_reads_the_room_file_and_ignores_an_absent_one() {
        let temp = TempDir::new("load");
        let room = temp.path.join("room/117");
        fs::create_dir_all(&room).unwrap();
        fs::write(
            room.join("bot.xml"),
            br#"<bot format="1">
                <puzzle id="paintings"><step slot="8" bank="0" bit="0x03" label="last">
                    <walk x="2900" z="4100"/><face x="1850" z="4100"/>
                </step></puzzle>
            </bot>"#,
        )
        .unwrap();

        let pack = Pack::open(&temp.path).unwrap();
        let meta = load(&pack, room_117(), &Params::default())
            .unwrap()
            .expect("the room metadata");
        assert_eq!(meta.puzzles[0].steps[0].slot, 8);
        assert_eq!(meta.puzzles[0].steps[0].walks, vec![[2900, 4100]]);
        assert_eq!(meta.puzzles[0].steps[0].face, Some([1850, 4100]));

        // A directory pack reads lazily, so a rewrite is visible to the next
        // load without reopening the pack (the --watch path).
        fs::write(
            room.join("bot.xml"),
            br#"<bot format="1">
                <puzzle id="paintings"><step slot="1" bank="0" bit="0x02" label="changed">
                    <face x="1" z="2"/>
                </step></puzzle>
            </bot>"#,
        )
        .unwrap();
        let changed = load(&pack, room_117(), &Params::default())
            .unwrap()
            .unwrap();
        assert_eq!(changed.puzzles[0].steps[0].label, "changed");
        assert_eq!(changed.puzzles[0].steps[0].walks, Vec::<[i32; 2]>::new());
        assert_eq!(changed.puzzles[0].steps[0].face, Some([1, 2]));

        let absent = RoomId {
            stage: 1,
            room: 0x18,
            player_flag: 1,
            mansion2: false,
        };
        assert_eq!(load(&pack, absent, &Params::default()).unwrap(), None);
    }
}
