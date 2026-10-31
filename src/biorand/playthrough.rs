//! A bot that plays a generated game through the real app, for the checks only
//! the running engine can make: doors arrive where they should, pickups give
//! what the script says, keys open their gates, the finale ends the game.
//!
//! It stands the player in front of what it wants rather than walking there,
//! it keeps the player's health up, and it removes enemies after watching them,
//! so it says nothing about how hard a game is. Set `ARKLAY_BIORAND_PACK` to a
//! generated pack, and optionally `ARKLAY_BIORAND_FRAMES` to a directory for
//! screenshots:
//!
//! `cargo test --profile test-release --lib biorand_playthrough -- --ignored --nocapture`

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::*;

const FLARE: u8 = 0x2A;
const GOAL_ROOM: u16 = 0x303;
/// The third action zone of the goal room, where the flare can be used.
const FLARE_ZONE: [i32; 3] = [12700, 0, 28000];
const HEALTH: i16 = 140;
/// The player states an enemy's grab holds the player in.
const PLAYER_GRABBED: [u8; 2] = [5, 6];
const BANK_LOCKS: u8 = 2;
const BANK_ROOM: u8 = 4;
const ROOM_CALLED: u8 = 4;
/// Raised by the script when the Tyrant's entrance has finished.
const ROOM_ENTERED: u8 = 7;
const TYRANT: u8 = 0x10;
const LAUNCHER: u8 = 0x0A;
const LAUNCHER_SLOT: u8 = 1;
/// Where the goal room's door sets the player down: clear of the flare's zones.
const CLEAR_OF_FLARE: [i32; 3] = [29130, 0, 5700];

#[derive(Clone, Copy, Debug)]
struct KnownDoor {
    slot: u8,
    target: u16,
    key: Option<u8>,
    flag: u8,
    arrival: [i32; 3],
    arrival_angle: u16,
    camera: u8,
}

struct Bot {
    app: App,
    input: InputState,
    confirm: u64,
    cancel: u64,
    fire: u64,
    aim: u64,
    frames: Option<PathBuf>,
    shot: u32,
    damage_seen: BTreeSet<u8>,
    moved_seen: BTreeSet<u8>,
    enemies_seen: BTreeMap<u16, BTreeSet<u8>>,
}

fn word(text: &str) -> u64 {
    InputScript::parse(&format!("0 {text}\n"))
        .unwrap()
        .word_at(0)
}

impl Bot {
    fn session(&mut self) -> &mut GameSession {
        match &mut self.app.mode {
            Mode::Play(session) => session,
            _ => panic!("the game is not in play"),
        }
    }

    fn playing(&self) -> bool {
        matches!(self.app.mode, Mode::Play(_))
    }

    fn room(&mut self) -> u16 {
        let id = self.session().game.id;
        (u16::from(id.stage) << 8) | u16::from(id.room)
    }

    fn tick(&mut self, buttons: u64) {
        if let Mode::Play(session) = &mut self.app.mode {
            session.game.entities[0].health = HEALTH;
            session.game.set_health_copy(HEALTH);
        }
        let tick = self.input.tick_scripted(buttons);
        self.app.update(tick.ui, tick.player, tick.action).unwrap();
        self.app.post_present();
    }

    fn snap(&mut self, label: &str) {
        let Some(dir) = self.frames.clone() else {
            return;
        };
        self.shot += 1;
        let path = dir.join(format!("{:03}-{label}.bmp", self.shot));
        self.app.draw();
        capture_bmp(&presented_image(self.app.frame()), &path).unwrap();
    }

    /// Whether a window, door animation or film holds the game.
    fn busy(&mut self) -> bool {
        if !self.playing() {
            return true;
        }
        let session = self.session();
        session.transition.is_some()
            || session.pickup_view.is_some()
            || session.boot_message.is_some()
            || session.room_frozen()
            || session.game.message.phase() != crate::message::MessagePhase::Idle
    }

    /// Press through whatever holds the game until the player has control.
    fn settle(&mut self, limit: u32) {
        for tick in 0..limit {
            if !self.busy() {
                // One more calm tick lets a window that just closed finish.
                self.tick(0);
                if !self.busy() {
                    return;
                }
            }
            // A used-up key's discard prompt leaves the status screen open.
            let session = self.session();
            let in_menu = session.menu.is_some() && session.pickup_view.is_none();
            let press = match (tick % 4 == 0, in_menu) {
                (false, _) => 0,
                (true, false) => self.confirm,
                (true, true) => self.cancel,
            };
            self.tick(press);
        }
        let room = self.room();
        let session = self.session();
        panic!(
            "the game stayed busy for {limit} ticks in room {room:03x}: transition {}, pickup {}, \
             boot {:?}, message {:?}, fade {:?} menu {} box {} file {} map {} save {} debug {} \
             title {} computer {} system {}",
            session.transition.is_some(),
            session.pickup_view.is_some(),
            session.boot_message,
            session.game.message.phase(),
            session.menu_fade_phase,
            session.menu.is_some(),
            session.item_box.is_some(),
            session.file.is_some(),
            session.map.is_some(),
            session.save_screen.is_some(),
            session.debug_menu.is_some(),
            session.return_title.is_some(),
            session.computer.is_some(),
            session.system_menu_open,
        );
    }

    /// Stand facing `angle` so the reach point is on `zone`'s centre and press
    /// the action.
    fn press_at(&mut self, zone: [i16; 4], angle: u16) {
        let center = [
            i32::from(zone[0]) + i32::from(zone[2]) / 2,
            i32::from(zone[1]) + i32::from(zone[3]) / 2,
        ];
        let (dx, dz) = player::reach_offset(angle);
        let session = self.session();
        session.player.pos = [center[0] - dx, 0, center[1] - dz];
        session.player.angle = angle;
        session.game.sync_entity_from_player(&session.player);
        self.tick(self.confirm);
    }

    /// Press until the game responds, from each side in turn: furniture can
    /// push the player off one, and the action key is ignored for a moment
    /// after a window closes.
    fn press_until_busy(&mut self, zone: [i16; 4]) {
        for attempt in 0..16u16 {
            self.press_at(zone, (attempt % 4) * 1024);
            self.tick(0);
            if self.busy() {
                return;
            }
            for _ in 0..8 {
                self.tick(0);
            }
        }
    }

    fn enemies(&mut self) -> Vec<(usize, u8, [i32; 3])> {
        let session = self.session();
        session
            .game
            .entities
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, entity)| entity.active())
            .map(|(slot, entity)| (slot, entity.id, entity.pos))
            .collect()
    }

    /// Stand by and see what the room's enemies do, then clear them.
    fn watch_enemies(&mut self, ticks: u32) {
        let room = self.room();
        let before = self.enemies();
        let ids: BTreeSet<u8> = before.iter().map(|enemy| enemy.1).collect();
        self.enemies_seen.insert(room, ids);
        for tick in 0..ticks {
            let health = {
                let session = self.session();
                session.game.entities[0].health = HEALTH;
                session.game.set_health_copy(HEALTH);
                session.game.entities[0].health
            };
            self.tick(0);
            let hurt = self.session().game.entities[0].health < health;
            if hurt {
                self.damage_seen.extend(before.iter().map(|enemy| enemy.1));
            }
            if tick == ticks / 2 {
                self.snap(&format!("{room:03x}-enemies"));
            }
        }
        let after = self.enemies();
        for (slot, id, pos) in &before {
            if after
                .iter()
                .any(|enemy| enemy.0 == *slot && enemy.2 != *pos)
            {
                self.moved_seen.insert(*id);
            }
        }
        // The goal room's enemy is the finale.
        if room == GOAL_ROOM {
            return;
        }
        // An enemy taken away mid-grab would leave the player held for good.
        for _ in 0..1200 {
            if !PLAYER_GRABBED.contains(&self.session().game.entities[0].state()) {
                break;
            }
            self.tick(0);
        }
        let session = self.session();
        let state = session.game.entities[0].state();
        assert!(!PLAYER_GRABBED.contains(&state), "held in room {room:03x}");
        for (slot, _, _) in &after {
            session.game.entities[*slot].status_flags = 0;
        }
    }

    fn known_doors(&mut self) -> Vec<KnownDoor> {
        let room = self.room();
        let stage = (room >> 8) as u8;
        self.session()
            .game
            .doors
            .iter()
            .flatten()
            .filter(|door| !(door.lock & 0x80 != 0 && door.key == 0xFF))
            .filter(|door| door.lock & 0x40 == 0)
            .map(|door| KnownDoor {
                slot: door.slot,
                target: crate::biorand::room_of(door.next_room, stage),
                key: (door.lock & 0x80 != 0).then_some(door.key),
                flag: door.lock & 0x3F,
                arrival: door.next_pos,
                arrival_angle: door.next_angle as u16,
                camera: door.camera,
            })
            .collect()
    }

    /// Take every item slot of this room there is room for.
    fn take_items(&mut self, wanted: &BTreeSet<u8>, log: &mut Vec<String>) {
        let room = self.room();
        let slots: Vec<(u8, [i16; 4], u8)> = self
            .session()
            .game
            .room_actions
            .iter()
            .flatten()
            .filter_map(|action| Some((action.slot, action.zone, action.item_data?[0])))
            .collect();
        for (slot, zone, item) in slots {
            if !wanted.contains(&slot) {
                continue;
            }
            let game = &self.session().game;
            let held = game.inventory.iter().filter(|stack| stack.id != 0).count();
            let free = held < game.inventory_capacity();
            // Two slots can share a zone, the first taking the press: leave
            // nothing behind.
            if !free {
                self.make_space();
            }
            let held = self.count(item);
            // A press plays the reach-down animation before the viewer opens.
            for attempt in 0..12u16 {
                self.press_at(zone, (attempt % 4) * 1024);
                for _ in 0..60 {
                    self.tick(0);
                    if self.busy() {
                        self.settle(1200);
                        break;
                    }
                }
                if self.count(item) > held {
                    break;
                }
            }
            let now = self.count(item);
            let session = self.session();
            let inventory: Vec<(u8, u8)> = session
                .game
                .inventory
                .iter()
                .map(|stack| (stack.id, stack.quantity))
                .collect();
            assert!(
                now > held,
                "room {room:03x} slot {slot} did not give item {item:#x}: inventory {inventory:x?} \
                 capacity {} action {:x?} player {:?} locked {:?} weapon {} msg {:?} log:\n{}",
                session.game.inventory_capacity(),
                session.game.room_actions[usize::from(slot)],
                session.player.pos,
                session.player.locked,
                session.player.weapon.behavior,
                session.game.message.phase(),
                log.join("\n")
            );
            log.push(format!("{room:03x}: slot {slot} gave item {item:#04x}"));
            self.snap(&format!("{room:03x}-pickup-{item:02x}"));
        }
    }

    /// Drop a supply so a key fits: the bot has an item box only in the start
    /// room, and what it wants to see is the route.
    fn make_space(&mut self) {
        let game = &mut self.session().game;
        let spare = game
            .inventory
            .iter()
            .rev()
            .map(|stack| stack.id)
            .find(|id| !(0x2A..=0x3F).contains(id) && *id > 2);
        if let Some(item) = spare {
            game.item_remove(item);
        }
    }

    fn count(&mut self, item: u8) -> u32 {
        self.session()
            .game
            .inventory
            .iter()
            .filter(|stack| stack.id == item)
            .map(|stack| u32::from(stack.quantity).max(1))
            .sum()
    }

    /// Go through `door`, turning its key first if it has one.
    fn use_door(&mut self, door: KnownDoor, log: &mut Vec<String>) {
        let from = self.room();
        let live = |bot: &mut Self| {
            bot.session().game.doors[usize::from(door.slot)]
                .expect("the door is registered")
                .zone
        };
        let needs_key = door.key.is_some()
            && !self.session().game.flags[usize::from(BANK_LOCKS)].bit(door.flag);
        if needs_key {
            let key = door.key.unwrap();
            assert!(self.session().game.has_item(key), "no key {key:#x}");
            let zone = live(self);
            self.press_until_busy(zone);
            self.settle(600);
            assert!(
                self.session().game.flags[usize::from(BANK_LOCKS)].bit(door.flag),
                "key {key:#x} did not open the door in {from:03x}"
            );
            log.push(format!(
                "{from:03x}: key {key:#04x} opened door {}",
                door.slot
            ));
        }
        let mut attempts = 0;
        for tick in 0..4000 {
            if self.playing() && self.room() != from && !self.busy() {
                break;
            }
            if tick % 40 == 0 && self.room() == from && !self.busy() {
                attempts += 1;
                let zone = live(self);
                self.press_at(zone, (attempts as u16 % 4) * 1024);
            } else {
                let press = self.busy() && tick % 8 == 0;
                self.tick(if press { self.confirm } else { 0 });
            }
        }
        if attempts != 1 {
            log.push(format!(
                "{from:03x}: door {} took {attempts} presses",
                door.slot
            ));
        }
        let room = self.room();
        assert_eq!(room, door.target, "door {} of {from:03x}", door.slot);
        self.settle(1200);
        self.snap(&format!("{room:03x}-from-{from:03x}"));
        let session = self.session();
        assert_eq!(
            [session.player.pos[0], session.player.pos[2]],
            [door.arrival[0], door.arrival[2]],
            "arrival in {room:03x} from {from:03x}"
        );
        assert_eq!(
            session.player.angle, door.arrival_angle,
            "arrival angle in {room:03x}"
        );
        let camera = session.game.camera.current_cut;
        log.push(format!(
            "{from:03x} -> {room:03x}: camera {camera} (door says {})",
            door.camera & 0x3F
        ));
    }
}

fn bfs_next(
    from: u16,
    goal: impl Fn(u16) -> bool,
    known: &BTreeMap<u16, Vec<KnownDoor>>,
    can_open: &impl Fn(&KnownDoor) -> bool,
) -> Option<KnownDoor> {
    let mut seen = BTreeSet::from([from]);
    let mut queue: VecDeque<(u16, Option<KnownDoor>)> = VecDeque::from([(from, None)]);
    while let Some((room, first)) = queue.pop_front() {
        for door in known.get(&room).into_iter().flatten() {
            if !can_open(door) {
                continue;
            }
            let step = first.unwrap_or(*door);
            if goal(door.target) {
                return Some(step);
            }
            if seen.insert(door.target) {
                queue.push_back((door.target, Some(step)));
            }
        }
    }
    None
}

#[test]
#[ignore = "requires ARKLAY_BIORAND_PACK"]
fn plays_a_generated_game_to_its_ending() {
    let Some(path) = std::env::var_os("ARKLAY_BIORAND_PACK").map(PathBuf::from) else {
        return;
    };
    let frames = std::env::var_os("ARKLAY_BIORAND_FRAMES").map(PathBuf::from);
    if let Some(dir) = &frames {
        std::fs::create_dir_all(dir).unwrap();
    }
    let saves = std::env::temp_dir().join(format!("biorand-saves-{}", std::process::id()));
    let pack = Pack::open(&path).unwrap();
    let rooms_in_pack: BTreeSet<u16> = pack
        .own_paths()
        .filter_map(crate::biorand::script_room)
        .collect();
    let slots = crate::biorand::slots_in(&pack).unwrap();
    let mut bot = Bot {
        app: App::new(pack, saves, false),
        input: InputState::default(),
        confirm: word("confirm"),
        cancel: word("cancel"),
        fire: word("fire"),
        aim: word("aim"),
        frames,
        shot: 0,
        damage_seen: BTreeSet::new(),
        moved_seen: BTreeSet::new(),
        enemies_seen: BTreeMap::new(),
    };
    let mut log = Vec::new();

    // The title's new game: the seed's character, with no select screen.
    let _ = bot.app.apply(ScreenAction::CharSelect).unwrap();
    assert!(bot.playing(), "a new game goes straight to play");
    bot.settle(2000);
    bot.snap("start");
    let character = bot.session().game.player_choice().character;
    log.push(format!(
        "character {character}, start room {:03x}",
        bot.room()
    ));

    let mut known: BTreeMap<u16, Vec<KnownDoor>> = BTreeMap::new();
    let mut visited: BTreeSet<u16> = BTreeSet::new();
    let mut emptied: BTreeSet<u16> = BTreeSet::new();
    for _ in 0..400 {
        let room = bot.room();
        if visited.insert(room) {
            bot.snap(&format!("{room:03x}-arrive"));
            bot.watch_enemies(90);
        }
        known.insert(room, bot.known_doors());
        if emptied.insert(room) {
            let wanted = slots.get(&room).cloned().unwrap_or_default();
            bot.take_items(&wanted, &mut log);
        }
        if visited == rooms_in_pack {
            break;
        }
        let held: BTreeSet<u8> = bot.session().game.inventory.iter().map(|s| s.id).collect();
        let open_flags: BTreeSet<u8> = (0u8..64)
            .filter(|&bit| bot.session().game.flags[usize::from(BANK_LOCKS)].bit(bit))
            .collect();
        let can_open = |door: &KnownDoor| {
            door.key.is_none_or(|key| held.contains(&key)) || open_flags.contains(&door.flag)
        };
        let next = bfs_next(room, |target| !visited.contains(&target), &known, &can_open)
            .unwrap_or_else(|| {
                panic!(
                    "stuck in {room:03x} with {held:?}; visited {visited:?}\n{known:#?}\n{}",
                    log.join("\n")
                )
            });
        bot.use_door(next, &mut log);
    }
    assert_eq!(visited, rooms_in_pack, "the bot saw every room");

    // Walk back to the goal room through real doors, proving the way back.
    while bot.room() != GOAL_ROOM {
        let room = bot.room();
        let next = bfs_next(room, |target| target == GOAL_ROOM, &known, &|_| true).unwrap();
        bot.use_door(next, &mut log);
    }
    assert!(
        bot.session().game.has_item(FLARE),
        "the flare was in a slot the bot took"
    );

    // Use the flare the way the menu does, standing in the zone.
    let session = bot.session();
    session.player.pos = FLARE_ZONE;
    session.game.sync_entity_from_player(&session.player);
    session.game.record_used_item(FLARE);
    log.push("303: flare used".to_string());
    let mut tyrant_seen = false;
    let mut left_zone = false;
    let mut launcher_taken = false;
    let mut shots = 0u32;
    let mut ticks = 0u32;
    while bot.playing() && bot.app.ending.is_none() && ticks < 40_000 {
        ticks += 1;
        let session = bot.session();
        // The Tyrant waits for the player to walk away from the flare's zones.
        if session.game.flags[usize::from(BANK_ROOM)].bit(ROOM_CALLED) && !left_zone {
            left_zone = true;
            session.player.pos = CLEAR_OF_FLARE;
            session.game.sync_entity_from_player(&session.player);
        }
        // Its entrance has finished once the fight flag has been up.
        let tyrant = session
            .game
            .entities
            .iter()
            .position(|entity| entity.active() && entity.id == TYRANT);
        let fight = tyrant.filter(|_| session.game.flags[usize::from(BANK_ROOM)].bit(ROOM_ENTERED));
        if fight.is_some() && !tyrant_seen {
            tyrant_seen = true;
            log.push(format!("303: Tyrant in the fight after {ticks} ticks"));
            bot.snap("303-tyrant");
        }
        if ticks.is_multiple_of(900) {
            bot.snap("303-finale");
        }
        // Whatever holds the game first: Brad's drop plays as a scene.
        if bot.busy() {
            bot.tick(if ticks.is_multiple_of(16) {
                bot.confirm
            } else {
                0
            });
            continue;
        }
        // Brad drops the launcher when the Tyrant is hurt or time has passed: the
        // only weapon that kills this Tyrant. The script arms its slot.
        let launcher_slot = bot.session().game.room_actions[usize::from(LAUNCHER_SLOT)]
            .filter(|action| action.handler != 0);
        if !launcher_taken {
            if let Some(action) = launcher_slot
                && ticks.is_multiple_of(30)
            {
                if bot
                    .session()
                    .game
                    .inventory
                    .iter()
                    .all(|stack| stack.id != 0)
                {
                    bot.make_space();
                }
                bot.press_at(action.zone, (ticks / 30 % 4) as u16 * 1024);
                for _ in 0..60 {
                    bot.tick(0);
                    if bot.busy() {
                        bot.settle(1200);
                        break;
                    }
                }
                if bot.playing() && bot.count(LAUNCHER) > 0 {
                    launcher_taken = true;
                    log.push(format!("303: rocket launcher taken at tick {ticks}"));
                }
                continue;
            }
            bot.tick(0);
            continue;
        }
        let Some(slot) = fight else {
            bot.tick(0);
            continue;
        };
        let session = bot.session();
        session.game.set_equipped(Some(LAUNCHER));
        let pos = session.game.entities[slot].pos;
        let me = session.player.pos;
        session.player.angle = crate::biorand::angle_towards(me, pos);
        session.game.sync_entity_from_player(&session.player);
        let fire = ticks % 48 == 47;
        shots += u32::from(fire);
        bot.tick(bot.aim | if fire { bot.fire } else { 0 });
    }
    assert!(tyrant_seen, "the Tyrant never came:\n{}", log.join("\n"));
    let flags = |bot: &mut Bot, bank: usize| -> Vec<u8> {
        (0u8..=255)
            .filter(|&bit| bot.playing() && bot.session().game.flags[bank].bit(bit))
            .collect()
    };
    let (room, status, enemy) = (flags(&mut bot, 4), flags(&mut bot, 5), flags(&mut bot, 3));
    assert!(
        bot.app.ending.is_some(),
        "no ending after {ticks} ticks and {shots} shots, launcher {launcher_taken}, playing {}, room \
         {room:x?} status {status:x?} enemy {enemy:x?}\n{}",
        bot.playing(),
        log.join("\n")
    );
    log.push(format!("303: ending after {ticks} ticks, {shots} shots"));

    // Let the ending chain run its films and screens to the end.
    let mut modes = Vec::new();
    for tick in 0..60_000u32 {
        let name = match bot.app.mode {
            Mode::Movie(_) => "film",
            Mode::Ending(_) => "result",
            Mode::Epilogue(_) => "epilogue",
            Mode::Save(_) => "save",
            Mode::Title(_) => "title",
            Mode::Play(_) => "play",
            _ => "other",
        };
        if modes.last() != Some(&name) {
            modes.push(name);
            bot.snap(&format!("ending-{name}"));
        }
        if name == "title" {
            break;
        }
        bot.tick(if tick % 8 == 0 { bot.confirm } else { 0 });
    }
    log.push(format!("ending chain: {modes:?}"));
    assert_eq!(modes.last(), Some(&"title"), "the ending reached the title");

    log.push(format!("enemies by room: {:x?}", bot.enemies_seen));
    log.push(format!("enemy types that moved: {:x?}", bot.moved_seen));
    log.push(format!("enemy types near damage: {:x?}", bot.damage_seen));
    println!("{}", log.join("\n"));
}
