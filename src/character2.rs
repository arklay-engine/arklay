//! RE2's script characters: `_Spl`, which runs ids 0x40 to 0x5B in the enemy
//! slots. Only `Spl_Scenario` is here so far, the routine scripts steer with.

use crate::enemy::EntityModelCache;
use crate::game::GameState;
use crate::model::{Clip, Emd, Keyframe};
use crate::pack::Pack;
use crate::player2::{self, Banks, Tick};
use crate::scd2::host::Character;
use crate::state::RoomState;

/// `Game_loop`'s pass over the enemy slots, for the characters `_Spl` runs.
pub fn tick(
    game: &mut GameState,
    room: &RoomState,
    pack: &Pack,
    npc_models: &mut EntityModelCache,
) {
    init(game, room, pack, npc_models);
    if game.re2.characters_stopped() {
        return;
    }
    let scenario = game.id.player_flag & 1;
    for slot in 0..game.re2.characters.0.len() {
        let Some(mut character) = game.re2.characters.0[slot].take() else {
            continue;
        };
        let model = npc_models.get(pack, character.id, scenario);
        if let Some(model) = model.filter(|_| character.be_flg & 1 != 0) {
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
                let mask = if matches!(character.id, 0x45 | 0x4F) {
                    crate::sca::CHARACTER_MASK_LOW
                } else {
                    crate::sca::CHARACTER_MASK
                };
                let resolved = crate::sca::resolve_masked(sca, mask, prev, body.pos, body.radius);
                body.pos = resolved.pos;
                body.collision_mirror = [body.pos[0], body.pos[2]];
            }
        }
        game.re2.characters.0[slot] = Some(character);
    }
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
    for slot in 0..game.re2.characters.0.len() {
        let Some(mut character) = game.re2.characters.0[slot].take() else {
            continue;
        };
        if character.with_room
            && let Some(model) = npc_models.get(pack, character.id, scenario)
        {
            let tick = tick_of(room, game, &character, slot, &model);
            let arrived = em_init_move(&mut character, &tick);
            if let Some(flag) = arrived {
                game.re2.raise_room_flag(flag);
            }
        }
        game.re2.characters.0[slot] = Some(character);
    }
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
    }
}
