//! RE2's script characters: `_Spl`, which runs ids 0x40 to 0x5B in the enemy
//! slots. Only `Spl_Scenario` is here so far, the routine scripts steer with.

use crate::enemy::EntityModelCache;
use crate::game::GameState;
use crate::model::{Clip, Emd, Keyframe};
use crate::pack::Pack;
use crate::player2::{self, Banks, Tick};
use crate::scd2::host::{Arms, Character, EsprCall, NO_ENEMY_FLAG, SPLAYER_SLOT};
use crate::state::RoomState;

/// `Game_loop`'s pass over the enemy slots, for the characters `_Spl` runs.
pub fn tick(
    game: &mut GameState,
    room: &RoomState,
    pack: &Pack,
    npc_models: &mut EntityModelCache,
    mut lua: Option<&mut crate::enemy::LuaEnemyHost>,
) {
    init(game, room, pack, npc_models, lua.as_deref_mut());
    // A door's load ends behind its animation, and `fg_stop` stays up a tick more.
    if std::mem::take(&mut game.re2.door_stop) {
        run_load_stop(game, room, pack, npc_models, lua.as_deref_mut());
        return;
    }
    let scenario = game.id.player_flag & 1;
    for slot in run_order() {
        if slot == SPLAYER_SLOT {
            partner_root_ck_sys(game, room, false);
        }
        // `Game_loop` still calls a stopped enemy, which does its `Root_ck` first.
        let stopped = game.re2.character_stopped(slot);
        let zombie = game
            .re2
            .characters
            .0
            .get(slot)
            .is_some_and(|character| character.as_deref().is_some_and(|c| c.zombie.is_some()));
        if stopped && !zombie {
            continue;
        }
        let Some(mut character) = game.re2.characters.0.get_mut(slot).and_then(Option::take) else {
            continue;
        };
        if moves(&character) {
            let (dx, dz) = (
                character.body.pos[0] - game.entities[0].pos[0],
                character.body.pos[2] - game.entities[0].pos[2],
            );
            character.distance =
                crate::sca::square_root0(dz.wrapping_mul(dz).wrapping_add(dx.wrapping_mul(dx)))
                    as u32;
        }
        let model = npc_models.get(pack, character.model, scenario);
        if let Some(mut zombie) = character.zombie.take() {
            let mut sent = Vec::new();
            if let Some(model) = model.filter(|_| moves(&character)) {
                sent = run_zombie(
                    game,
                    room,
                    pack,
                    lua.as_deref_mut(),
                    &mut character,
                    &mut zombie,
                    slot,
                    &model,
                    false,
                );
            }
            character.zombie = Some(zombie);
            game.re2.characters.0[slot] = Some(character);
            run_spawned(game, room, pack, npc_models, lua.as_deref_mut(), sent);
            continue;
        }
        if let Some(model) = model.filter(|_| moves(&character)) {
            let held = character.held_bank.clone();
            let tick = tick_of(room, game, &character, slot, &model, held.as_deref());
            let old = character.body.pos;
            let arrived = spl(game, &mut character, &tick);
            if let Some(flag) = arrived {
                game.re2.raise_room_flag(flag);
            }
            // The ripples of her footfalls in water (`Snd_se_walk`).
            let calls = std::mem::take(&mut character.work.effects);
            esp_calls(game, calls);
            if character.partner.is_some() && game.re2.partner_bit() {
                partner_oba_ck_em(game, &mut character, old);
            }
            sce_blood(game, &mut character);
            // `Snd_se_walk` from the character's own walk. Their `Floor` isn't
            // modelled, so the zone is the one at the step's height.
            for step in character.body.take_footsteps() {
                let floor = crate::sca::floor_of(step.pos[1]);
                game.re2.sounds.push(crate::scd2::host::SoundRequest::new(
                    crate::se2::footstep_sound(&room.footstep_zones, &step, floor, false),
                    step.pos,
                ));
            }
            spl_sca_ck_em(room, &mut character);
        }
        game.re2.characters.0[slot] = Some(character);
    }
}

/// The player's `Atd[0]` at `pos`, as `Oba_set_ofs` leaves it.
fn player_atd(game: &GameState, pos: [i32; 3]) -> crate::zombie2::Atd {
    let width = player_width(game.id.player_flag);
    crate::zombie2::Atd {
        ofs_x: 0,
        ofs_y: -(PLAYER_HEIGHT as i16),
        at_w: width,
        at_h: PLAYER_HEIGHT,
        w: width,
        at: [pos[0], pos[1] - i32::from(PLAYER_HEIGHT), pos[2]],
    }
}

/// A character's `Atd[0]`: a zombie's own, as its `Oba_set_ofs` and pushes
/// left it, else a box of its height and radius on its position.
pub(crate) fn body_atd(character: &Character) -> crate::zombie2::Atd {
    if let Some(zombie) = character.zombie.as_ref() {
        return zombie.atd;
    }
    let height = player2::character_height(character.id);
    let pos = character.body.pos;
    crate::zombie2::Atd {
        ofs_x: 0,
        ofs_y: -(height as i16),
        at_w: character.body.radius as u16,
        at_h: height,
        w: character.body.radius as u16,
        at: [pos[0], pos[1] - i32::from(height), pos[2]],
    }
}

/// `Game_loop`'s draw, for `Om_move`'s next tick: the player and each
/// character in play, stopped or not, keep their box as `Old_x2`, `Old_y2`
/// and `Old_z2`.
pub fn keep_old_boxes(game: &mut GameState, player_pos: [i32; 3]) {
    game.re2.player.old_box = Some(player_atd(game, player_pos).at);
    for character in game.re2.characters.0.iter_mut().flatten() {
        if character.be_flg & 1 != 0 {
            character.work.old_box = Some(body_atd(character).at);
        }
    }
}

/// What the slots in play show `Oba_ck_hit`, in `player_work`'s order.
fn slot_bodies(game: &GameState) -> Vec<crate::zombie2::Oba> {
    let in_play = game.re2.characters.0.iter().enumerate();
    in_play
        .filter_map(|(slot, other)| Some((slot, other.as_deref()?)))
        .filter(|(_, other)| other.be_flg & 1 != 0)
        .flat_map(|(slot, other)| {
            let atd = body_atd(other);
            // A licker has a second box for its head and, on a wall or
            // ceiling, a third for its back.
            let licker = other.zombie.as_ref().and_then(|z| z.licker.as_ref());
            let head = licker.map(|licker| licker.head);
            let back = licker.and_then(|licker| licker.back);
            let more = other
                .zombie
                .as_ref()
                .map(|z| z.more_atd.clone())
                .unwrap_or_default();
            std::iter::once(atd)
                .chain(head)
                .chain(back)
                .chain(more)
                .map(move |atd| crate::zombie2::Oba {
                    slot: slot as u8,
                    be_flg: other.be_flg,
                    atd,
                    pos: other.body.pos,
                })
        })
        .collect()
}

/// `Oba_ck_em2`: the player's push out of the bodies in play, after `Player_move`.
///
/// `old` is where the player ended the last tick: `Old_x`, `Old_y` and `Old_z`.
pub fn oba_ck_em2(game: &mut GameState, player: &mut crate::player::PlayerState, old: [i32; 3]) {
    let mut atd = player_atd(game, player.pos);
    let be_flg = game.re2.player.be_flg;
    let mut touching = 0u32;
    let mut partner = false;
    for other in slot_bodies(game) {
        if crate::zombie2::oba_hit(&other, be_flg, &mut atd, &mut player.pos, Some(old)) {
            if usize::from(other.slot) == SPLAYER_SLOT {
                partner = true;
            } else {
                touching |= 1u32.checked_shl(u32::from(other.slot)).unwrap_or(0);
            }
        }
    }
    game.re2.player.at_em_flg = touching;
    game.re2.player.at_em_partner = partner;
    game.re2.player.atd = Some(atd);
    game.re2.player.old = Some(old);
}

/// `Em10` throws a zombie back (`Routine` `0x901`), if it can be.
fn knock(game: &mut GameState, slot: usize, back: bool) -> bool {
    let Some(Some(other)) = game.re2.characters.0.get_mut(slot) else {
        return false;
    };
    let Some(zombie) = other.zombie.as_mut() else {
        return false;
    };
    if other.id & 0x10 != 0x10 || !zombie.can_be_knocked(other.work.kind) {
        return false;
    }
    other.work.routine = [1, 9, 0, 0];
    zombie.knocked(back);
    true
}

/// `Player_move` while an enemy holds the player. `FUN_0045a880`'s shove throws
/// back the slots the player touched.
pub fn player_held(
    game: &mut GameState,
    player: &mut crate::player::PlayerState,
    tick: &player2::Tick<'_>,
) {
    let slot = game.re2.player.held_by.unwrap_or(usize::MAX);
    let holder = game
        .re2
        .characters
        .0
        .get(slot)
        .and_then(|character| character.as_deref())
        .map_or(player.pos, |character| character.body.pos);
    let holder_id = game
        .re2
        .characters
        .0
        .get(slot)
        .and_then(|character| character.as_deref())
        .map_or(0, |character| character.id);
    let holder_angle = game
        .re2
        .characters
        .0
        .get(slot)
        .and_then(|character| character.as_deref())
        .map_or(0, |character| character.body.angle);
    let holder_aim = game
        .re2
        .characters
        .0
        .get(slot)
        .and_then(|character| character.as_deref())
        .map_or(0, crate::zombie2::plant_aim);
    let holder_move_cnt = game
        .re2
        .characters
        .0
        .get(slot)
        .and_then(|character| character.as_deref())
        .map_or(0, |character| character.work.move_cnt);
    let holder_grip = game
        .re2
        .characters
        .0
        .get(slot)
        .and_then(|character| character.as_deref())
        .filter(|character| character.id == crate::zombie2::G3_ID)
        .and_then(|character| character.zombie.as_ref())
        .and_then(|zombie| zombie.g3_grip());
    let re2 = &mut game.re2;
    let mut held = player2::Held {
        holder,
        holder_angle,
        holder_aim,
        holder_id,
        holder_move_cnt,
        holder_grip,
        draw_buffer: re2.draw_buffer,
        rnd: &mut re2.rnd,
        life: &mut game.entities[0].health,
        released: false,
        knocks: 0,
    };
    player2::player_held(&mut re2.player, player, tick, &mut held);
    let (released, knocks) = (held.released, held.knocks);
    if released && let Some(Some(character)) = game.re2.characters.0.get_mut(slot) {
        character.be_flg &= !0x1004;
    }
    for other in 0..game.re2.characters.0.len().min(32) {
        if knocks & (1 << other) == 0 {
            continue;
        }
        // `~Cdir_ck2`: the shove comes from in front of it, or behind.
        let front = game.re2.characters.0[other]
            .as_deref()
            .is_some_and(|other| {
                let to = player2::direction_ck(
                    player.pos[0],
                    player.pos[2],
                    other.body.pos[0],
                    other.body.pos[2],
                );
                other.body.angle.wrapping_sub(to).wrapping_add(0x400) & 0xFFF < 0x800
            });
        knock(game, other, !front);
    }
}

/// `Player_move`'s last call: the dropped magazine falls, and sounds as it bounces.
pub fn mag_down(game: &mut GameState, player: &mut crate::player::PlayerState) {
    let re2 = &mut game.re2;
    if let Some(at) = re2.player.mag_down(&mut re2.rnd) {
        player.queue_sound_at(MAGAZINE_SOUND, at);
    }
}

/// `Snd_se_on(0x10e0001)`.
const MAGAZINE_SOUND: u16 = 0xE;

/// `pl_bow`, `Player_move`'s call after `Rot_neck` while the bowgun is held:
/// its bolts fly, and what they hit takes it at once. Any other weapon puts
/// the bolts away, for `Plw_init_tbl` to set up again.
pub fn pl_bow(game: &mut GameState, sca: Option<&crate::state::Sca>) {
    if game.equipped_weapon_id != crate::bow2::BOW {
        game.re2.player.quiver = None;
        return;
    }
    let water = water_areas(game);
    let re2 = &mut game.re2;
    let mut bodies: Vec<_> = targets_of(re2)
        .into_iter()
        .map(|target| {
            let zombie = re2.characters.0[target.slot]
                .as_deref()
                .and_then(|character| character.zombie.as_ref());
            let parts = crate::bow2::aim_parts(target.id).map(|part| {
                let matrix = zombie.and_then(|zombie| zombie.drawn.get(usize::from(part)));
                matrix.copied().flatten().map(|matrix| (part, matrix))
            });
            crate::bow2::Body { target, parts }
        })
        .collect();
    let quiver = re2.player.quiver.get_or_insert_with(Default::default);
    let mut world = crate::bow2::World {
        bodies: &mut bodies,
        sca,
        water: &water,
        rnd: &mut re2.rnd,
        calls: Vec::new(),
    };
    crate::bow2::pl_bow(quiver, &mut world);
    for call in world.calls {
        match call {
            crate::bow2::Call::Effect(call) => {
                re2.effects
                    .call(call.kind, call.row, call.size, call.dir, call.at, false);
            }
            crate::bow2::Call::Sound(id, pos) => {
                if let Some(id) = crate::se2::SoundId::decode(id) {
                    re2.sounds
                        .push(crate::scd2::host::SoundRequest::new(id, pos));
                }
            }
            crate::bow2::Call::Blast(blast) => {
                let slot = match blast {
                    crate::weapon2::BlastEffect::Tested(slot) => slot,
                    crate::weapon2::BlastEffect::Hit(hit) => hit.slot,
                };
                if let Some(character) = re2.characters.0.get_mut(slot).and_then(Option::as_mut)
                    && let Some(mut zombie) = character.zombie.take()
                {
                    crate::zombie2::take_blast(character, &mut zombie, &blast);
                    character.zombie = Some(zombie);
                }
            }
            crate::bow2::Call::Stuck { slot, part, yaw } => {
                if let Some(zombie) = re2
                    .characters
                    .0
                    .get_mut(slot)
                    .and_then(Option::as_mut)
                    .and_then(|character| character.zombie.as_mut())
                {
                    zombie.bolt_part = part;
                    zombie.bolt_yaw = yaw;
                }
            }
        }
    }
}

/// What the player's routine sees of the slots, in `enemy_work`'s order.
pub fn targets(game: &GameState) -> Vec<crate::weapon2::Target> {
    targets_of(&game.re2)
}

/// [`targets`], from the RE 2 state alone.
pub fn targets_of(re2: &crate::scd2::host::Re2State) -> Vec<crate::weapon2::Target> {
    let characters = re2.characters.0.iter().enumerate();
    characters
        .filter(|(slot, _)| *slot != SPLAYER_SLOT)
        .filter_map(|(slot, character)| {
            let character = character.as_deref()?;
            // `Em_2D`'s hits (`Routine_0` 2 and 3) aren't ported yet.
            if character.id == crate::zombie2::EM2D_ID {
                return None;
            }
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
                at_w1: if character.id == crate::zombie2::DOG_ID {
                    600
                } else if character.id == crate::zombie2::ALLIGATOR_ID {
                    // `FUN_00468a10`.
                    0x44C
                } else if matches!(
                    character.id,
                    crate::zombie2::CROW_ID
                        | crate::zombie2::MRX_ID
                        | crate::zombie2::SPIDER_ID
                        | crate::zombie2::YOUNG_ID
                        | crate::zombie2::ROACH_ID
                        | crate::zombie2::EMBRYO_ID
                ) {
                    // `Em_21`, `MrX_init`, `Em_25`'s, `Em_26`'s, `Em_27`'s and
                    // `Em_29`'s inits never give it an `Atd[1]`.
                    0
                } else {
                    zombie
                        .licker
                        .as_ref()
                        .map_or(500, |licker| licker.head.at_w)
                },
                neck_y: character.work.neck.position.map_or(pos[1], |neck| neck[1]),
                ofs_y: zombie.atd.ofs_y,
                at_h: zombie.atd.at_h,
                angle: character.body.angle,
            })
        })
        .collect()
}

/// `pl_mv05_w1_t2`'s call of `Shoot_camera`: with few enemies about, a gun
/// fired along the camera's view throws sparks at the lens.
pub fn shoot_camera(game: &mut GameState, cut: Option<&crate::state::Cut>) {
    let Some(line) = game.re2.player.shot_line.take() else {
        return;
    };
    let Some(cut) = cut else {
        return;
    };
    let weapon = game.re2_weapon();
    if usize::from(game.re2.enemies) >= crate::weapon2::SHOOT_CAMERA_ENEMIES {
        return;
    }
    let at = crate::weapon2::shoot_camera(weapon.id, line, cut.pos, cut.look_at, &mut game.re2.rnd);
    for at in at {
        game.re2.effects.call(7, 0, 0x2C8, 0, at, false);
    }
}

/// A move's `Esp_call`s, in the order it made them.
fn esp_calls(game: &mut GameState, calls: Vec<player2::EffectCall>) {
    for call in calls {
        let effects = &mut game.re2.effects;
        let index = match call.on {
            Some((part, matrix, point)) => {
                let anchor = crate::esp2::Anchor {
                    body: crate::esp2::Body::Player,
                    part,
                };
                effects.call_on_part(
                    call.kind, call.row, call.size, call.dir, anchor, matrix, point,
                )
            }
            None => effects.call(call.kind, call.row, call.size, call.dir, call.at, false),
        };
        if let (Some(index), Some(motion)) = (index, call.motion) {
            game.re2.effects.launch(index, motion);
        }
    }
}

/// What the player's tick asked of the effects and the slots: the shot's
/// `Esp_call`s, and what it found goes to who it hit.
pub fn take_hit(game: &mut GameState) {
    let pos = game.entities[0].pos;
    for sound in std::mem::take(&mut game.re2.player.sounds) {
        if let Some(id) = crate::se2::SoundId::decode(sound) {
            game.re2
                .sounds
                .push(crate::scd2::host::SoundRequest::new(id, pos));
        }
    }
    let calls = std::mem::take(&mut game.re2.player.effects);
    esp_calls(game, calls);
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

/// Where each room object's twinkle sits on it now (`Sce_kirakira_set`'s
/// point on the object's matrix), for the twinkles that follow their object.
pub(crate) fn twinkle_points(game: &GameState) -> Vec<Option<[i16; 3]>> {
    game.objects
        .records
        .iter()
        .map(|record| crate::objects::twinkle(record).map(|(_, at)| at))
        .collect()
}

/// The water areas (`aot_set` kind 7) in `aot_table`'s order, for `Water_ck`.
pub(crate) fn water_areas(game: &GameState) -> Vec<crate::zombie2::WaterArea> {
    game.room_actions
        .iter()
        .enumerate()
        .filter_map(|(slot, action)| {
            let action = action.as_ref()?;
            (action.sce == crate::scd2::host::SCE_WATER).then(|| crate::zombie2::WaterArea {
                zone: action.zone,
                quad: game.re2.gates.get(slot).and_then(|gate| gate.quad),
                level: i16::from_le_bytes([action.params[0], action.params[1]]),
            })
        })
        .collect()
}

/// Where `player_work[index]` stands, for a script's `Sce_free3`.
fn body_of(game: &GameState, index: i16) -> Option<[i32; 3]> {
    let slot = match index {
        0 => return Some(game.entities[0].pos),
        1 => SPLAYER_SLOT,
        _ => usize::try_from(index - 2).ok()?,
    };
    let character = game.re2.characters.0.get(slot)?.as_deref()?;
    Some(character.body.pos)
}

/// The end of `FUN_00459ef0`'s grab: the body named goes to `FUN_004f6800` on
/// the zombie's own clips, from the zombie's base.
fn grab(game: &mut GameState, index: i16, base: [i16; 3], model: &Emd) {
    let slot = match index {
        1 => SPLAYER_SLOT,
        _ => match usize::try_from(index - 2) {
            Ok(slot) => slot,
            Err(_) => return,
        },
    };
    let (Some(Some(character)), Some(bank)) =
        (game.re2.characters.0.get_mut(slot), model.banks.get(1))
    else {
        return;
    };
    character.work.base = base;
    character.work.routine = [player2::MODE_SCENARIO, 0x20, 0, 0];
    character.be_flg |= crate::zombie2::BE_HOLDING;
    character.held_bank = Some(std::sync::Arc::new(crate::game::PlayerDamageBank {
        keyframes: std::sync::Arc::new(bank.keyframes.clone()),
        clips: std::sync::Arc::new(bank.clips.clone()),
    }));
}

/// `Sce_espr_on`'s `Esp_call`s on someone's matrix: what the scripts just asked.
pub fn run_esprs(
    game: &mut GameState,
    room: &RoomState,
    pack: &Pack,
    npc_models: &mut EntityModelCache,
) {
    let scenario = game.id.player_flag & 1;
    for (call, matrix) in std::mem::take(&mut game.re2.pending_esprs) {
        let world = match matrix.slot {
            None => None,
            Some(slot) => game
                .re2
                .characters
                .0
                .get_mut(slot)
                .and_then(Option::take)
                .map(|mut character| {
                    let model = npc_models.get(pack, character.model, scenario);
                    let world = model.zip(matrix.part).and_then(|(model, part)| {
                        let held = character.held_bank.clone();
                        let tick = tick_of(room, game, &character, slot, &model, held.as_deref());
                        let character = &mut *character;
                        player2::part_world(&mut character.work, &mut character.body, &tick, part)
                    });
                    let root = (character.body.pos, character.body.angle);
                    game.re2.characters.0[slot] = Some(character);
                    world.unwrap_or_else(|| root_matrix(root.0, root.1))
                }),
        };
        let world = world.unwrap_or_else(|| {
            let player = &game.entities[0];
            root_matrix(player.pos, player.angle)
        });
        let at = call.at.map(i32::from);
        let at = std::array::from_fn(|axis| {
            let turned: i32 = (0..3).map(|i| i32::from(world.m[axis][i]) * at[i]).sum();
            (world.t[axis] + (turned >> 12)) as i16
        });
        game.re2.effects.call_placed(EsprCall { at, ..call });
    }
}

fn root_matrix(pos: [i32; 3], angle: u16) -> player2::Matrix {
    player2::Matrix {
        m: player2::rot_matrix([0, angle as i16, 0]),
        t: pos,
    }
}

/// `FUN_004f6800`'s `Esp_call`s, on the `Rnd` it had no hold of.
fn sce_blood(game: &mut GameState, character: &mut Character) {
    let Some((spray, at)) = character.work.sce_blood.take() else {
        return;
    };
    let re2 = &mut game.re2;
    let dir = i16::from(re2.rnd.draw()) << 4;
    if spray {
        re2.effects.call(0, 0, 0x1000, dir, at, false);
        let dir = i16::from(re2.rnd.draw()) << 4;
        re2.effects.call(0, 0, 0x1400, dir, at, false);
    } else {
        let size = u16::from(re2.rnd.draw()) * 8 + 0x400;
        re2.effects.call(0, 0, size, dir, at, false);
    }
}

/// `Game_loop`'s call of `Em_Zombie`, with its view of the rest of the game.
/// It gives back the slots `Set_enemy_work` gave out whose routine runs at
/// once (`Spawn::run_now`).
#[allow(clippy::too_many_arguments)]
#[must_use]
fn run_zombie(
    game: &mut GameState,
    room: &RoomState,
    pack: &Pack,
    lua: Option<&mut crate::enemy::LuaEnemyHost>,
    character: &mut Character,
    zombie: &mut crate::zombie2::Zombie,
    slot: usize,
    model: &Emd,
    loading: bool,
) -> Vec<usize> {
    let tick = tick_of(room, game, character, slot, model, None);
    zombie.parts_num = parts_num(model);
    let player = game.entities[0].pos;
    let scene_target = body_of(game, character.work.dest[1]).unwrap_or(character.body.pos);
    let mut bodies = vec![crate::zombie2::Oba {
        slot: crate::zombie2::PLAYER_BODY,
        be_flg: game.re2.player.be_flg,
        atd: game
            .re2
            .player
            .atd
            .unwrap_or_else(|| player_atd(game, player)),
        pos: player,
    }];
    bodies.extend(slot_bodies(game));
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
    let root_target = zombie_root_target(game, room, character);
    let stopped = game.re2.character_stopped(slot);
    let easy = game.re2.flag(0, FG_SYSTEM_EASY);
    let difficulty = game.re2.difficulty();
    let enemies = game.re2.enemies;
    let noise = game.re2.player.noise;
    let player_floor = game.re2.player.floor;
    let player_neck = game.re2.player.neck.position;
    let stage = game.id.stage;
    let room_code = u16::from(stage) << 8 | u16::from(game.id.room);
    let player_angle = game.entities[0].angle;
    let player_id = game.id.player_flag;
    let weapon = game.re2_weapon().id;
    let common_a4 = game.re2.flag(FG_COMMON, FG_COMMON_LICKER);
    let claire = game.re2.flag(
        crate::scd2::host::STATUS_BANK,
        crate::scd2::host::STATUS_PLAYER,
    );
    let water = if matches!(
        character.id,
        crate::zombie2::SPIDER_ID | crate::zombie2::YOUNG_ID
    ) {
        water_areas(game)
    } else {
        Vec::new()
    };
    let alligator = character.id == crate::zombie2::ALLIGATOR_ID;
    let camera_cut = game.camera.current_cut as u8;
    let objects = if alligator {
        game.objects
            .records
            .iter()
            .map(|object| object.pos)
            .collect()
    } else {
        Vec::new()
    };
    let re2 = &mut game.re2;
    let banks = if alligator {
        re2.banks.to_vec()
    } else {
        Vec::new()
    };
    let gacha = re2.key_trg & 0x34F != 0;
    let same_id = re2
        .characters
        .0
        .iter()
        .flatten()
        .filter(|other| other.be_flg & 1 != 0 && other.id == character.id)
        .count() as u8;
    let parent = zombie
        .parent
        .and_then(|parent| re2.characters.0.get(usize::from(parent))?.as_deref())
        .and_then(|parent| {
            let z = parent.zombie.as_ref()?;
            Some(crate::zombie2::ParentView {
                vars: z.vars(),
                pos: parent.body.pos,
                angle: parent.body.angle,
                life: z.life,
                kind: parent.work.kind,
                routine: parent.work.routine,
                parts: z.parts.clone(),
                rotations: z.rotations.clone(),
                be_flg: parent.be_flg,
            })
        });
    let works = (0..SPLAYER_SLOT)
        .map(|other| {
            if other == slot {
                return (other, Some(character.id), character.be_flg & 1 != 0);
            }
            match re2.characters.0.get(other).and_then(Option::as_deref) {
                Some(other_character) => (
                    other,
                    Some(other_character.id),
                    other_character.be_flg & 1 != 0,
                ),
                None => (other, None, false),
            }
        })
        .collect();
    let effects = &mut re2.effects;
    let player_life = &mut game.entities[0].health;
    let mut world = crate::zombie2::World {
        slot,
        same_id,
        room_enemy: &mut re2.room_enemy,
        turn_player: None,
        pl: &mut re2.player,
        player_angle,
        player_life,
        gacha,
        player_id,
        knocks: Vec::new(),
        player,
        player_neck,
        player_floor,
        noise: u16::from(noise),
        rnd: &mut re2.rnd,
        partner,
        easy,
        difficulty,
        enemies,
        stopped,
        bodies: &bodies,
        guides: &room.guides,
        sca,
        effects,
        weapon,
        claire,
        kin: Vec::new(),
        tongue_owner: &mut re2.tongue_owner,
        em2d_groan: &mut re2.em2d_groan,
        swarm: &mut re2.swarm,
        common_a4,
        draw_buffer: re2.draw_buffer,
        room_code,
        player_moved: false,
        status_or: 0,
        scene_target,
        pack,
        lua,
        parent,
        works,
        spawns: Vec::new(),
        killed: false,
        water,
        banks,
        flag_ops: Vec::new(),
        status_and_not: 0,
        cut: None,
        objects,
        sub1: false,
        camera_cut,
        hidden_objects: Vec::new(),
        root_target,
        l_spl: character.l_spl,
        dir_spl: character.dir_spl,
    };
    crate::zombie2::em_zombie(character, zombie, &tick, &mut world);
    let spawns = std::mem::take(&mut world.spawns);
    let killed = world.killed;
    let moved = world.player_moved.then_some(world.player);
    let status_or = world.status_or;
    let status_and_not = world.status_and_not;
    let flag_ops = std::mem::take(&mut world.flag_ops);
    let cut = world.cut;
    let sub1 = world.sub1;
    let objects = std::mem::take(&mut world.objects);
    let hidden_objects = std::mem::take(&mut world.hidden_objects);
    let knocks = std::mem::take(&mut world.knocks);
    let kin = std::mem::take(&mut world.kin);
    if let Some(angle) = world.turn_player {
        re2.player.turn_to = Some(angle);
    }
    if let Some([x, _, z]) = moved {
        game.entities[0].pos[0] = x;
        game.entities[0].pos[2] = z;
        re2.player.move_to = Some([x, z]);
    }
    for bit in (0..32u8).filter(|bit| status_or & (0x8000_0000u32 >> bit) != 0) {
        re2.set_flag(crate::scd2::host::STATUS_BANK, bit, 1);
    }
    for bit in (0..32u8).filter(|bit| status_and_not & (0x8000_0000u32 >> bit) != 0) {
        re2.set_flag(crate::scd2::host::STATUS_BANK, bit, 0);
    }
    for (bank, index, op) in flag_ops {
        re2.set_flag(bank, index, op);
    }
    // The alligator moves the gas tank it bites (`FUN_004697f0`).
    for (record, pos) in game.objects.records.iter_mut().zip(objects) {
        record.pos = pos;
    }
    for index in hidden_objects {
        if let Some(record) = game.objects.record_mut(index) {
            record.flag &= !crate::objects::OBJECT_FLAG_ACTIVE;
        }
    }
    if let Some(cut) = cut {
        // `Cut_change`: the cut changes, and the zones carry on from it.
        game.set_camera_cut(usize::from(cut & 0x7F));
    }
    // `fg_status`'s `0x100` is the camera's lock, which `Cut_check` obeys.
    if status_or & 0x100 != 0 {
        game.camera.locked = true;
    } else if status_and_not & 0x100 != 0 {
        game.camera.locked = false;
    }
    let re2 = &mut game.re2;
    for call in &kin {
        for other in re2.characters.0.iter_mut().flatten() {
            if other.id == character.id
                && let Some(other) = other.zombie.as_mut()
            {
                other.hear(call);
            }
        }
    }
    if killed {
        re2.enemies = re2.enemies.wrapping_sub(1);
    }
    let mut run_now = Vec::new();
    for spawn in spawns {
        if spawn.run_now {
            run_now.push(spawn.slot);
        }
        set_enemy_work(re2, character, slot, spawn, loading);
    }
    let pos = character.body.pos;
    if player2::is_held(&re2.player)
        && re2.player.held_by == Some(slot)
        && re2.player.routine[2] == 0
        && let Some(bank) = model.banks.get(1)
    {
        game.player_damage = Some(std::sync::Arc::new(crate::game::PlayerDamageBank {
            keyframes: std::sync::Arc::new(bank.keyframes.clone()),
            clips: std::sync::Arc::new(bank.clips.clone()),
        }));
    }
    if sub1 && let Some(bank) = model.banks.get(1) {
        game.player_damage = Some(std::sync::Arc::new(crate::game::PlayerDamageBank {
            keyframes: std::sync::Arc::new(bank.keyframes.clone()),
            clips: std::sync::Arc::new(bank.clips.clone()),
        }));
    }
    if let Some(base) = zombie.grabbed.take() {
        grab(game, character.work.dest[1], base, model);
    }
    let re2 = &mut game.re2;
    if std::mem::take(&mut zombie.died) && zombie.dead_flag != NO_ENEMY_FLAG {
        let bank = re2.enemy_flag_bank(stage);
        re2.set_flag(bank, zombie.dead_flag, 1);
    }
    // `Snd_se_enem`, always in 3D.
    let second = if character.be_flg & 0x2000 != 0 {
        0x10
    } else {
        0
    };
    let enemy_sound =
        |slot: u8| crate::se2::SoundId::decode(3 << 24 | u32::from(slot + second) << 16 | 1);
    // Each pans from where the body was at its call, before the tick's pushes.
    for (sound, pos) in zombie.sounds.drain(..) {
        if let Some(id) = enemy_sound(sound) {
            re2.sounds
                .push(crate::scd2::host::SoundRequest::new(id, pos));
        }
    }
    let mine = character.body.angle;
    for other in knocks.into_iter().map(usize::from) {
        // `Cdir_ck`.
        let behind = game.re2.characters.0.get(other).is_some_and(|target| {
            target.as_deref().is_some_and(|target| {
                target.body.angle.wrapping_sub(mine).wrapping_add(0x400) & 0xFFF < 0x800
            })
        });
        if knock(game, other, behind)
            && let Some(id) = enemy_sound(4)
        {
            game.re2
                .sounds
                .push(crate::scd2::host::SoundRequest::new(id, pos));
        }
    }
    run_now
}

/// The routine's run on each work `Set_enemy_work` gave out that runs at once,
/// in the order they were given.
fn run_spawned(
    game: &mut GameState,
    room: &RoomState,
    pack: &Pack,
    npc_models: &mut EntityModelCache,
    mut lua: Option<&mut crate::enemy::LuaEnemyHost>,
    slots: Vec<usize>,
) {
    let scenario = game.id.player_flag & 1;
    for slot in slots {
        let Some(mut character) = game.re2.characters.0.get_mut(slot).and_then(Option::take) else {
            continue;
        };
        if let Some(mut zombie) = character.zombie.take() {
            if let Some(model) = npc_models.get(pack, character.model, scenario) {
                // It sends nothing on itself.
                let _ = run_zombie(
                    game,
                    room,
                    pack,
                    lua.as_deref_mut(),
                    &mut character,
                    &mut zombie,
                    slot,
                    &model,
                    false,
                );
            }
            character.zombie = Some(zombie);
        }
        game.re2.characters.0[slot] = Some(character);
    }
}

/// What `Set_enemy_work` leaves in the work it gave `spawn`: a dead one of the
/// id back in play, or a copy of the last of the id. `parent` is the script's
/// own work, which it runs from (`pEnemy_ptr`).
fn set_enemy_work(
    re2: &mut crate::scd2::host::Re2State,
    parent: &Character,
    parent_slot: usize,
    spawn: crate::zombie2::Spawn,
    loading: bool,
) {
    re2.enemies = re2.enemies.wrapping_add(1);
    let characters = &mut re2.characters.0;
    characters.resize_with(crate::scd2::host::CHARACTER_SLOTS, || None);
    let template = (0..SPLAYER_SLOT)
        .rev()
        .find_map(|other| {
            if other == parent_slot {
                return (parent.id == spawn.id).then_some(parent);
            }
            characters[other]
                .as_deref()
                .filter(|other| other.id == spawn.id)
        })
        .cloned();
    let Some(template) = template else {
        return;
    };
    let mut character = if spawn.reused {
        match characters[spawn.slot].take() {
            Some(mut character) => {
                character.be_flg |= 1;
                character.work.routine = [0; 4];
                *character
            }
            None => return,
        }
    } else {
        let mut character = template;
        let mut work = player2::Work::default();
        work.routine = [0; 4];
        work.kind = character.work.kind;
        work.floor = character.work.floor;
        character.work = work;
        character.body.angle = 0;
        character.be_flg = 1;
        let floor = character.zombie.as_ref().map_or(0, |z| z.floor);
        let model_type = character.zombie.as_ref().map_or(0, |z| z.model_type);
        let water = character
            .zombie
            .as_ref()
            .map_or(0, |z| z.vars()[crate::zombie2::YOUNG_WATER]);
        let mut zombie = crate::zombie2::spawned(spawn.slot as u8, floor, model_type);
        if spawn.id == crate::zombie2::YOUNG_ID {
            // The copy keeps the template's `Water`.
            zombie.set_var(crate::zombie2::YOUNG_WATER, water);
        }
        zombie.set_sight_turn(re2.enemies & 7);
        character.zombie = Some(zombie);
        character
    };
    character.with_room = loading;
    character.work.kind = spawn.kind;
    character.kind = spawn.kind;
    if let Some((pos, angle)) = spawn.place {
        character.body.pos = pos;
        character.body.angle = angle;
        character.body.collision_mirror = [pos[0], pos[2]];
    }
    if let Some(zombie) = character.zombie.as_mut() {
        zombie.parent = Some(parent_slot as u8);
        for (index, value) in spawn.vars {
            zombie.set_var(index, value);
        }
    }
    characters[spawn.slot] = Some(Box::new(character));
}

/// The player's `Atd[0]` width, `At_w` and `W`: `Player_set`'s 450, which
/// `Pl_init` makes 350 for Claire's bodies and Sherry, as it does the radius
/// the room's collision pushes with.
pub fn player_width(player_flag: u8) -> u16 {
    crate::sca::player_radius(player_flag) as u16
}
/// The player's `At_h`, and how far its box's `Ofs_y` lifts it.
pub const PLAYER_HEIGHT: u16 = 0x5FA;
/// `fg_system`'s `0x20`, easy mode's bit.
const FG_SYSTEM_EASY: u8 = 26;
/// `fg_common`'s `0xa4`, which `Em22_Init` reads.
const FG_COMMON: u8 = 4;
const FG_COMMON_LICKER: u8 = 0xA4;

/// `Sca_ck_em`'s mask: the children's is lower, and a zombie's its own.
pub fn sca_mask(id: u8) -> u16 {
    if (0x10..=0x22).contains(&id) || id == 0x24 {
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
    mut lua: Option<&mut crate::enemy::LuaEnemyHost>,
) {
    let scenario = game.id.player_flag & 1;
    for slot in run_order() {
        let Some(mut character) = game.re2.characters.0.get_mut(slot).and_then(Option::take) else {
            continue;
        };
        if character.with_room
            && let Some(model) = npc_models.get(pack, character.model, scenario)
        {
            if let Some(mut zombie) = character.zombie.take() {
                character.with_room = false;
                let held = character.work.kind & player2::KIND_NOT_MOVED;
                character.work.kind &= !player2::KIND_NOT_MOVED;
                // No shot lands in the load, so it sends nothing.
                let _ = run_zombie(
                    game,
                    room,
                    pack,
                    lua.as_deref_mut(),
                    &mut character,
                    &mut zombie,
                    slot,
                    &model,
                    true,
                );
                let (start, flags) = character.start;
                if start != 0 {
                    player2::spl_start(&mut character.work, start, flags);
                    let _ = run_zombie(
                        game,
                        room,
                        pack,
                        lua.as_deref_mut(),
                        &mut character,
                        &mut zombie,
                        slot,
                        &model,
                        true,
                    );
                }
                character.work.kind |= held;
                character.zombie = Some(zombie);
                game.re2.characters.0[slot] = Some(character);
                continue;
            }
            let tick = tick_of(room, game, &character, slot, &model, None);
            let arrived = em_init_move(game, room, &mut character, &tick);
            if let Some(flag) = arrived {
                game.re2.raise_room_flag(flag);
            }
        }
        game.re2.characters.0[slot] = Some(character);
    }
    // The pass of `Game_loop` that ends a load runs `pTbefore_func` and `Esp_move`,
    // and little else.
    if std::mem::take(&mut game.re2.load_pass) {
        game.re2.tick_before();
        game.re2.effects.water = water_areas(game);
        game.re2.effects.objects = twinkle_points(game);
        game.re2.effects.weapon = game.equipped_weapon_id;
        let player = game
            .entities
            .first_mut()
            .map(|player| (player.pos, &mut player.health));
        game.re2.tick_effects(room.sca.as_ref(), player);
        run_load_stop(game, room, pack, npc_models, lua);
    }
}

/// A zombie's first run is a script, and a frame has no VM to run it with.
pub fn awaits_script(character: &Character) -> bool {
    character.with_room && character.zombie.is_some()
}

/// [`init`] for a frame drawn before its tick. With a zombie still to start, it
/// leaves the whole pass to the tick, so that it runs in its one order.
pub fn init_for_draw(
    game: &mut GameState,
    room: &RoomState,
    pack: &Pack,
    npc_models: &mut EntityModelCache,
) {
    let characters = &game.re2.characters.0;
    if !characters.iter().flatten().any(|c| awaits_script(c)) {
        init(game, room, pack, npc_models, None);
    }
}

/// Marvin's id: his init (`0x4f00e0`) gives his arms two sets of meshes.
const MARVIN: u8 = 0x4A;

/// The part pairs Marvin's init swaps (`0x53cb20`): each arm joint and the
/// spare part that holds its other mesh.
const MARVIN_ARMS: [(usize, usize); 7] = [
    (8, 17),
    (12, 18),
    (13, 15),
    (14, 19),
    (9, 20),
    (10, 21),
    (11, 22),
];

/// The mesh Marvin's right hand takes with `Type`'s `0x800`: the one with the
/// gun.
const MARVIN_GUN_HAND: usize = 23;

/// The ticks `Spl_chenge00` takes: the first part's `Poly_rgb` climbs a step a
/// tick to `0x40`, where the meshes swap, and takes its first step back in the
/// same call.
const MARVIN_FADE_TICKS: u8 = 0x40 * 2 - 1;

/// `Type`: a script reads and writes the work's, `sce_em_set` gave the rest.
fn marvin_kind(character: &Character) -> u16 {
    character.kind | character.work.kind
}

fn clear_kind(character: &mut Character, bits: u16) {
    character.kind &= !bits;
    character.work.kind &= !bits;
}

/// `Spl_in4a`, after `Spl_Init`. With `Type`'s `0x300` up, the joints take the
/// other set of arms at once and the bits go. Otherwise the arms wait for
/// `Spl_chenge00`.
fn spl_in4a(character: &mut Character) {
    if marvin_kind(character) & 0x300 != 0 {
        character.arms.swapped = true;
        clear_kind(character, 0x300);
    } else {
        character.arms.fade = 0;
    }
}

/// `Spl_mv4a`'s `Spl_chenge00`: with `Type`'s `0x200`, the fade to the other
/// set of arms, which clears the bit at its end. Room 202's script waits for
/// that before it makes Marvin a zombie. A fade that has run doesn't run again.
/// The fade's light on the arms isn't drawn: the joints swap at its end.
fn spl_mv4a(character: &mut Character) {
    if marvin_kind(character) & 0x200 == 0 || character.arms.faded {
        return;
    }
    character.arms.fade += 1;
    if character.arms.fade == MARVIN_FADE_TICKS {
        character.arms = Arms {
            swapped: true,
            fade: 0,
            faded: true,
        };
        clear_kind(character, 0x200);
    }
}

/// Which mesh each joint draws, where that isn't its own. Marvin's init gives
/// every arm joint a spare part with the same matrix. Until his arms swap the
/// spare draws the joint's own mesh (the hand's from mesh 23 with `Type`'s
/// `0x800`), and the joint draws the other mesh, black and translucent, so
/// that only the spare shows. Once they swap, the joint takes the other mesh.
/// The model decides, not `Id`: as a zombie he keeps his arms.
pub fn joint_meshes(character: &Character, joints: usize, meshes: usize) -> Option<Vec<usize>> {
    if character.model != MARVIN || joints <= 14 || meshes <= MARVIN_GUN_HAND {
        return None;
    }
    let mut joint_meshes: Vec<usize> = (0..joints).collect();
    if character.arms.swapped {
        for (joint, spare) in MARVIN_ARMS {
            joint_meshes[joint] = spare;
        }
    } else if marvin_kind(character) & 0x800 != 0 {
        joint_meshes[11] = MARVIN_GUN_HAND;
    }
    Some(joint_meshes)
}

/// A `Game_loop` pass under `fg_stop` outside the room's own tick (the
/// get-item window's, the status screen's and the item box's open and
/// close, the typewriter's open): each enemy does its `Root_ck` and nothing
/// else.
pub fn tick_stopped(
    game: &mut GameState,
    room: &RoomState,
    pack: &Pack,
    npc_models: &mut EntityModelCache,
    lua: Option<&mut crate::enemy::LuaEnemyHost>,
) {
    run_load_stop(game, room, pack, npc_models, lua);
}

/// The tick the load's `fg_stop` ends: the enemies do no more than a `Root_ck`.
fn run_load_stop(
    game: &mut GameState,
    room: &RoomState,
    pack: &Pack,
    npc_models: &mut EntityModelCache,
    lua: Option<&mut crate::enemy::LuaEnemyHost>,
) {
    let scenario = game.id.player_flag & 1;
    run_load_stop_with(game, room, pack, lua, |model| {
        npc_models.get(pack, model, scenario)
    });
}

/// [`run_load_stop`] with each character's model from `model`.
fn run_load_stop_with(
    game: &mut GameState,
    room: &RoomState,
    pack: &Pack,
    mut lua: Option<&mut crate::enemy::LuaEnemyHost>,
    mut model: impl FnMut(u8) -> Option<std::sync::Arc<Emd>>,
) {
    let was_stopped = game.re2.enemies_stopped();
    game.re2.set_enemies_stopped(true);
    for slot in run_order() {
        if slot == SPLAYER_SLOT {
            // A position a door or a script gave is still pending in the
            // load's last pass.
            partner_root_ck_sys(game, room, true);
        }
        let Some(mut character) = game.re2.characters.0.get_mut(slot).and_then(Option::take) else {
            continue;
        };
        if moves(&character)
            && let Some(mut zombie) = character.zombie.take()
        {
            if let Some(model) = model(character.model) {
                // A stopped enemy only does its `Root_ck`, and sends nothing.
                let _ = run_zombie(
                    game,
                    room,
                    pack,
                    lua.as_deref_mut(),
                    &mut character,
                    &mut zombie,
                    slot,
                    &model,
                    false,
                );
            }
            character.zombie = Some(zombie);
        }
        game.re2.characters.0[slot] = Some(character);
    }
    game.re2.set_enemies_stopped(was_stopped);
}

/// `Game_loop` and `Em_init_move` start at `splayer_work`.
fn run_order() -> impl Iterator<Item = usize> {
    std::iter::once(SPLAYER_SLOT).chain(0..SPLAYER_SLOT)
}

/// `_Spl`'s routine: `Spl_Init` for one no script has given a routine,
/// `Spl_Auto` for Sherry as the partner (`partner2`), and `Spl_Scenario`.
/// Anyone else's `Spl_Auto` isn't ported: they stand still.
fn spl(game: &mut GameState, character: &mut Character, tick: &Tick<'_>) -> Option<u8> {
    let sherry = matches!(character.id, 0x45 | 0x4F);
    let arrived = if player2::uninitialised(&character.work) {
        // `Spl_Init`: `Type`'s `0x400` makes the partner, and the scene's
        // Ada or Sherry stops being one.
        let partner = character.kind & SPL_AUTO != 0;
        // Everyone takes `Type`'s 0x1000; a scene character also takes
        // 0x4000, which sends it to `Spl_Scenario`, and the partner drops
        // 0x1000 again.
        character.kind |= 0x1000;
        if partner {
            character.kind &= !0x1000;
            game.re2.set_partner_bit(true);
        } else {
            character.kind |= 0x4000;
            if matches!(character.id, 0x41 | 0x43 | 0x45 | 0x4F) {
                game.re2.set_partner_bit(false);
            }
        }
        let partner = partner && sherry && character.partner.is_some();
        player2::spl_init(
            &mut character.work,
            &mut character.body,
            tick,
            character.kind,
            partner,
        );
        // `Spl_in41` and `Spl_in45` (from `Spl_Init`'s table) send the
        // partner to `Spl_Auto` whatever 0x4000 chose.
        if matches!(character.id, 0x41 | 0x45 | 0x4F) && game.re2.partner_bit() {
            character.work.routine = [1, 0, 0, 0];
        }
        if character.id == MARVIN {
            spl_in4a(character);
        }
        if sherry && let Some(words) = character.partner.as_deref_mut() {
            words.spl_in45(game.re2.partner_bit(), game.re2.partner_life);
        }
        None
    } else if player2::in_scenario(&character.work) {
        player2::player_move(&mut character.work, &mut character.body, tick)
    } else if player2::in_auto(&character.work)
        && sherry
        && let Some(words) = character.partner.as_deref_mut()
    {
        let (player, player_angle) = game
            .entities
            .first()
            .map_or(([0; 3], 0), |p| (p.pos, p.angle));
        let nearest = spl_enemy_ck(game, tick, character.body.pos, character.work.neck.position);
        let mut timer = game.re2.spl_pos_ck_timer;
        let mut world = crate::partner2::PartnerWorld {
            player,
            player_angle,
            player_routine: game.re2.player.routine[1],
            distance: character.distance,
            nearest,
            nearest_dist: game.re2.nearest_dist,
            rnd: &mut game.re2.rnd,
            pos_ck_timer: &mut timer,
            ground: partner_ground(tick.room, character.body.pos),
            be_flg: character.be_flg,
            noise: 0,
            poisoned: game.re2.partner_poisoned,
        };
        let kind = &mut character.kind;
        player2::spl_auto(&mut character.work, &mut character.body, tick, |mover| {
            crate::partner2::spl_auto(mover, words, kind, &mut world);
        });
        character.be_flg = world.be_flg;
        game.re2.player.noise |= world.noise;
        game.re2.spl_pos_ck_timer = timer;
        None
    } else {
        None
    };
    // `_Spl`'s call by `Id`, after the routine.
    if character.id == MARVIN {
        spl_mv4a(character);
    }
    arrived
}

/// The partner's `Ground`, which `Game_loop` writes after the moves:
/// `Sca_ck_hit` under her with no reach and her `Sc_id`'s mask (`Sce_em_set`
/// gives every `Id` from 0x40 the player's 0x80).
fn partner_ground(room: &RoomState, pos: [i32; 3]) -> i16 {
    room.sca.as_ref().map_or(0, |sca| {
        crate::sca::ground_at(sca, pos, 0, crate::sca::PLAYER_MASK).height as i16
    })
}

/// `Em_Zombie`'s `Root_ck` target: the partner, when she has this zombie
/// first on her list and stands in a guide area, else the player.
fn zombie_root_target(game: &GameState, room: &RoomState, character: &Character) -> [i32; 3] {
    let player = game.entities[0].pos;
    if !game.re2.partner_bit() || character.l_spl & crate::partner2::L_SPL_LISTED != 1 << 30 {
        return player;
    }
    let Some(partner) = game
        .re2
        .characters
        .0
        .get(SPLAYER_SLOT)
        .and_then(Option::as_deref)
    else {
        return player;
    };
    let [x, _, z] = partner.body.pos;
    match crate::zombie2::guide_check(&room.guides, x as i16, z as i16) {
        Some(_) => partner.body.pos,
        None => player,
    }
}

/// `Sherry_move`'s first call, `Spl_enemy_ck`, over the enemy slots: it
/// writes each enemy's `L_spl` and `Dir_spl` and the nearest-enemy cache, and
/// answers where the listed enemies stand.
fn spl_enemy_ck(
    game: &mut GameState,
    tick: &Tick<'_>,
    body: [i32; 3],
    neck: Option<[i32; 3]>,
) -> [Option<[i32; 3]>; 3] {
    let re2 = &mut game.re2;
    let slots: Vec<usize> = (0..SPLAYER_SLOT)
        .filter(|&slot| re2.characters.0.get(slot).is_some_and(Option::is_some))
        .collect();
    let mut views: Vec<crate::partner2::EnemyView> = slots
        .iter()
        .filter_map(|&slot| re2.characters.0[slot].as_deref())
        .map(|enemy| {
            let (kind, life) = match enemy.zombie.as_ref() {
                Some(zombie) => (enemy.work.kind, zombie.life),
                // A script character's `Life` is -1.
                None => (enemy.kind, -1),
            };
            crate::partner2::EnemyView {
                be_flg: enemy.be_flg,
                kind,
                life,
                pos: enemy.body.pos,
                neck: enemy.work.neck.position,
                l_spl: enemy.l_spl,
                dir_spl: enemy.dir_spl,
            }
        })
        .collect();
    let sca = tick
        .room
        .sca
        .as_ref()
        .map_or(&[][..], |sca| sca.records.as_slice());
    let listed = crate::partner2::spl_enemy_ck(
        &mut views,
        re2.enemies,
        body,
        neck,
        sca,
        &mut re2.nearest_dist,
    );
    for (&slot, view) in slots.iter().zip(&views) {
        if let Some(enemy) = re2.characters.0[slot].as_deref_mut() {
            enemy.l_spl = view.l_spl;
            enemy.dir_spl = view.dir_spl;
        }
    }
    re2.nearest_slots = listed.map(|index| index.map(|index| slots[index]));
    listed.map(|index| index.map(|index| views[index].pos))
}

/// `Type`'s bit that gives a character to `Spl_Auto`.
const SPL_AUTO: u16 = 0x400;

/// `_Spl`'s `Spl_Root_ck_sys` for `splayer_work`, which runs before the stop
/// test: on a stopped tick it is all `_Spl` does.
fn partner_root_ck_sys(game: &mut GameState, room: &RoomState, loading: bool) {
    let Some(mut character) = game
        .re2
        .characters
        .0
        .get_mut(SPLAYER_SLOT)
        .and_then(Option::take)
    else {
        return;
    };
    if moves(&character) && character.zombie.is_none() {
        root_ck_sys(game, room, &mut character, loading);
    }
    game.re2.characters.0[SPLAYER_SLOT] = Some(character);
}

/// `loading`: the room's load, where the player is where the door or a
/// script's `pos_set` put them, which the port applies only at the next tick.
fn root_ck_sys(game: &GameState, room: &RoomState, character: &mut Character, loading: bool) {
    let Some(partner) = character.partner.as_deref_mut() else {
        return;
    };
    let sca = room
        .sca
        .as_ref()
        .map_or(&[][..], |sca| sca.records.as_slice());
    let (player, player_angle) = game
        .entities
        .first()
        .map_or(([0; 3], 0), |p| (p.pos, p.angle));
    let pending = &game.re2.pending_player;
    let (player, player_angle) = if loading {
        (
            pending.pos.unwrap_or(player),
            pending.angle.unwrap_or(player_angle),
        )
    } else {
        (player, player_angle)
    };
    let world = crate::partner2::RootSys {
        guides: &room.guides,
        sca,
        player,
        player_angle,
        player_neck: game.re2.player.neck.position,
        partner: game.re2.partner_bit(),
    };
    crate::partner2::root_ck_sys(
        partner,
        &mut character.work,
        character.body.pos,
        character.id,
        character.kind,
        &world,
    );
}

/// The partner's `Oba_ck_em`: pushed out of the player's box as the player's
/// last `Oba_ck_em2` left it, then out of the bodies in the enemy slots. It
/// leaves `At_em_flg` alone.
fn partner_oba_ck_em(game: &GameState, character: &mut Character, old: [i32; 3]) {
    let height = player2::character_height(character.id);
    let width = character.body.radius as u16;
    let pos = character.body.pos;
    let mut atd = crate::zombie2::Atd {
        ofs_x: 0,
        ofs_y: -(height as i16),
        at_w: width,
        at_h: height,
        w: width,
        at: [pos[0], pos[1] - i32::from(height), pos[2]],
    };
    let player = game.entities.first().map_or([0; 3], |p| p.pos);
    let mut bodies = vec![crate::zombie2::Oba {
        slot: crate::zombie2::PLAYER_BODY,
        be_flg: game.re2.player.be_flg,
        atd: game
            .re2
            .player
            .atd
            .unwrap_or_else(|| player_atd(game, player)),
        pos: player,
    }];
    bodies.extend(slot_bodies(game));
    for other in &bodies {
        crate::zombie2::oba_hit(
            other,
            character.be_flg,
            &mut atd,
            &mut character.body.pos,
            Some(old),
        );
    }
}

/// `_Spl`'s `Sca_ck_em`, after its routine.
fn spl_sca_ck_em(room: &RoomState, character: &mut Character) {
    let Some(sca) = &room.sca else {
        return;
    };
    let body = &mut character.body;
    let [old_x, old_z] = body.collision_mirror;
    let prev = [old_x, body.pos[1], old_z];
    let mask = sca_mask(character.id);
    // `1 << nFloor`: the partner keeps hers through the climb and the jump.
    let floor = if character.partner.is_some() {
        character.work.floor
    } else {
        crate::sca::floor_of(body.pos[1])
    };
    let resolved = crate::sca::resolve_on_floor(sca, mask, prev, body.pos, body.radius, floor);
    body.pos = resolved.pos;
    body.collision_mirror = [body.pos[0], body.pos[2]];
    if let Some(partner) = character.partner.as_deref_mut() {
        partner.sca_hit = resolved.hit;
    }
}

/// `Em_init_move`'s `_Spl` calls for a character the room places, each with
/// its `Sca_ck_em`: room 202's Marvin, placed in the wall at the desks'
/// corner, is pushed out while the room loads.
fn em_init_move(
    game: &mut GameState,
    room: &RoomState,
    character: &mut Character,
    tick: &Tick<'_>,
) -> Option<u8> {
    character.with_room = false;
    // `_Spl`'s `Spl_Root_ck_sys` runs before `Spl_Init`, on what the work's
    // memory held: the follow point's kind is taken as 0.
    root_ck_sys(game, room, character, true);
    let arrived = spl(game, character, tick);
    spl_sca_ck_em(room, character);
    let (start, flags) = character.start;
    if start == 0 {
        return arrived;
    }
    player2::spl_start(&mut character.work, start, flags);
    let arrived = spl(game, character, tick).or(arrived);
    spl_sca_ck_em(room, character);
    arrived
}

/// `PartsWork_set`'s `Parts_num`: the model's mesh objects, which can be more
/// than its skeleton's joints (a zombie's 17 parts on 15 joints, parts 15
/// and 16 hidden by `Em10_Init`; a licker's 23 with its tongue).
pub fn parts_num(model: &Emd) -> u8 {
    model.mesh.objects.len().min(32) as u8
}

/// The tick a character's `_Spl` sees: its own model's banks, and the room's
/// animations for its slot.
pub fn tick_of<'a>(
    room: &'a RoomState,
    game: &GameState,
    character: &Character,
    slot: usize,
    model: &'a Emd,
    held: Option<&'a crate::game::PlayerDamageBank>,
) -> Tick<'a> {
    let empty: (&[Clip], &[Keyframe]) = (&[], &[]);
    let room_anim = room
        .character_anims
        .iter()
        .find(|(anims_slot, _)| usize::from(*anims_slot) == slot)
        .map(|(_, anim)| anim);
    let main = (&model.clips[..], &model.keyframes[..]);
    let sub0 = model
        .banks
        .first()
        .map_or(empty, |bank| (&bank.clips[..], &bank.keyframes[..]));
    // `MrX_init` swaps its main and first sub banks, but for the one `Type`.
    let swapped = character.id == crate::zombie2::MRX_ID && character.work.kind & 0xF != 7;
    let (pld, weapon) = if swapped { (sub0, main) } else { (main, sub0) };
    Tick {
        room,
        skeleton: &model.skeleton,
        morphs: &model.morphs,
        banks: Banks {
            pld,
            weapon,
            room: room_anim.map_or(empty, |anim| (&anim.clips, &anim.keyframes)),
            held: held.map_or(empty, |bank| {
                (bank.clips.as_slice(), bank.keyframes.as_slice())
            }),
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
        neck_targets: &[],
        enemies: 0,
        pushing: false,
        objects: &[],
        key_trg: 0,
    }
}

/// `Joint_trans2`'s parts for a character: the pose its banks show, with
/// its work's height and head, composed on its body's matrix (turned by a
/// surface's `super_matrix` when it has one). Returns the parts' world
/// matrices and the pose, or `None` when no pose is up.
pub fn pose_joints(
    room: &RoomState,
    game: &GameState,
    character: &Character,
    slot: usize,
    model: &Emd,
) -> Option<(Vec<crate::anim::Mat4x3>, Keyframe)> {
    let tick = tick_of(
        room,
        game,
        character,
        slot,
        model,
        character.held_bank.as_deref(),
    );
    let (clips, keyframes) = tick.banks.get(character.work.bank);
    let mut keyframe = character.body.anim.pose_keyframe(clips, keyframes)?;
    character.work.apply_to_pose(&mut keyframe);
    let [pitch, roll] = character
        .zombie
        .as_ref()
        .map_or([0; 2], |zombie| zombie.tilt);
    let mut matrix = crate::anim::entity_matrix_rotated_scaled(
        character.body.pos,
        pitch,
        character.body.angle,
        roll,
        0,
    );
    if let Some(surface) = character.work.super_matrix {
        // `matrix.r` turns a flipped Y, and the surface a real one.
        let flip = |row: usize, column: usize| i32::from(row == 1) ^ i32::from(column == 1) != 0;
        let surface = std::array::from_fn(|row| {
            std::array::from_fn(|column| {
                let value = i32::from(surface[row][column]);
                if flip(row, column) { -value } else { value }
            })
        });
        matrix.r = crate::anim::mul_matrix0(&surface, &matrix.r);
    }
    let mut local = crate::anim::joint_local_transforms(&model.skeleton, &keyframe);
    character.work.pose_locals(&mut local);
    let joints = crate::anim::joint_matrices_from_locals(&model.skeleton, &local, &matrix, &[]);
    Some((joints, keyframe))
}

/// `Joint_trans2` for every character in play, before `Om_trans`: keep each
/// slot's parts' world matrices
/// ([`crate::scd2::host::Re2State::character_joints`]), which an object
/// hung on a character's joint composes on.
pub fn keep_joints(
    game: &mut GameState,
    room: &RoomState,
    pack: &Pack,
    npc_models: &mut EntityModelCache,
) {
    keep_joints_with(game, room, |character| {
        let skin = character
            .zombie
            .as_ref()
            .map_or(0, |zombie| zombie.skin.saturating_sub(1));
        npc_models.get_skinned(pack, character.model, room.player_flag & 1, skin)
    });
}

/// [`keep_joints`] with each character's model from `model`: the slots of
/// the characters the draw poses (in play, and not a zombie waiting for the
/// room's scripts), the others empty.
fn keep_joints_with(
    game: &mut GameState,
    room: &RoomState,
    mut model: impl FnMut(&Character) -> Option<std::sync::Arc<Emd>>,
) {
    let characters = &game.re2.characters.0;
    let mut kept = vec![Vec::new(); characters.len()];
    for (slot, character) in characters.iter().enumerate() {
        let Some(character) = character
            .as_deref()
            .filter(|c| c.be_flg & 1 != 0 && !awaits_script(c))
        else {
            continue;
        };
        let Some(model) = model(character) else {
            continue;
        };
        if let Some((joints, _)) = pose_joints(room, game, character, slot, &model) {
            kept[slot] = joints;
        }
    }
    game.re2.character_joints = kept;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::RoomId;

    #[test]
    fn parts_num_counts_the_meshs_parts_not_the_skeletons_joints() {
        use crate::model::{Skeleton, Texture8, Tmd, TmdObject};
        // A zombie's model: 15 joints, 17 parts. `Em10_Init` hides parts 15
        // and 16, which the original's `Parts_num` (17 in room 204's body)
        // still counts.
        let model = Emd {
            skeleton: Skeleton {
                relative: vec![[0; 3]; 15],
                children: vec![Vec::new(); 15],
            },
            keyframes: Vec::new(),
            clips: Vec::new(),
            damage_clips: Vec::new(),
            damage_keyframes: Vec::new(),
            banks: Vec::new(),
            mesh: Tmd {
                objects: vec![TmdObject::default(); 17],
            },
            texture: Texture8 {
                width: 0,
                height: 0,
                indices: Vec::new(),
                palettes: Vec::new(),
                stp: Vec::new(),
            },
            morphs: Vec::new(),
        };
        assert_eq!(parts_num(&model), 17);
    }

    /// A model of 15 joints with one clip of one keyframe, at rest.
    fn posable_model() -> Emd {
        use crate::model::{Clip, ClipFrame, Skeleton, Texture8, Tmd, TmdObject};
        Emd {
            skeleton: Skeleton {
                relative: vec![[0, -100, 0]; 15],
                children: (0..15)
                    .map(|joint| {
                        if joint < 14 {
                            vec![joint + 1]
                        } else {
                            Vec::new()
                        }
                    })
                    .collect(),
            },
            keyframes: vec![Keyframe {
                offset: [0; 3],
                vertex: [0; 3],
                rotations: vec![[0; 3]; 15],
            }],
            clips: vec![Clip {
                frames: vec![ClipFrame::default()],
            }],
            damage_clips: Vec::new(),
            damage_keyframes: Vec::new(),
            banks: Vec::new(),
            mesh: Tmd {
                objects: vec![TmdObject::default(); 15],
            },
            texture: Texture8 {
                width: 0,
                height: 0,
                indices: Vec::new(),
                palettes: Vec::new(),
                stp: Vec::new(),
            },
            morphs: Vec::new(),
        }
    }

    /// `Game_loop`'s pass under `fg_stop` outside the room's own tick (the
    /// status screen's and the item box's open and close, the get-item
    /// window's): a stopped zombie does its `Root_ck`, which counts its turn
    /// (`+0x150`) down, 7 again after 0, and nothing else: it doesn't move,
    /// its routine and damage count hold, and the room's own stop is left
    /// as it was.
    #[test]
    fn a_stopped_pass_counts_a_zombies_turn_and_nothing_else() {
        let model = std::sync::Arc::new(posable_model());
        let pack = Pack::from_bytes(crate::pack::PackWriter::new().to_bytes().unwrap()).unwrap();
        let mut zombie = crate::zombie2::Zombie::default();
        zombie.set_sight_turn(3);
        zombie.damage_cnt = 2;
        let mut character = sherry(0);
        character.id = 0x10;
        character.model = 0x10;
        character.partner = None;
        character.body.pos = [1000, 0, 2000];
        character.body.angle = 0x400;
        character.work.routine = [1, 0, 0, 0];
        character.zombie = Some(zombie);
        let mut game = GameState::default();
        game.re2.characters.0 = vec![Some(Box::new(character))];
        let mut turns = Vec::new();
        for _ in 0..5 {
            run_load_stop_with(&mut game, &RoomState::default(), &pack, None, |_| {
                Some(model.clone())
            });
            let character = game.re2.characters.0[0].as_deref().unwrap();
            let zombie = character.zombie.as_ref().unwrap();
            turns.push(zombie.sight_turn());
            assert_eq!(character.body.pos, [1000, 0, 2000], "no move");
            assert_eq!(character.body.angle, 0x400, "no turn");
            assert_eq!(character.work.routine, [1, 0, 0, 0], "its routine holds");
            assert_eq!(zombie.damage_cnt, 2, "its damage count holds");
        }
        assert_eq!(turns, [2, 1, 0, 6, 5], "Root_ck's turn");
        assert!(!game.re2.enemies_stopped(), "the room's stop as it was");
    }

    #[test]
    fn keep_joints_poses_the_characters_in_play() {
        let model = std::sync::Arc::new(posable_model());
        let mut placed = sherry(0);
        placed.partner = None;
        placed.body.pos = [-24000, 0, -19000];
        placed.work.bank = player2::Bank::Pld;
        let mut out = placed.clone();
        out.be_flg = 0;
        let mut waiting = placed.clone();
        waiting.with_room = true;
        waiting.zombie = Some(crate::zombie2::Zombie::default());
        let mut game = GameState::default();
        game.re2.characters.0 = vec![
            Some(Box::new(placed)),
            Some(Box::new(out)),
            None,
            Some(Box::new(waiting)),
        ];
        keep_joints_with(&mut game, &RoomState::default(), |_| Some(model.clone()));
        let kept = &game.re2.character_joints;
        assert_eq!(kept.len(), 4);
        assert_eq!(kept[0].len(), 15, "in play: its parts");
        assert_eq!(kept[0][0].t, [-24000, 0, -19000], "on its body's matrix");
        assert!(kept[1].is_empty(), "out of play");
        assert!(kept[2].is_empty(), "no character");
        assert!(kept[3].is_empty(), "waiting for the room's scripts");
        // No model: nothing kept.
        keep_joints_with(&mut game, &RoomState::default(), |_| None);
        assert!(game.re2.character_joints.iter().all(Vec::is_empty));
    }

    #[test]
    fn a_character_stops_when_out_of_play_or_told_not_to_move() {
        let mut character = Character {
            id: 0x45,
            model: 0x45,
            em_set: (0, 0, NO_ENEMY_FLAG),
            arms: Arms::default(),
            be_flg: 1,
            work: player2::Work::default(),
            body: crate::player::spawn(RoomId::parse("1140").unwrap(), &RoomState::default()),
            kind: 0,
            kage: None,
            l_spl: 0,
            dir_spl: 0,
            start: (0, 0),
            sound_type: 0,
            distance: 0,
            area: crate::scd2::host::NO_AREA,
            with_room: false,
            held_bank: None,
            partner: None,
            zombie: None,
        };
        assert!(moves(&character));
        character.work.kind = player2::KIND_NOT_MOVED;
        assert!(!moves(&character));
        character.work.kind = 0;
        character.be_flg = 0;
        assert!(!moves(&character));
    }

    #[test]
    fn no_enemies_switches_off_every_slot_but_the_kept_rooms_ones() {
        let room = RoomId::parse("1010").unwrap();
        let character = Character {
            id: 0x48,
            model: 0x48,
            em_set: (0, 0, NO_ENEMY_FLAG),
            arms: Arms::default(),
            be_flg: 3,
            work: player2::Work::default(),
            body: crate::player::spawn(room, &RoomState::default()),
            kind: 0,
            kage: None,
            l_spl: 0,
            dir_spl: 0,
            start: (0, 0),
            sound_type: 0,
            distance: 0,
            area: crate::scd2::host::NO_AREA,
            with_room: false,
            held_bank: None,
            partner: None,
            zombie: None,
        };
        let mut game = GameState::default();
        game.id = room;
        game.re2.characters.0 = vec![
            Some(Box::new(character.clone())),
            None,
            Some(Box::new(character)),
        ];
        let flags = |game: &GameState| {
            game.re2
                .characters
                .0
                .iter()
                .map(|c| c.as_ref().map(|c| c.be_flg))
                .collect::<Vec<_>>()
        };

        game.apply_re2_no_enemies();
        assert_eq!(
            flags(&game),
            [Some(3), None, Some(3)],
            "no setup leaves them alone"
        );

        game.re2_no_enemies = Some(vec![(0x101, 0), (0x102, 2)]);
        game.apply_re2_no_enemies();
        assert_eq!(
            flags(&game),
            [Some(3), None, Some(2)],
            "slot 0 is kept in room 101 only"
        );

        game.id = RoomId::parse("1020").unwrap();
        game.re2.characters.0[0].as_mut().unwrap().be_flg = 1;
        game.re2.characters.0[2].as_mut().unwrap().be_flg = 1;
        game.apply_re2_no_enemies();
        assert_eq!(flags(&game), [Some(0), None, Some(1)]);
    }

    #[test]
    fn claire_is_pushed_out_of_a_body_with_her_own_width() {
        // Kendo's body in room 101 (`At_w` 450) and the player walking past it,
        // 850 units off: Leon (450 wide) touches it, Claire (350) doesn't.
        let room = RoomId::parse("1010").unwrap();
        let mut body = crate::player::spawn(room, &RoomState::default());
        body.pos = [0, 0, 0];
        body.radius = 450;
        let kendo = Character {
            id: 0x48,
            model: 0x48,
            em_set: (0, 0, NO_ENEMY_FLAG),
            arms: Arms::default(),
            be_flg: 0x2005,
            work: player2::Work::default(),
            body,
            kind: 0,
            kage: None,
            l_spl: 0,
            dir_spl: 0,
            start: (0, 0),
            sound_type: 0,
            distance: 0,
            area: crate::scd2::host::NO_AREA,
            with_room: false,
            held_bank: None,
            partner: None,
            zombie: None,
        };
        for (player_flag, x) in [(0, -901), (1, -850)] {
            let mut game = GameState::default();
            game.id = room;
            game.id.player_flag = player_flag;
            game.re2.characters.0 = vec![Some(Box::new(kendo.clone()))];
            let mut player = crate::player::spawn(room, &RoomState::default());
            player.pos = [-850, 0, 0];
            oba_ck_em2(&mut game, &mut player, [-850, 0, 0]);
            assert_eq!(player.pos, [x, 0, 0], "player {player_flag}");
            assert_eq!(game.re2.player.at_em_flg, u32::from(player_flag == 0));
        }
    }

    #[test]
    fn touching_the_partner_is_not_a_touch_of_enemy_slot_0() {
        let room = RoomId::parse("5021").unwrap();
        let mut partner = sherry(0x400);
        partner.be_flg = 0x2005;
        let mut game = GameState::default();
        game.id = room;
        game.id.player_flag = 1;
        game.re2.characters.0 = vec![None; SPLAYER_SLOT + 1];
        game.re2.characters.0[SPLAYER_SLOT] = Some(Box::new(partner));
        let mut player = crate::player::spawn(room, &RoomState::default());
        player.pos = [-400, 0, 0];
        oba_ck_em2(&mut game, &mut player, [-400, 0, 0]);
        assert_eq!(game.re2.player.at_em_flg, 0);
        assert!(game.re2.player.at_em_partner);
    }

    fn sherry(kind: u16) -> Character {
        let mut body = crate::player::spawn(RoomId::parse("21B1").unwrap(), &RoomState::default());
        body.pos = [0, 0, 0];
        body.radius = 250;
        let mut work = player2::Work::default();
        work.routine = [0; 4];
        Character {
            id: 0x45,
            model: 0x45,
            em_set: (0, 0, NO_ENEMY_FLAG),
            arms: Arms::default(),
            be_flg: 1,
            work,
            body,
            kind,
            kage: None,
            l_spl: 0,
            dir_spl: 0,
            start: (0, 0),
            sound_type: 0,
            distance: 0,
            area: crate::scd2::host::NO_AREA,
            with_room: false,
            held_bank: None,
            partner: Some(Box::new(crate::partner2::Partner::placed())),
            zombie: None,
        }
    }

    /// `_Spl` with no model: banks without clips.
    fn spl_tick(room: &RoomState, skeleton: &crate::model::Skeleton) -> Tick<'static> {
        let empty: (&[Clip], &[Keyframe]) = (&[], &[]);
        // The test's room and skeleton live as long as the test.
        let room: &'static RoomState = Box::leak(Box::new(room.clone()));
        let skeleton: &'static crate::model::Skeleton = Box::leak(Box::new(skeleton.clone()));
        Tick {
            room,
            skeleton,
            morphs: &[],
            banks: Banks {
                pld: empty,
                weapon: empty,
                room: empty,
                held: empty,
            },
            input: Default::default(),
            options: Default::default(),
            id: 0x45,
            life: 200,
            poisoned: false,
            stopped: false,
            weapon: Default::default(),
            necks: [None; player2::NECK_WORKS],
            targets: &[],
            neck_targets: &[],
            enemies: 0,
            pushing: false,
            objects: &[],
            key_trg: 0,
        }
    }

    #[test]
    fn a_partner_sherry_raises_the_partner_bit_and_keeps_routine_1() {
        let mut game = GameState::default();
        game.re2.partner_life = 150;
        let mut character = sherry(0x400);
        let tick = spl_tick(&RoomState::default(), &Default::default());
        spl(&mut game, &mut character, &tick);
        assert!(game.re2.partner_bit());
        assert_eq!(character.work.routine[0], 1);
        assert_eq!(character.kind, 0x400);
        let partner = character.partner.as_deref().unwrap();
        assert_eq!((partner.life, partner.max_life), (150, 200));
        assert_eq!(character.work.neck.flags, 0);
        assert_eq!(
            character.work.neck.target,
            Some(0),
            "she looks at the player"
        );
    }

    #[test]
    fn a_scene_sherry_clears_the_partner_bit() {
        let mut game = GameState::default();
        game.re2.set_partner_bit(true);
        let mut character = sherry(0);
        let tick = spl_tick(&RoomState::default(), &Default::default());
        spl(&mut game, &mut character, &tick);
        assert!(!game.re2.partner_bit());
        assert_eq!(character.work.routine[..2], [4, 6]);
        assert_eq!(character.kind, 0x5000, "Spl_Init's 0x4000 and 0x1000");
        let partner = character.partner.as_deref().unwrap();
        assert_eq!((partner.life, partner.max_life), (-1, -1));
        assert_eq!(character.work.neck.flags, 0x92);
    }

    /// `Spl_Init` drops the partner's 0x1000 and sends anyone with 0x4000 to
    /// `Spl_Scenario`, but `Spl_in45` sends the partner Sherry to `Spl_Auto`
    /// even with 0x4000.
    #[test]
    fn spl_init_drops_the_partners_0x1000_and_spl_in45_keeps_her_on_auto() {
        let mut game = GameState::default();
        let mut character = sherry(0x1400);
        let tick = spl_tick(&RoomState::default(), &Default::default());
        spl(&mut game, &mut character, &tick);
        assert_eq!(character.kind, 0x400);
        assert_eq!(character.work.routine[0], 1);
        let mut character = sherry(0x4400);
        spl(&mut game, &mut character, &tick);
        assert_eq!(character.work.routine[..2], [1, 0]);
        assert!(game.re2.partner_bit());
    }

    #[test]
    fn the_partners_box_is_810_high_and_250_wide() {
        assert_eq!(player2::character_height(0x45), 810);
        assert_eq!(player2::character_height(0x4F), 810);
        assert_eq!(player2::character_height(0x48), 0x5FA);
        let mut game = GameState::default();
        game.re2.characters.0 = vec![Some(Box::new(sherry(0x400)))];
        let bodies = slot_bodies(&game);
        let atd = bodies[0].atd;
        assert_eq!((atd.ofs_y, atd.at_h, atd.at_w), (-810, 810, 250));
    }

    #[test]
    fn sherry_is_pushed_out_of_claire() {
        let mut game = GameState::default();
        game.id = RoomId::parse("21B1").unwrap();
        let mut player = crate::player::spawn(game.id, &RoomState::default());
        player.pos = [-400, 0, 0];
        game.entities[0].pos = player.pos;
        // The box the player's last `Oba_ck_em2` left.
        let at = player.pos;
        oba_ck_em2(&mut game, &mut player, at);
        let mut character = sherry(0x400);
        partner_oba_ck_em(&game, &mut character, [0; 3]);
        assert!(character.body.pos[0] > 0, "{:?}", character.body.pos);
        assert_eq!(character.body.pos[2], 0);
    }

    #[test]
    fn marvins_right_hand_holds_the_gun_with_type_0x800() {
        let mut character = Character {
            id: MARVIN,
            model: MARVIN,
            em_set: (0, 0, NO_ENEMY_FLAG),
            arms: Arms::default(),
            be_flg: 1,
            work: player2::Work::default(),
            body: crate::player::spawn(RoomId::parse("2020").unwrap(), &RoomState::default()),
            kind: 0x5800,
            kage: None,
            l_spl: 0,
            dir_spl: 0,
            start: (0, 0),
            sound_type: 0,
            distance: 0,
            area: crate::scd2::host::NO_AREA,
            with_room: false,
            held_bank: None,
            partner: None,
            zombie: None,
        };
        let meshes = joint_meshes(&character, 15, 24).unwrap();
        assert_eq!(meshes[11], MARVIN_GUN_HAND);
        assert_eq!(meshes[10], 10);
        // Init with `0x100` swaps the arms for the spare meshes, the hand mesh 22.
        character.kind = 0x5900;
        spl_in4a(&mut character);
        assert_eq!(character.kind, 0x5800);
        let meshes = joint_meshes(&character, 15, 24).unwrap();
        assert_eq!(&meshes[8..15], &[17, 20, 21, 22, 18, 15, 19]);
        // As a zombie he keeps them: the model decides.
        character.id = 0x1E;
        assert!(joint_meshes(&character, 15, 24).is_some());
        character.model = 0x45;
        assert!(joint_meshes(&character, 15, 24).is_none());
    }

    #[test]
    fn marvins_arms_fade_for_127_ticks_then_clear_the_bit() {
        let mut character = Character {
            id: MARVIN,
            model: MARVIN,
            em_set: (0, 0, NO_ENEMY_FLAG),
            arms: Arms::default(),
            be_flg: 1,
            work: player2::Work::default(),
            body: crate::player::spawn(RoomId::parse("2020").unwrap(), &RoomState::default()),
            kind: 0x5000,
            kage: None,
            l_spl: 0,
            dir_spl: 0,
            start: (0, 0),
            sound_type: 0,
            distance: 0,
            area: crate::scd2::host::NO_AREA,
            with_room: false,
            held_bank: None,
            partner: None,
            zombie: None,
        };
        spl_in4a(&mut character);
        assert!(!character.arms.swapped);
        // Room 202's script sets the bit in the work's `Type`.
        character.work.kind = 0x200;
        for _ in 1..MARVIN_FADE_TICKS {
            spl_mv4a(&mut character);
        }
        assert!(!character.arms.swapped);
        assert_eq!(character.work.kind, 0x200);
        spl_mv4a(&mut character);
        assert!(character.arms.swapped);
        assert_eq!(character.work.kind, 0);
        // A second fade never ends.
        character.work.kind = 0x200;
        for _ in 0..200 {
            spl_mv4a(&mut character);
        }
        assert_eq!(character.work.kind, 0x200);
    }
}
