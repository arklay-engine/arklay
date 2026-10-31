//! RE2's script characters: `_Spl`, which runs ids 0x40 to 0x5B in the enemy
//! slots. Only `Spl_Scenario` is here so far, the routine scripts steer with.

use crate::enemy::EntityModelCache;
use crate::game::GameState;
use crate::model::{Clip, Emd, Keyframe};
use crate::pack::Pack;
use crate::player2::{self, Banks, Tick};
use crate::scd2::host::{Character, NO_ENEMY_FLAG, SPLAYER_SLOT};
use crate::state::RoomState;

/// `Game_loop`'s pass over the enemy slots, for the characters `_Spl` runs.
pub fn tick(
    game: &mut GameState,
    room: &RoomState,
    pack: &Pack,
    npc_models: &mut EntityModelCache,
) {
    init(game, room, pack, npc_models);
    let scenario = game.id.player_flag & 1;
    for slot in run_order() {
        if game.re2.character_stopped(slot) {
            continue;
        }
        let Some(mut character) = game.re2.characters.0.get_mut(slot).and_then(Option::take) else {
            continue;
        };
        let model = npc_models.get(pack, character.id, scenario);
        if let Some(mut zombie) = character.zombie.take() {
            if let Some(model) = model.filter(|_| moves(&character)) {
                run_zombie(game, room, &mut character, &mut zombie, slot, &model);
            }
            character.zombie = Some(zombie);
            game.re2.characters.0[slot] = Some(character);
            continue;
        }
        if let Some(model) = model.filter(|_| moves(&character)) {
            let tick = tick_of(room, game, &character, slot, &model);
            let arrived = spl(&mut character, &tick);
            if let Some(flag) = arrived {
                game.re2.raise_room_flag(flag);
            }
            // `_Spl`'s `Sca_ck_em`.
            if let Some(sca) = &room.sca {
                let body = &mut character.body;
                let [old_x, old_z] = body.collision_mirror;
                let prev = [old_x, body.pos[1], old_z];
                let mask = sca_mask(character.id);
                let resolved = crate::sca::resolve_masked(sca, mask, prev, body.pos, body.radius);
                body.pos = resolved.pos;
                body.collision_mirror = [body.pos[0], body.pos[2]];
            }
        }
        game.re2.characters.0[slot] = Some(character);
    }
}

/// What the player's routine sees of the slots, in `enemy_work`'s order.
pub fn targets(game: &GameState) -> Vec<crate::weapon2::Target> {
    let characters = game.re2.characters.0.iter().enumerate();
    characters
        .filter(|(slot, _)| *slot != SPLAYER_SLOT)
        .filter_map(|(slot, character)| {
            let character = character.as_deref()?;
            let zombie = character.zombie.as_ref()?;
            let pos = character.body.pos;
            Some(crate::weapon2::Target {
                slot,
                id: character.id,
                be_flg: character.be_flg,
                kind: character.work.kind,
                life: zombie.life,
                damage_cnt: zombie.damage_cnt,
                pos,
                at: [zombie.atd.at[0], zombie.atd.at[2]],
                at_w: zombie.atd.at_w,
                at_w1: 0,
                neck_y: character.work.neck.position.map_or(pos[1], |neck| neck[1]),
            })
        })
        .collect()
}

/// What the player's tick asked of the effects and the slots: the shot's
/// `Esp_call`s, and what it found goes to who it hit.
pub fn take_hit(game: &mut GameState) {
    for call in std::mem::take(&mut game.re2.player.effects) {
        game.re2
            .effects
            .call(call.kind, call.row, call.size, call.dir, call.at, false);
    }
    let Some(hit) = game.re2.player.hit.take() else {
        return;
    };
    let from = game.entities[0].pos;
    let Some(character) = game
        .re2
        .characters
        .0
        .get_mut(hit.slot)
        .and_then(Option::as_mut)
    else {
        return;
    };
    if let Some(mut zombie) = character.zombie.take() {
        crate::zombie2::take_hit(character, &mut zombie, &hit, from);
        character.zombie = Some(zombie);
    }
}

/// `Game_loop`'s call of `Em_Zombie`, with its view of the rest of the game.
fn run_zombie(
    game: &mut GameState,
    room: &RoomState,
    character: &mut Character,
    zombie: &mut crate::zombie2::Zombie,
    slot: usize,
    model: &Emd,
) {
    let tick = tick_of(room, game, character, slot, model);
    let player = game.entities[0].pos;
    // `player_work`'s bodies: the player, then the slots.
    let mut bodies = vec![crate::zombie2::Oba {
        slot: 0,
        be_flg: 1,
        atd: crate::zombie2::Atd {
            ofs_y: -0x5FA,
            at_w: PLAYER_WIDTH,
            at_h: 0x5FA,
            w: PLAYER_WIDTH,
            at: [player[0], player[1] - 0x5FA, player[2]],
        },
    }];
    let in_play = game.re2.characters.0.iter().flatten();
    bodies.extend(
        in_play
            .filter(|other| other.be_flg & 1 != 0)
            .map(|other| crate::zombie2::Oba {
                slot: 0,
                be_flg: other.be_flg,
                atd: other.zombie.as_ref().map_or_else(
                    || crate::zombie2::Atd {
                        ofs_y: -0x5FA,
                        at_w: other.body.radius as u16,
                        at_h: 0x5FA,
                        w: other.body.radius as u16,
                        at: [
                            other.body.pos[0],
                            other.body.pos[1] - 0x5FA,
                            other.body.pos[2],
                        ],
                    },
                    |zombie| zombie.atd,
                ),
            }),
    );
    let sca = room
        .sca
        .as_ref()
        .map_or(&[][..], |sca| sca.records.as_slice());
    let partner = game
        .re2
        .characters
        .0
        .get(SPLAYER_SLOT)
        .is_some_and(|other| other.as_deref().is_some_and(|other| other.be_flg & 1 != 0));
    let stopped = game.re2.character_stopped(slot);
    let easy = game.re2.flag(0, FG_SYSTEM_EASY);
    let enemies = game.re2.enemies;
    let player_clip = game.re2.player.move_no;
    let noise = game.re2.player.noise;
    let player_neck = game.re2.player.neck.position;
    let stage = game.id.stage;
    let re2 = &mut game.re2;
    let effects = &mut re2.effects;
    let mut world = crate::zombie2::World {
        player,
        player_neck,
        player_floor: crate::sca::floor_of(player[1]),
        player_clip,
        lock: &mut re2.player_lock,
        noise: u16::from(noise),
        rnd: &mut re2.rnd,
        partner,
        easy,
        enemies,
        stopped,
        bodies: &bodies,
        guides: &room.guides,
        sca,
        effects,
    };
    crate::zombie2::em_zombie(character, zombie, &tick, &mut world);
    let pos = character.body.pos;
    if std::mem::take(&mut zombie.died) && zombie.dead_flag != NO_ENEMY_FLAG {
        let bank = re2.enemy_flag_bank(stage);
        re2.set_flag(bank, zombie.dead_flag, 1);
    }
    for sound in zombie.sounds.drain(..) {
        let id = 3 << 24 | u32::from(sound) << 16;
        if let Some(id) = crate::se2::SoundId::decode(id) {
            re2.sounds.push(crate::scd2::host::SoundRequest { id, pos });
        }
    }
}

/// `Player_set`'s `At_w`.
const PLAYER_WIDTH: u16 = 450;
/// `fg_system`'s `0x20`, easy mode's bit.
const FG_SYSTEM_EASY: u8 = 26;

/// `Sca_ck_em`'s mask: the children's is lower, and a zombie's its own.
pub fn sca_mask(id: u8) -> u16 {
    if (0x10..0x20).contains(&id) {
        crate::zombie2::MASK
    } else if matches!(id, 0x45 | 0x4F) {
        crate::sca::CHARACTER_MASK_LOW
    } else {
        crate::sca::CHARACTER_MASK
    }
}

/// `Game_loop` runs what's in play, unless its `Type` says not to: how a script
/// sends a character away for good.
fn moves(character: &Character) -> bool {
    character.be_flg & 1 != 0 && character.work.kind & player2::KIND_NOT_MOVED == 0
}

/// `Em_init_move`, the room load's first run of the characters it placed. It
/// waits for the room and the pack, which the scripts' first pass hasn't got,
/// so it's here for whatever needs a pose or a position first.
pub fn init(
    game: &mut GameState,
    room: &RoomState,
    pack: &Pack,
    npc_models: &mut EntityModelCache,
) {
    let scenario = game.id.player_flag & 1;
    for slot in run_order() {
        let Some(mut character) = game.re2.characters.0.get_mut(slot).and_then(Option::take) else {
            continue;
        };
        if character.with_room
            && let Some(model) = npc_models.get(pack, character.id, scenario)
        {
            if let Some(mut zombie) = character.zombie.take() {
                character.with_room = false;
                let held = character.work.kind & player2::KIND_NOT_MOVED;
                character.work.kind &= !player2::KIND_NOT_MOVED;
                run_zombie(game, room, &mut character, &mut zombie, slot, &model);
                let (start, flags) = character.start;
                if start != 0 {
                    player2::spl_start(&mut character.work, start, flags);
                    run_zombie(game, room, &mut character, &mut zombie, slot, &model);
                }
                character.work.kind |= held;
                character.zombie = Some(zombie);
                game.re2.characters.0[slot] = Some(character);
                continue;
            }
            let tick = tick_of(room, game, &character, slot, &model);
            let arrived = em_init_move(&mut character, &tick);
            if let Some(flag) = arrived {
                game.re2.raise_room_flag(flag);
            }
        }
        game.re2.characters.0[slot] = Some(character);
    }
    // The pass of `Game_loop` that ends a load runs `Esp_move`, and little else.
    if std::mem::take(&mut game.re2.load_pass) {
        game.re2.tick_effects(room.sca.as_ref());
    }
}

/// `Game_loop` and `Em_init_move` start at `splayer_work`.
fn run_order() -> impl Iterator<Item = usize> {
    std::iter::once(SPLAYER_SLOT).chain(0..SPLAYER_SLOT)
}

/// `_Spl`'s routine: `Spl_Init` for one no script has given a routine, and
/// `Spl_Scenario`. `Spl_Auto` isn't ported: one of its own stands still.
fn spl(character: &mut Character, tick: &Tick<'_>) -> Option<u8> {
    if player2::uninitialised(&character.work) {
        player2::spl_init(
            &mut character.work,
            &mut character.body,
            tick,
            character.kind,
        );
        None
    } else if player2::in_scenario(&character.work) {
        player2::player_move(&mut character.work, &mut character.body, tick)
    } else {
        None
    }
}

/// `Em_init_move`'s `_Spl` calls for a character the room places.
fn em_init_move(character: &mut Character, tick: &Tick<'_>) -> Option<u8> {
    character.with_room = false;
    let arrived = spl(character, tick);
    let (start, flags) = character.start;
    if start == 0 {
        return arrived;
    }
    player2::spl_start(&mut character.work, start, flags);
    spl(character, tick).or(arrived)
}

/// The tick a character's `_Spl` sees: its own model's banks, and the room's
/// animations for its slot.
pub fn tick_of<'a>(
    room: &'a RoomState,
    game: &GameState,
    character: &Character,
    slot: usize,
    model: &'a Emd,
) -> Tick<'a> {
    let empty: (&[Clip], &[Keyframe]) = (&[], &[]);
    let room_anim = room
        .character_anims
        .iter()
        .find(|(anims_slot, _)| usize::from(*anims_slot) == slot)
        .map(|(_, anim)| anim);
    Tick {
        room,
        skeleton: &model.skeleton,
        banks: Banks {
            pld: (&model.clips, &model.keyframes),
            weapon: model
                .banks
                .first()
                .map_or(empty, |bank| (&bank.clips, &bank.keyframes)),
            room: room_anim.map_or(empty, |anim| (&anim.clips, &anim.keyframes)),
        },
        input: Default::default(),
        options: game.player_options,
        id: character.id,
        life: i32::from(i16::MAX),
        poisoned: false,
        stopped: false,
        weapon: Default::default(),
        necks: game.re2.necks(),
        targets: &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::RoomId;

    #[test]
    fn a_character_stops_when_out_of_play_or_told_not_to_move() {
        let mut character = Character {
            id: 0x45,
            be_flg: 1,
            work: player2::Work::default(),
            body: crate::player::spawn(RoomId::parse("1140").unwrap(), &RoomState::default()),
            kind: 0,
            start: (0, 0),
            with_room: false,
            zombie: None,
        };
        assert!(moves(&character));
        character.work.kind = player2::KIND_NOT_MOVED;
        assert!(!moves(&character));
        character.work.kind = 0;
        character.be_flg = 0;
        assert!(!moves(&character));
    }
}
