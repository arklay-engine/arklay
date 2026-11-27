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
        } else {
            enemy_spl_root_ck_sys(game, room, slot, false);
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
            // Its parts as the last frame drew them, for a work that reads
            // them through its `pSin_parts_ptr` after this one's tick.
            character.work.drawn_parts.clone_from(&zombie.drawn);
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
            character.work.drawn_parts.clone_from(&character.work.parts);
            let arrived = if player2::in_special(&character.work) {
                spl_sp_move(game, &mut character, &tick, slot);
                None
            } else {
                spl(game, &mut character, &tick)
            };
            if let Some(flag) = arrived {
                game.re2.raise_room_flag(flag);
            }
            spl_after_routine(game, &mut character, old);
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
            player2::latch_drawn(&mut character.work, &mut character.body, &tick);
        }
        game.re2.characters.0[slot] = Some(character);
    }
}

/// The player's `Atd[0]` at `pos`, as `Oba_set_ofs` leaves it.
fn player_atd(game: &GameState, pos: [i32; 3]) -> crate::zombie2::Atd {
    let width = player_width(game.re2.pl_id);
    let height = player_height(game.re2.pl_id);
    crate::zombie2::Atd {
        ofs_x: 0,
        ofs_y: -(height as i16),
        at_w: width,
        at_h: height,
        w: width,
        at: [pos[0], pos[1] - i32::from(height), pos[2]],
    }
}

/// `Pl_init`'s `Atd[0]`: the sizes of the player it makes (`Ofs_y`, `At_w`,
/// `At_h`, `W`), at the place the last room left the box, which it doesn't
/// write. With none before (a new game, a load, a warp) there is none to
/// keep: the load's first move takes a box on the player's own place
/// (`player_atd`), where the original's holds whatever its memory did.
pub(crate) fn pl_init_atd(game: &mut GameState) {
    let Some(kept) = game.re2.player.atd else {
        return;
    };
    let sized = player_atd(game, [0; 3]);
    game.re2.player.atd = Some(crate::zombie2::Atd {
        at: kept.at,
        ..sized
    });
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
    re2_slot_bodies(&game.re2)
}

/// [`slot_bodies`] of the RE 2 state alone.
pub(crate) fn re2_slot_bodies(re2: &crate::scd2::host::Re2State) -> Vec<crate::zombie2::Oba> {
    let in_play = re2.characters.0.iter().enumerate();
    in_play
        .filter_map(|(slot, other)| Some((slot, other.as_deref()?)))
        .filter(|(_, other)| other.be_flg & 1 != 0)
        // `nOba` 0: no box to push the others out of.
        .filter(|(_, other)| !other.zombie.as_ref().is_some_and(|z| z.no_oba))
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
    game.re2.player.oba_ran = true;
}

/// The end of `Game_loop`'s pass over the works: `Player_move` and
/// `Oba_ck_em2` run on every tick of it, a stopped one too (`fg_stop` holds
/// the moves, not the push), unless the player's `Type` has 0x8000. Where the
/// player's own tick didn't run (a door's stop, a scene's hold), the push
/// runs here from where the player stands, and `Atd[0]` follows it: room
/// 21A's sleeping dog, pushed on the load's pass out of where Leon stood at
/// 219's door 2, isn't pushed out of it again on its first live tick.
pub fn oba_ck_em2_unmoved(game: &mut GameState, player: &mut crate::player::PlayerState) {
    if std::mem::take(&mut game.re2.player.oba_ran)
        || game.re2.player.kind & player2::KIND_NOT_MOVED != 0
    {
        return;
    }
    // `Game_loop`'s draw pass sets `Old_x` to `Pos_x` at the end of every
    // tick, so a player held still has its old place where it stands.
    let at = player.pos;
    oba_ck_em2(game, player, at);
    // The flag marks the player's own move; the next tick starts clear.
    game.re2.player.oba_ran = false;
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
        die: false,
        knocks: 0,
    };
    player2::player_held(&mut re2.player, player, tick, &mut held);
    let (released, knocks, die) = (held.released, held.knocks, held.die);
    game.re2.player.die_start |= die;
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

/// `+0x1EE` (`Eff_at_r`), which widens a shot's first box (`Weapon_at_ck`)
/// and a blast's (`FUN_004d7e30`) by a quarter of it: each kind's init
/// (`enemy_init_table`, `Em_move_tbl_set`) writes it, and both lickers'
/// box sets (`Em22_Atd_set_1`, `Em22_Atd_set_2`, which 0x24's `0x46cfa0`
/// and `0x46e350` call too) by the surface.
fn eff_at_r(id: u8, licker: Option<&crate::zombie2::licker::Licker>) -> u16 {
    use crate::zombie2 as z;
    if let Some(licker) = licker {
        return licker.widen();
    }
    match id {
        // `Em20_Init`.
        z::DOG_ID => 600,
        // `FUN_0045fd50`.
        z::CROW_ID => 300,
        // `FUN_00468a10`.
        z::ALLIGATOR_ID => 0x44C,
        // `FUN_0046f0c0`.
        z::SPIDER_ID => 800,
        // `FUN_004736a0`.
        z::YOUNG_ID => 0x14,
        // `FUN_004742e0`, for every `Type`.
        z::EMBRYO_ID => 300,
        // `FUN_004771d0`.
        z::ADULT_ID => 800,
        // `FUN_0047c0e0`.
        z::ROACH_ID => 800,
        // `MrX_init`.
        z::MRX_ID => 800,
        // `FUN_00483eb0`.
        z::EM2D_ID => 800,
        // `FUN_00484c90`, which `Em_2E` runs for the poison ivy (0x39) too.
        z::IVY_ID | z::POISON_IVY_ID => 800,
        // `FUN_00488a70`.
        z::VINE_ID => 0x4B0,
        // `FUN_004903f0`.
        z::G2_ID => 800,
        // `FUN_00496a80`.
        z::G3_ID => 900,
        // `FUN_004acd70`.
        z::MOTH_ID => 0x384,
        // `FUN_004b03d0`, for the kinds it ports; `Type` 0x2003 to 0x2006
        // (on a wall, not ported) take 400 from `FUN_004b0660`.
        z::LARVA_ID => 200,
        // `Em10_Init`'s, and G1's (`FUN_0048a540`).
        _ => 500,
    }
}

/// [`targets`], from the RE 2 state alone.
pub fn targets_of(re2: &crate::scd2::host::Re2State) -> Vec<crate::weapon2::Target> {
    let characters = re2.characters.0.iter().enumerate();
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
                at_y: zombie.atd.at[1],
                at_w: zombie.atd.at_w,
                widen: eff_at_r(character.id, zombie.licker.as_deref()),
                neck_y: character.work.neck.position.map_or(pos[1], |neck| neck[1]),
                ofs_y: zombie.atd.ofs_y,
                at_h: zombie.atd.at_h,
                angle: character.body.angle,
            })
        })
        .collect()
}

/// The `Rnd` the player's move drew, in `pl_mv05_w1_t2`'s order:
/// `Shoot_camera`'s on the shot's first tick, `Weapon_at_ck`'s for the body it
/// found (the Beretta's roll and `hard_mode`'s), then the splashes'. The
/// move's own `Esp_call`s follow `Shoot_camera`'s ([`take_hit`]).
pub fn shot_draws(game: &mut GameState, cut: Option<&crate::state::Cut>) {
    shoot_camera(game, cut);
    let origin = game.origin();
    let room_code = u16::from(origin.stage) << 8 | u16::from(origin.room);
    let re2 = &mut game.re2;
    let hard = re2.difficulty().hard;
    re2.player.shot_draws(&mut re2.rnd, hard, room_code);
}

/// `pl_mv05_w1_t2`'s call of `Shoot_camera`: with few enemies about, a gun
/// fired along the camera's view throws sparks at the lens.
fn shoot_camera(game: &mut GameState, cut: Option<&crate::state::Cut>) {
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
        if let (Some(index), Some(turn)) = (index, call.turn) {
            game.re2.effects.turn(index, turn);
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

/// Each room object's `Work_m` (`+0x54`), by slot, for the effects started on
/// one: the matrix `Obj_model_set` composed and `Om_trans` composes again each
/// frame it draws the object, kept as it was while the object isn't drawn.
/// [`crate::objects::ObjectRecord::world`] keeps it with the draw's builder
/// and its turn with Y flipped. While that is still the object's `Cdir` and
/// `Pos` on the world or on another object, it is composed again as the
/// original does it, with `RotMatrix` and `CompM` ([`pc_work_m`]).
pub(crate) fn object_worlds(game: &GameState) -> Vec<player2::Matrix> {
    let records = &game.objects.records;
    (0..records.len())
        .map(|index| {
            pc_work_m(records, index, 0).unwrap_or_else(|| player2::Matrix {
                m: player2::flip_y(records[index].world.r).map(|row| row.map(|v| v as i16)),
                t: records[index].world.t,
            })
        })
        .collect()
}

/// `CompM(+0x80, RotMatrix(Cdir) at Pos)` for an object on the world or on
/// another object, when [`crate::objects::ObjectRecord::world`] shows it is
/// what `Om_trans` last composed: `None` for one on a character, or one
/// whose `Pos`, `Cdir` or parent has changed since (a hidden object keeps
/// its old `Work_m`) or that `Om_trans` changed after (a box's lift).
fn pc_work_m(
    records: &[crate::objects::ObjectRecord],
    index: usize,
    depth: usize,
) -> Option<player2::Matrix> {
    use crate::objects::SuperParent;
    let record = records.get(index)?;
    let identity = crate::anim::Mat4x3 {
        r: [[4096, 0, 0], [0, 4096, 0], [0, 0, 4096]],
        t: [0; 3],
    };
    let (drawn_parent, parent) = match record.super_parent {
        SuperParent::World => (
            identity,
            player2::Matrix {
                m: [[0x1000, 0, 0], [0, 0x1000, 0], [0, 0, 0x1000]],
                t: [0; 3],
            },
        ),
        SuperParent::Object(of) if depth < 0x14 => {
            let of = usize::from(of);
            (records.get(of)?.world, pc_work_m(records, of, depth + 1)?)
        }
        _ => return None,
    };
    let own = crate::anim::Mat4x3 {
        r: crate::objects::rotation(record),
        t: record.pos,
    };
    if crate::anim::compose(&drawn_parent, &own) != record.world {
        return None;
    }
    Some(parent.comp(&player2::Matrix {
        m: player2::rot_matrix(record.rotation),
        t: record.pos,
    }))
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
pub(crate) fn run_esprs(
    game: &mut GameState,
    room: &RoomState,
    pack: &Pack,
    npc_models: &mut EntityModelCache,
    player_parts: &[(usize, player2::Matrix)],
) {
    let scenario = game.id.player_flag & 1;
    for (call, matrix, root) in std::mem::take(&mut game.re2.pending_esprs) {
        let world = match matrix.slot {
            None => matrix.part.and_then(|part| {
                player_parts
                    .iter()
                    .find(|(at, _)| *at == part)
                    .map(|(_, world)| *world)
            }),
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
        // The player's `M` as the opcode read it, not where the pass left him.
        let world = world.or(root).unwrap_or_else(|| {
            let player = &game.entities[0];
            root_matrix(player.pos, player.angle)
        });
        let at = place_on(&world, call.at);
        let work = crate::esp2::WorkMatrix {
            slot: matrix.slot,
            part: matrix.part,
        };
        game.re2
            .effects
            .call_placed_on_work(EsprCall { at, ..call }, work);
    }
}

/// `Get_matrix`'s matrix applied to the call's vector: where in the world
/// the effect starts.
fn place_on(world: &player2::Matrix, at: [i16; 3]) -> [i16; 3] {
    let at = at.map(i32::from);
    std::array::from_fn(|axis| {
        let turned: i32 = (0..3).map(|i| i32::from(world.m[axis][i]) * at[i]).sum();
        (world.t[axis] + (turned >> 12)) as i16
    })
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
    pending_place: bool,
) -> Vec<usize> {
    let tick = tick_of(room, game, character, slot, model, None);
    zombie.parts_num = parts_num(model);
    let (player, player_angle) = player_at(game, pending_place);
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
    let root_target = zombie_root_target(game, room, character, pending_place);
    let stopped = game.re2.character_stopped(slot);
    let easy = game.re2.flag(0, FG_SYSTEM_EASY);
    let difficulty = game.re2.difficulty();
    let enemies = game.re2.enemies;
    let noise = game.re2.player.noise;
    let player_floor = game.re2.player.floor;
    let player_neck = game.re2.player.neck.position;
    let origin = game.origin();
    let stage = origin.stage;
    let room_code = u16::from(stage) << 8 | u16::from(origin.room);
    let player_id = game.re2.pl_id;
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
    // `scd_var_cut` (the alligator's tests at `0x469D94` and `0x469EE4`),
    // not the live cut a zone may have moved this tick.
    let camera_cut = game.re2.scd_var_cut();
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
    // The alligator's scene flags, and an ivy shut in until flag 4:95 lets it
    // out (state 11, `0x486ee0`).
    let reads_flags = alligator
        || matches!(
            character.id,
            crate::zombie2::IVY_ID | crate::zombie2::POISON_IVY_ID
        );
    let banks = if reads_flags {
        re2.banks.to_vec()
    } else {
        Vec::new()
    };
    let key_trg = re2.key_trg;
    let gacha = key_trg & 0x34F != 0;
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
    let hung_on = zombie
        .hung
        .and_then(|hung| hung_on(&re2.characters.0, slot, hung));
    // The G embryo rides on `enemy_work[0]` (Ben in 301): its parts as the
    // last frame's `Joint_trans2` left them (Ben has moved this tick), and
    // its facing now.
    let work0 = (character.id == crate::zombie2::EMBRYO_ID && slot != 0)
        .then(|| re2.characters.0.first().and_then(Option::as_deref))
        .flatten()
        .map(|other| crate::zombie2::Work0View {
            parts: other.work.drawn_parts.clone(),
            angle: other.body.angle,
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
        key_trg,
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
        hung_on,
        work0,
        work0_routines: None,
        works,
        spawns: Vec::new(),
        freed: Vec::new(),
        killed: false,
        kills: 0,
        water,
        banks,
        flag_ops: Vec::new(),
        status_and_not: 0,
        cut: None,
        cut_load: false,
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
    let freed = std::mem::take(&mut world.freed);
    let killed = world.killed;
    let kills = world.kills;
    let moved = world.player_moved.then_some(world.player);
    let status_or = world.status_or;
    let status_and_not = world.status_and_not;
    let flag_ops = std::mem::take(&mut world.flag_ops);
    let cut = world.cut;
    let cut_load = world.cut_load;
    let sub1 = world.sub1;
    let objects = std::mem::take(&mut world.objects);
    let hidden_objects = std::mem::take(&mut world.hidden_objects);
    let knocks = std::mem::take(&mut world.knocks);
    let kin = std::mem::take(&mut world.kin);
    let work0_routines = world.work0_routines.take();
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
    re2.kills = re2.kills.wrapping_add(kills);
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
    if cut_load {
        game.re2.bg_load = true;
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
    // A work the script took out of play outright: it counts out of the room
    // (`Set_enemy_work` counts a taken one back in).
    for freed in freed {
        if let Some(other) = re2.characters.0.get_mut(freed).and_then(Option::as_mut) {
            other.be_flg = 0;
            re2.enemies = re2.enemies.wrapping_sub(1);
        }
    }
    if let Some(routines) = work0_routines {
        // `enemy_work[0]`, which is this work when it runs in slot 0.
        let work0 = if slot == 0 {
            Some(&mut character.work)
        } else {
            re2.characters
                .0
                .first_mut()
                .and_then(Option::as_mut)
                .map(|other| &mut other.work)
        };
        if let Some(work0) = work0 {
            work0.routine[..3].copy_from_slice(&routines);
        }
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
        let bank = re2.enemy_flag_bank(origin.re2_stage());
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
                .push(crate::scd2::host::SoundRequest::enemy(id, pos));
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

/// The player's `Atd[0]` width, `At_w` and `W`, by `pPl.Id`: `Player_set`'s
/// 450, which `Pl_init` makes 350 for Claire's bodies and Ada and 250 for
/// Sherry, as it does the radius the room's collision pushes with.
pub fn player_width(id: u8) -> u16 {
    crate::sca::player_radius(id) as u16
}
/// The player's `At_h`, and how far its box's `Ofs_y` lifts it, as
/// `Player_set` (0x4D93A0) leaves them.
pub const PLAYER_HEIGHT: u16 = 0x5FA;
/// `Pl_init` (0x4D9940) lowers the box of Claire's bodies and Sherry (ids 1, 3,
/// 5, 7, 9 and 0xE) to an `At_h` and `Ofs_y` of 0x546, and id 0xF's to 0x32A.
/// `Oba_ck_hit` (0x4CC150) reads it: a crow flying low over Claire (room 109)
/// clears her head where it would touch Leon's.
pub fn player_height(id: u8) -> u16 {
    match id {
        1 | 3 | 5 | 7 | 9 | 0xE => 0x546,
        0xF => 0x32A,
        _ => PLAYER_HEIGHT,
    }
}
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
        game.re2.effects.object_worlds = object_worlds(game);
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

/// Ada's ids, which `Spl_Init`'s table (`0x53cbc8`) gives `Spl_in41`.
const ADA_IDS: [u8; 2] = [0x41, 0x43];

/// Ada's right hand and the part that holds its other mesh, which
/// `Spl_in41` swaps (`Type`'s `0x800`).
const ADA_HAND: usize = 11;
const ADA_SPARE_HAND: usize = 15;

/// Ada's part 16, which `Spl_in41` puts on only with scenario flag 80
/// (`fg_scenario`'s `0x50`), hung from part 0 at (0x42, -0x28e, 0).
const ADA_PART_16: usize = 16;
const ADA_PART_16_FLAG: u8 = 0x50;

/// `Spl_in41` (`0x4efc70`), after `Spl_Init`'s bits: the partner (`Type`'s
/// `0x400`) takes `0x800`, and `0x800` swaps the hand's meshes with the spare
/// part's and goes. The spare part is always off, part 16 is off but for
/// scenario flag 80 (`flag_80`), and `0x80` marks the work as set up. Its
/// partner's words are `Spl_in45`'s, and its partner gets `Spl_Auto`'s
/// routine whatever `Type`'s `0x4000` chose.
///
/// Not ported: part 16's hanging from part 0 (the port's parts keep the
/// model's tree, so with flag 80 its mesh isn't drawn), part 0's `Be_flg`
/// `0x4000`, and `Aida_Jh_init` with its `Aida_before`, `Aida_Jh_move`
/// (`0x4b4680`), which every tick moves the vertices at five joints (parts
/// 1, 2, 5, 9 and 12, the tables at `0x52c5ec` to `0x52c868`) to close the
/// skin's seams.
fn spl_in41(character: &mut Character, flag_80: bool) {
    if character.kind & 0x400 != 0 {
        character.kind |= 0x800;
    }
    character.work.hidden_parts |= 1 << ADA_SPARE_HAND;
    if flag_80 {
        character.work.hidden_parts &= !(1 << ADA_PART_16);
    } else {
        character.work.hidden_parts |= 1 << ADA_PART_16;
    }
    if character.kind & 0x800 != 0 {
        character.kind &= !0x800;
        character.arms.swapped = !character.arms.swapped;
    }
    character.kind |= 0x80;
}

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
    character.work.hidden_parts |= 1 << MARVIN_GUN_HAND;
    if marvin_kind(character) & 0x300 != 0 {
        character.arms.swapped = true;
        hide_spare_arms(character, false);
        clear_kind(character, 0x300);
    } else {
        character.arms.fade = 0;
    }
}

/// The body on Irons's desk in room 21B, one part with one clip.
const DESK_BODY: u8 = 0x4B;

/// `Spl_Scenario`'s routine `0x14`, `nullsub_1`: the body stays as it is.
const SCE_STILL: u8 = 0x14;

/// `Spl_in4b` (`0x4f02b0`), after `Spl_Init`: the part keeps the clip's first
/// frame but for its height, which goes to 0, and the routine does nothing
/// from then on. `Neck_flg`'s 1 holds the head (`Rot_neck`) and `Neck_no` 0
/// makes the one part the head, and `Be_flg`'s 8 keeps it out of
/// `Sca_ck_em`.
fn spl_in4b(character: &mut Character) {
    character.work.part0_y = Some(0);
    character.work.neck.flags = 1;
    character.work.neck.joint = Some(0);
    character.be_flg |= 0xA;
    character.work.routine = [player2::MODE_SCENARIO, SCE_STILL, 0, 0];
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
        hide_spare_arms(character, true);
        clear_kind(character, 0x200);
    }
}

/// Once the arms swap, the spare parts are off (`Be_flg` bit 0 clear), as the
/// gun hand has been since `Spl_in4a`: as a zombie his parts are 15 to 23
/// hidden. `black`: the fade's end also leaves the spares black (the
/// original's `zp`, `ff8000,808080,8888880`); `Type`'s `0x300` at init leaves
/// them at their 0x808080. The joints draw the spares' meshes, so this only
/// changes the parts' state, not the draw (`joint_meshes` reads it by joint).
fn hide_spare_arms(character: &mut Character, black: bool) {
    for (_, spare) in MARVIN_ARMS {
        character.work.hidden_parts |= 1 << spare;
        if black {
            character.work.part_rgb[spare] = 0;
        }
    }
}

/// Which mesh each joint draws, where that isn't its own. Marvin's init gives
/// every arm joint a spare part with the same matrix. Until his arms swap the
/// spare draws the joint's own mesh (the hand's from mesh 23 with `Type`'s
/// `0x800`), and the joint draws the other mesh, black and translucent, so
/// that only the spare shows. Once they swap, the joint takes the other mesh.
/// The model decides, not `Id`: as a zombie he keeps his arms.
pub fn joint_meshes(character: &Character, joints: usize, meshes: usize) -> Option<Vec<usize>> {
    if ADA_IDS.contains(&character.model) {
        if !character.arms.swapped || joints <= ADA_HAND || meshes <= ADA_SPARE_HAND {
            return None;
        }
        let mut joint_meshes: Vec<usize> = (0..joints).collect();
        joint_meshes[ADA_HAND] = ADA_SPARE_HAND;
        return Some(joint_meshes);
    }
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
        } else {
            enemy_spl_root_ck_sys(game, room, slot, true);
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
                    // The load's place for the player is still pending here.
                    true,
                );
            }
            character.zombie = Some(zombie);
        }
        game.re2.characters.0[slot] = Some(character);
    }
    game.re2.set_enemies_stopped(was_stopped);
}

/// The part a hung part of the body in `slot` hangs on, another work's, as
/// the frame draws it: a zombie's `drawn`, a scene character's `Workm`
/// (`Work::parts`, latched after its move and the pushes). `None` for its
/// own part, which [`crate::zombie2::Zombie::draw_hung`] takes from its own.
fn hung_on(
    characters: &[Option<Box<Character>>],
    slot: usize,
    hung: crate::zombie2::Hung,
) -> Option<player2::Matrix> {
    if usize::from(hung.on_slot) == slot {
        return None;
    }
    let other = characters.get(usize::from(hung.on_slot))?.as_deref()?;
    let part = usize::from(hung.on_part);
    match &other.zombie {
        Some(z) => z.drawn.get(part).copied().flatten(),
        None => other.work.parts.get(part).copied().flatten(),
    }
}

/// Ben's ids, whose `Spl_Sp_Move` is his death in room 301: 0x44 and the
/// other Ben, 0x46, which `_Spl`'s tables run alike.
const BEN: u8 = 0x44;
const BEN_ALT: u8 = 0x46;
/// Irons's id, whose `Spl_Sp_Move` is his death in room 300.
const IRONS: u8 = 0x40;

/// `Spl_Sp_Move` (`0x4f6e50`, `_Spl`'s `Routine_0` 8): a character's own
/// scene, from the table at `0x53d6e8` by `Id` and `Routine_1`. Only `Routine_1`
/// 0 has entries: 0x40's (Irons's death, `Spl_sp40_00`) and Ben's, 0x44's and
/// 0x46's (`Spl_sp44_00`). Anyone else stands still.
fn spl_sp_move(game: &mut GameState, character: &mut Character, tick: &Tick<'_>, slot: usize) {
    if character.work.routine[1] != 0 || !matches!(character.id, IRONS | BEN | BEN_ALT) {
        return;
    }
    let irons = character.id == IRONS;
    let effects = player2::spl_special(&mut character.work, &mut character.body, tick, |mover| {
        if irons {
            spl_sp40_00(mover)
        } else {
            spl_sp44_00(mover)
        }
    });
    let anchor = crate::esp2::Anchor {
        body: crate::esp2::Body::Enemy(slot as u8),
        part: 0,
    };
    for (kind, row, size, dir, matrix, point) in effects.calls {
        game.re2
            .effects
            .call_on_part(kind, row, size, dir, anchor, matrix, point);
    }
    if let Some(embryo) = game.re2.characters.0.get_mut(1).and_then(Option::as_mut) {
        let step = &mut embryo.work.routine[2];
        if effects.burst {
            *step = 5;
        }
        *step = step.wrapping_add(effects.embryo_steps);
    }
}

/// What a death in `Spl_Sp_Move` did this tick: the `Esp_call`s on the
/// body's part 0 (kind, row, size, direction, the part's `Workm` as the last
/// draw left it, and the point on it), whether it let the embryo out
/// (`enemy_work[1]`'s `Routine_2` 5), and how many times Irons's step 3
/// moved that `Routine_2` on by one.
#[derive(Debug, Default)]
struct SpDeath {
    calls: Vec<(u8, u8, u16, i16, player2::Matrix, [i16; 3])>,
    burst: bool,
    embryo_steps: u8,
}

/// `Spl_sp44_00` (`0x4f7690`): Ben's death, which the G embryo in him starts
/// (`FUN_00474b80` sets his routines to 8, 0, 0). See [`spl_sp_death`];
/// his step 3 does nothing.
fn spl_sp44_00(mover: &mut player2::Mover<'_, '_>) -> SpDeath {
    spl_sp_death(mover, false)
}

/// `Spl_sp40_00` (`0x4f6e70`): Irons's death in room 300, which the G embryo
/// in him starts as Ben's does. Steps 0 to 2 are Ben's (see
/// [`spl_sp_death`]); step 2 falls into 3, which counts `Timer0` (`+0x158`)
/// down from 0x96: at 0x82 it moves the embryo's `Routine_2` on one, from
/// 0x81 to 0x61 it swells and shrinks part 0, from 0x8b to 0x74 it bleeds
/// kind 0 row 2 (0xfa0) on part 0 every eighth count, and on the tick it
/// counts from 0 it goes on to step 4 (nothing) and moves the embryo's
/// `Routine_2` on again.
fn spl_sp40_00(mover: &mut player2::Mover<'_, '_>) -> SpDeath {
    spl_sp_death(mover, true)
}

/// Steps 0 to 3 of `Spl_sp44_00` and `Spl_sp40_00`, alike but for what they
/// draw and Irons's step 3. Clip 0 plays once, through `Hokan_flg` 0xf, with
/// blood on part 0: two drops at the start, two every fourth frame below
/// 0x2a further down the body each time (`Timer0`, `+0x158`), and kind 0
/// row 2 every fourth frame from 0x24 to 0x36; on frame 0x46 the embryo
/// comes out (`enemy_work[1]`'s `Routine_2` 5). What the death does to the
/// parts as drawn isn't drawn: part 0's and its twin's swell (kept as
/// weights), the part step 0 shows (Ben's 15 at `+0xa14`, Irons's 16 at
/// `+0xac0`), Irons's `Be_flg` 0x8000 on parts 9 to 11 (`Joint_trans2`'s
/// `ko_joint_trans2` for them; nothing reads their `Workm`), and the
/// textures' moves on frames 0x28 and 0x32.
fn spl_sp_death(mover: &mut player2::Mover<'_, '_>, irons: bool) -> SpDeath {
    let mut out = SpDeath::default();
    let dir = mover.player.angle as i16;
    let part0 = mover.work.workm(0);
    let call = |out: &mut SpDeath, row: u8, size: u16, dir: i16, point: [i16; 3]| {
        if let Some(matrix) = part0 {
            out.calls.push((0, row, size, dir, matrix, point));
        }
    };
    let work = &mut *mover.work;
    let step = work.routine[2];
    if step == 0 {
        // `+0x14c`'s dword: clip 0 from its start, `Hokan_flg` 0xf, of the
        // room's bank (`pRoom_kan_t_ptr`, `Joint_move`'s).
        work.bank = player2::Bank::Room;
        work.move_no = 0;
        work.move_cnt = 0;
        work.hokan = 0xF;
        work.routine[2] = 1;
        call(&mut out, 0, 0x1770, dir, [200, -400, -200]);
        call(
            &mut out,
            0,
            0x1770,
            dir.wrapping_add(0x800),
            [0x96, -0x15E, -200],
        );
        work.timers[0] = 0;
        work.timers[2] = 5;
        work.sp_timer3 = 0;
        work.sp_weight = 0x200;
    }
    match work.routine[2] {
        1 => {
            let frame = work.move_cnt;
            if frame < 0x2A && frame & 3 == 0 {
                let count = work.timers[0] as i16;
                call(
                    &mut out,
                    1,
                    0xBB8,
                    dir,
                    [
                        200,
                        count.wrapping_mul(0x1E).wrapping_sub(400),
                        count.wrapping_mul(0xF).wrapping_sub(200),
                    ],
                );
                call(
                    &mut out,
                    0,
                    0x1388,
                    dir.wrapping_add(0x800),
                    [
                        100,
                        count.wrapping_mul(5).wrapping_sub(0x46).wrapping_mul(5),
                        count.wrapping_mul(0xF).wrapping_sub(200),
                    ],
                );
                work.timers[0] = count.wrapping_add(1) as u16;
            }
            // Below 0x2a part 0 shrinks by 0x50 while `Timer2` (a signed
            // byte, 5 down to 0 and round) is under 4, else swells by 200;
            // past 0x32 it swells by 200 to 0x1000, which sets `Timer3`.
            if frame < 0x2A {
                let count = work.timers[2] as u8 as i8;
                work.sp_weight = work
                    .sp_weight
                    .wrapping_add(if count < 4 { -0x50 } else { 200 });
                work.timers[2] = if count == 0 {
                    5
                } else {
                    u16::from(count.wrapping_sub(1) as u8)
                };
            }
            if frame > 0x32 && work.sp_timer3 == 0 {
                work.sp_weight = work.sp_weight.wrapping_add(200);
                if work.sp_weight > 0x1000 {
                    work.sp_weight = 0x1000;
                    work.sp_timer3 = 1;
                }
            }
            if frame > 0x23 && frame < 0x37 && frame & 3 == 0 {
                call(&mut out, 2, 0x1770, dir, [0; 3]);
            }
            if frame == 0x46 {
                out.burst = true;
            }
            if mover.joint_move_late(0x100) {
                mover.work.routine[2] += 1;
            }
        }
        2 => {
            work.timers[0] = 0x96;
            work.routine[2] = 3;
            if irons {
                spl_sp40_step_3(work, &mut out, |out| call(out, 2, 0xFA0, dir, [0; 3]));
            }
        }
        3 if irons => {
            spl_sp40_step_3(work, &mut out, |out| call(out, 2, 0xFA0, dir, [0; 3]));
        }
        _ => {}
    }
    out
}

/// `Spl_sp40_00`'s step 3, by `Timer0` (`+0x158`, signed): see
/// [`spl_sp40_00`]. `bleed` is its `Esp_call(0x20fa0)` on part 0.
fn spl_sp40_step_3(work: &mut player2::Work, out: &mut SpDeath, bleed: impl Fn(&mut SpDeath)) {
    let count = work.timers[0] as i16;
    if count == 0x82 {
        out.embryo_steps = out.embryo_steps.wrapping_add(1);
    }
    let swell = match count {
        0x78..=0x81 => 100,
        0x72..=0x76 => -200,
        0x67..=0x70 => 0x50,
        0x61..=0x65 => -0xA0,
        _ => 0,
    };
    work.sp_weight = work.sp_weight.wrapping_add(swell);
    if count > 0x73 && count < 0x8C && count & 7 == 0 {
        bleed(out);
    }
    work.timers[0] = count.wrapping_sub(1) as u16;
    if count == 0 {
        work.routine[2] = 4;
        out.embryo_steps = out.embryo_steps.wrapping_add(1);
    }
}

/// `Game_loop` and `Em_init_move` start at `splayer_work`.
fn run_order() -> impl Iterator<Item = usize> {
    std::iter::once(SPLAYER_SLOT).chain(0..SPLAYER_SLOT)
}

/// `_Spl`'s routine: `Spl_Init` for one no script has given a routine,
/// `Spl_Auto` for Sherry or Ada as the partner (`partner2`), and
/// `Spl_Scenario`. Anyone else's `Spl_Auto` isn't ported: they stand still.
fn spl(game: &mut GameState, character: &mut Character, tick: &Tick<'_>) -> Option<u8> {
    let sherry = matches!(character.id, 0x45 | 0x4F);
    let ada = ADA_IDS.contains(&character.id);
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
        if ada {
            spl_in41(character, game.re2.flag(3, ADA_PART_16_FLAG));
        }
        let partner = partner && (sherry || ada) && character.partner.is_some();
        player2::spl_init(
            &mut character.work,
            &mut character.body,
            tick,
            character.kind,
            partner,
        );
        // `Spl_in41` and `Spl_in45` (from `Spl_Init`'s table) send the
        // partner to `Spl_Auto` whatever 0x4000 chose.
        if (ada || sherry) && game.re2.partner_bit() {
            character.work.routine = [1, 0, 0, 0];
        }
        if character.id == MARVIN {
            spl_in4a(character);
        }
        if character.id == DESK_BODY {
            spl_in4b(character);
        }
        // `Spl_in41` ends as `Spl_in45` does.
        if (sherry || ada)
            && let Some(words) = character.partner.as_deref_mut()
        {
            words.spl_in45(game.re2.partner_bit(), game.re2.partner_life);
        }
        None
    } else if player2::in_scenario(&character.work) {
        player2::player_move(
            &mut character.work,
            &mut character.body,
            tick,
            &mut game.re2.rnd,
        )
    } else if player2::in_auto(&character.work)
        && (sherry || ada)
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
            player_noise: game.re2.player.noise,
            player_spd_x: game.re2.player.spd[0],
            poisoned: game.re2.partner_poisoned,
        };
        let kind = &mut character.kind;
        player2::spl_auto(&mut character.work, &mut character.body, tick, |mover| {
            crate::partner2::spl_auto(mover, words, kind, &mut world, ada);
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
fn zombie_root_target(
    game: &GameState,
    room: &RoomState,
    character: &Character,
    pending_place: bool,
) -> [i32; 3] {
    let (player, _) = player_at(game, pending_place);
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

/// `Root_ck`'s count of its turn: 7 again after 0.
pub(crate) fn count_root_turn(turn: &mut u8) {
    if *turn & 0x7F == 0 {
        *turn |= 7;
    }
    *turn = turn.wrapping_sub(1);
}

/// `_Spl`'s `Spl_Root_ck_sys` for a scene character in an enemy slot, which
/// runs before the stop test as the partner's does.
fn enemy_spl_root_ck_sys(game: &mut GameState, room: &RoomState, slot: usize, loading: bool) {
    let Some(mut character) = game.re2.characters.0.get_mut(slot).and_then(Option::take) else {
        return;
    };
    if moves(&character) && character.zombie.is_none() {
        root_ck_sys(game, room, &mut character, loading);
    }
    game.re2.characters.0[slot] = Some(character);
}

/// The player's position and facing as an enemy's pass sees them. In the
/// room's load and its last pass (`pending_place`) the player is already where
/// the door, a script's `pos_set` or a driver's warp put them, which the port
/// applies only once the scripts' pass ends.
fn player_at(game: &GameState, pending_place: bool) -> ([i32; 3], u16) {
    let (player, player_angle) = game
        .entities
        .first()
        .map_or(([0; 3], 0), |p| (p.pos, p.angle));
    if !pending_place {
        return (player, player_angle);
    }
    let pending = &game.re2.pending_player;
    (
        pending.pos.unwrap_or(player),
        pending.angle.unwrap_or(player_angle),
    )
}

/// `loading`: the room's load (see [`player_at`]).
fn root_ck_sys(game: &GameState, room: &RoomState, character: &mut Character, loading: bool) {
    let Some(partner) = character.partner.as_deref_mut() else {
        // Anyone else's `Type` has `0x1000`, so only its turn counts.
        count_root_turn(&mut character.work.root_turn);
        return;
    };
    let sca = room
        .sca
        .as_ref()
        .map_or(&[][..], |sca| sca.records.as_slice());
    let (player, player_angle) = player_at(game, loading);
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

/// What `_Spl` does after a character's routine, whoever it is: the
/// ripples of its footfalls in water (`Snd_se_walk`), its push out of the
/// bodies ([`spl_oba_ck_em`]) and the scene's blood.
fn spl_after_routine(game: &mut GameState, character: &mut Character, old: [i32; 3]) {
    let calls = std::mem::take(&mut character.work.effects);
    esp_calls(game, calls);
    spl_oba_ck_em(game, character, old);
    sce_blood(game, character);
}

/// `_Spl`'s `Oba_ck_em`, after the routine of every character it runs, the
/// partner or not: pushed out of the player's box as the player's last
/// `Oba_ck_em2` left it, then out of the bodies in the enemy slots. It leaves
/// `At_em_flg` alone. As `Game_loop` runs the enemy slots before
/// `Player_move`, a character that lands on the player is pushed off it, and
/// the player's own push then finds no body there: room 404's thrown Annette
/// (`main_18`) lands beside Ada.
fn spl_oba_ck_em(game: &GameState, character: &mut Character, old: [i32; 3]) {
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

/// `_Spl`'s `Sca_ck_em` (`0x4CC730`), after its routine. A `Be_flg` with 8
/// skips the walls, as a zombie's does: room 301's `main_00` sets it on
/// Ben, placed in his bunk, who stays there (`leon_a_219_301`). The mirror
/// takes the place either way.
fn spl_sca_ck_em(room: &RoomState, character: &mut Character) {
    if let Some(partner) = character.partner.as_deref_mut() {
        partner.sca_hit = false;
    }
    let body = &mut character.body;
    let sca = room
        .sca
        .as_ref()
        .filter(|_| character.be_flg & crate::zombie2::BE_NO_SCA == 0);
    let Some(sca) = sca else {
        if room.sca.is_some() {
            body.collision_mirror = [body.pos[0], body.pos[2]];
        }
        return;
    };
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
    // memory held: the follow point's kind is taken as 0. Anyone else counts
    // only the turn, in each of the two calls: room 202's Marvin, whom
    // `Sce_em_set` starts at 2 in slot 0, ends the load at 0.
    root_ck_sys(game, room, character, true);
    let arrived = spl(game, character, tick);
    spl_sca_ck_em(room, character);
    let (start, flags) = character.start;
    if start == 0 {
        return arrived;
    }
    player2::spl_start(&mut character.work, start, flags);
    if character.partner.is_none() {
        count_root_turn(&mut character.work.root_turn);
    }
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
    character
        .work
        .apply_to_pose(&character.body.anim, &mut keyframe);
    let [pitch, roll] = character
        .zombie
        .as_ref()
        .map_or([0; 2], |zombie| zombie.tilt);
    let on_part = character
        .work
        .super_part
        .and_then(|part| game.joint_worlds[0].get(usize::from(part)));
    if let Some(part) = on_part {
        // `pSuper` is a part of the player's: `Cdir` turns on it, and `Pos`
        // is already the place in the world ([`on_player_parts`]).
        let [x, z] = character.work.tilt;
        let local = crate::anim::Mat4x3 {
            r: crate::anim::rotation_matrix(
                i32::from(x),
                i32::from(character.body.angle),
                i32::from(z),
            ),
            t: [0; 3],
        };
        let mut matrix = crate::anim::compose(part, &local);
        matrix.t = character.body.pos;
        let mut local = crate::anim::joint_local_transforms(&model.skeleton, &keyframe);
        character.work.pose_locals(&mut local);
        let joints = crate::anim::joint_matrices_from_locals(&model.skeleton, &local, &matrix, &[]);
        return Some((joints, keyframe));
    }
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

/// `Joint_trans2`'s `CompM(pSuper, ...)` for a body hung on a part of the
/// player's (`Be_flg`'s `0x3000000`): `M.t` is `Pos`, so the body's place
/// becomes the one in the world, on the player's pose this tick. The local
/// one is kept for `Em_29` to put back.
fn on_player_parts(game: &mut GameState) {
    let parts = &game.re2.player.parts;
    for character in game.re2.characters.0.iter_mut().flatten() {
        if character.be_flg & 1 == 0 || character.be_flg & 0x300_0000 != 0x300_0000 {
            continue;
        }
        let Some(part) = character.work.super_part else {
            continue;
        };
        let Some(Some(matrix)) = parts.get(usize::from(part)) else {
            continue;
        };
        let on_part = |local: [i32; 3]| -> [i32; 3] {
            std::array::from_fn(|axis| {
                let turned: i32 = (0..3)
                    .map(|index| i32::from(matrix.m[axis][index]) * local[index])
                    .sum();
                matrix.t[axis] + (turned >> 12)
            })
        };
        let local = character.body.pos;
        character.work.super_local = Some(local);
        let pos = on_part(local);
        character.body.pos = pos;
        // Its part 0 on that `M`, where the player's head turns to on the
        // next tick.
        let turn = player2::mul_matrix(
            &matrix.m,
            &character.work.body_rotation(character.body.angle),
        );
        if let Some(zombie) = character.zombie.as_mut() {
            zombie.roach_head = zombie.roach_head.map(|offset| {
                std::array::from_fn(|axis| {
                    let turned: i32 = (0..3)
                        .map(|index| i32::from(turn[axis][index]) * offset[index])
                        .sum();
                    pos[axis] + (turned >> 12)
                })
            });
        }
    }
}

/// [`keep_joints`] with each character's model from `model`: the slots of
/// the characters the draw poses (in play, and not a zombie waiting for the
/// room's scripts), the others empty.
fn keep_joints_with(
    game: &mut GameState,
    room: &RoomState,
    mut model: impl FnMut(&Character) -> Option<std::sync::Arc<Emd>>,
) {
    on_player_parts(game);
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

    /// Room 304's `sce_espr_on` on the player's part 8, at a vector off
    /// it: the effect starts on the part's `Workm` as the last tick left
    /// it (the neck's turn and all), the vector turned with the part.
    #[test]
    fn a_player_parts_effect_turns_its_vector_with_the_parts_workm() {
        let turned = player2::Matrix {
            m: player2::rot_matrix([0, 1024, 0]),
            t: [100, -1500, 200],
        };
        let mut work = player2::Work::default();
        assert_eq!(work.workm(8), None, "no tick has left one");
        work.parts = vec![None; 9];
        work.parts[8] = Some(turned);
        let world = work.workm(8).unwrap();
        assert_eq!(world, turned);
        assert_eq!(place_on(&world, [0, 0, 100]), [200, -1500, 200]);
        assert_eq!(place_on(&world, [100, 0, 0]), [100, -1500, 100]);
    }

    /// `pl_mv05_w1_t2` calls `Shoot_camera` on the shot's first tick, before
    /// `Weapon_at_ck`: a handgun fired past the camera draws the spark's
    /// three `Rnd`s, and `hard_mode`'s roll for the body it hit takes the two
    /// after them (#500). The roll reads `hard_mode` and the room, and in
    /// room 103 it is drawn and changes nothing.
    #[test]
    fn a_shots_draws_are_the_cameras_and_then_the_hard_mode_hits() {
        use crate::zombie2::Rnd;
        // `weapon2`'s own test of `Shoot_camera`: Leon's hand along -X, and a
        // camera on its line.
        let line = crate::weapon2::ShotLine {
            from: [18162, -2505, -2675],
            matrix: [[1, -4094, 11], [-4097, -4, 8], [-10, -10, -4094]],
        };
        let cut = crate::state::Cut {
            pos: [15164, -2388, -2682],
            look_at: [22356, -2466, 522],
            ..Default::default()
        };
        let hit = crate::weapon2::Hit {
            slot: 1,
            id: 0x10,
            weapon: 2,
            high: false,
            band: 1,
            region: 2,
            damage: 15,
            stun: 2,
            count: 5,
            flags: 0x12,
        };
        let after = |seed: u16, draws: usize| {
            let mut rnd = Rnd(seed);
            for _ in 0..draws {
                rnd.draw();
            }
            rnd.0
        };
        let roll = |seed: u16| {
            let mut rnd = Rnd(seed);
            let shift = rnd.draw() & 3;
            (rnd.draw() >> shift) & 0xF == 0
        };
        // A roll that comes up after the camera's three draws, and wouldn't
        // have before them.
        let seed = (0..=u16::MAX)
            .find(|&seed| !roll(seed) && roll(after(seed, 3)))
            .unwrap();
        let shoot = |room: &str, hard: bool, camera: bool| {
            let mut game = GameState::default();
            game.id = RoomId::parse(room).unwrap();
            game.re2.hard_mode = hard;
            game.equipped_weapon_id = 2;
            game.re2.rnd = Rnd(seed);
            game.re2.player.shot_line = camera.then_some(line);
            game.re2.player.hit = Some(hit);
            shot_draws(&mut game, Some(&cut));
            let hit = game.re2.player.hit.unwrap();
            (hit.weapon, hit.damage, game.re2.rnd.0)
        };
        assert_eq!(shoot("1180", true, true), (6, 900, after(seed, 5)));
        assert_eq!(shoot("1180", true, false), (2, 15, after(seed, 2)));
        assert_eq!(shoot("1180", false, true), (2, 15, after(seed, 3)));
        assert_eq!(shoot("1030", true, true), (2, 15, after(seed, 5)));
        assert_eq!(shoot("2030", true, true), (6, 900, after(seed, 5)));
    }

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
            marni: None,
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
            marni: None,
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

    /// The load's last pass: a door, a script's `pos_set` or a driver's warp
    /// has placed the player, which the port applies only once the scripts'
    /// pass ends. A zombie whose `Root_ck` looks in that pass routes toward
    /// that place, as the original's does (`leon_a_20a_20c`'s zombie 0 starts
    /// with the warp's place as its `Dest`), not toward the last tick's.
    #[test]
    fn the_loads_last_pass_routes_a_zombie_toward_the_pending_place() {
        let model = std::sync::Arc::new(posable_model());
        let pack = Pack::from_bytes(crate::pack::PackWriter::new().to_bytes().unwrap()).unwrap();
        let mut zombie = crate::zombie2::Zombie::default();
        zombie.set_sight_turn(1);
        let mut character = sherry(0);
        character.id = 0x10;
        character.model = 0x10;
        character.partner = None;
        character.body.pos = [1000, 0, 2000];
        character.work.routine = [1, 0, 0, 0];
        character.zombie = Some(zombie);
        let mut game = GameState::default();
        game.re2.characters.0 = vec![Some(Box::new(character))];
        game.entities[0].pos = [-20000, 0, -20000];
        game.re2.pending_player.pos = Some([3000, 0, 4000]);
        let room = RoomState {
            guides: vec![crate::state::Guide {
                x0: 0,
                z0: 0,
                x1: 5000,
                z1: 5000,
                links: 0,
            }],
            ..RoomState::default()
        };
        run_load_stop_with(&mut game, &room, &pack, None, |_| Some(model.clone()));
        let character = game.re2.characters.0[0].as_deref().unwrap();
        assert_eq!(character.work.dest, [3000, 4000]);
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

        game.re2_no_enemies = Some(crate::game::Re2NoEnemies {
            keep: vec![(Some(0x101), 0), (Some(0x102), 2)],
            room: None,
        });
        game.apply_re2_no_enemies();
        assert_eq!(
            flags(&game),
            [Some(3), None, Some(2)],
            "slot 0 is kept in room 101 only"
        );

        let reset = |game: &mut GameState| {
            for character in game.re2.characters.0.iter_mut().flatten() {
                character.be_flg = 1;
            }
        };
        game.id = RoomId::parse("1020").unwrap();
        reset(&mut game);
        game.apply_re2_no_enemies();
        assert_eq!(flags(&game), [Some(0), None, Some(1)]);

        // `--keep-enemy 2`: slot 2 in every room.
        game.re2_no_enemies = Some(crate::game::Re2NoEnemies {
            keep: vec![(None, 2)],
            room: None,
        });
        reset(&mut game);
        game.apply_re2_no_enemies();
        assert_eq!(flags(&game), [Some(0), None, Some(1)], "kept in room 102");
        game.id = RoomId::parse("2030").unwrap();
        reset(&mut game);
        game.apply_re2_no_enemies();
        assert_eq!(flags(&game), [Some(0), None, Some(1)], "and in room 203");

        // `--no-enemies-room 0`: rooms 100, 200 and so on, and no other.
        game.re2_no_enemies = Some(crate::game::Re2NoEnemies {
            keep: Vec::new(),
            room: Some(0),
        });
        reset(&mut game);
        game.apply_re2_no_enemies();
        assert_eq!(
            flags(&game),
            [Some(1), None, Some(1)],
            "room 203 is left alone"
        );
        game.id = RoomId::parse("2000").unwrap();
        game.apply_re2_no_enemies();
        assert_eq!(
            flags(&game),
            [Some(0), None, Some(0)],
            "room 200 is cleared"
        );

        // The partner isn't in `enemy_work`: `remove_enemies` leaves it.
        let splayer = crate::scd2::host::SPLAYER_SLOT;
        let mut partner = game.re2.characters.0[0].clone();
        partner.as_mut().unwrap().be_flg = 1;
        game.re2.characters.0.resize(splayer + 1, None);
        game.re2.characters.0[splayer] = partner;
        game.apply_re2_no_enemies();
        assert_eq!(
            game.re2.characters.0[splayer].as_ref().map(|c| c.be_flg),
            Some(1)
        );
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
            game.re2.pl_id = player_flag;
            game.re2.characters.0 = vec![Some(Box::new(kendo.clone()))];
            let mut player = crate::player::spawn(room, &RoomState::default());
            player.pos = [-850, 0, 0];
            oba_ck_em2(&mut game, &mut player, [-850, 0, 0]);
            assert_eq!(player.pos, [x, 0, 0], "player {player_flag}");
            assert_eq!(game.re2.player.at_em_flg, u32::from(player_flag == 0));
        }
    }

    /// Each kind's `+0x1EE` as its init writes it (#974): one row per
    /// writer in the decompile, by the kind `Em_move_tbl_set` gives it.
    #[test]
    fn each_kinds_eff_at_r_is_its_inits() {
        use crate::zombie2 as z;
        let kinds = [
            (0x10, 500),
            (0x1F, 500),
            (z::DOG_ID, 600),
            (z::CROW_ID, 300),
            (z::ALLIGATOR_ID, 1100),
            (z::SPIDER_ID, 800),
            (z::YOUNG_ID, 20),
            (z::EMBRYO_ID, 300),
            (z::ADULT_ID, 800),
            (z::ROACH_ID, 800),
            (z::MRX_ID, 800),
            (z::EM2D_ID, 800),
            (z::IVY_ID, 800),
            (z::POISON_IVY_ID, 800),
            (z::VINE_ID, 1200),
            (z::G1_ID, 500),
            (z::G2_ID, 800),
            (z::G3_ID, 900),
            (z::MOTH_ID, 900),
            (z::LARVA_ID, 200),
        ];
        for (id, widen) in kinds {
            assert_eq!(eff_at_r(id, None), widen, "kind {id:#x}");
        }
    }

    #[test]
    fn a_grenade_meets_a_floor_licker_by_its_wider_blast_box() {
        // Room 110 (`claire_a_110_10a`, tick 1287): the second round, at
        // (-25203, -8682, 9875), meets the licker coming on. `Em22_Atd_set_1`
        // widens a blast's box by 700 / 4 for it (`+0x1EE`), not by its head
        // box's `At_w` (0x180), which falls short this tick. On a wall
        // `Em22_Atd_set_2` makes it 0x1E4.
        let room = RoomId::parse("1100").unwrap();
        let mut body = crate::player::spawn(room, &RoomState::default());
        body.pos = [-25866, -7200, 10158];
        body.angle = 1904;
        let mut zombie = crate::zombie2::Zombie::default();
        zombie.life = 41;
        zombie.atd = crate::zombie2::Atd {
            ofs_x: 0x2A0,
            ofs_y: 0,
            at_w: 0x250,
            at_h: 600,
            w: 1000,
            at: [-26600, -7200, 10212],
        };
        let mut licker = crate::zombie2::licker::Licker::default();
        licker.head = crate::zombie2::Atd {
            ofs_x: -0x60,
            ofs_y: -0x180,
            at_w: 0x180,
            at_h: 0x180,
            w: 0x180,
            at: [0; 3],
        };
        let floor = Box::new(licker);
        let mut wall = floor.clone();
        wall.back = Some(crate::zombie2::Atd::default());
        for (licker, widen) in [(floor, 700), (wall, 0x1E4)] {
            zombie.licker = Some(licker);
            let character = Character {
                id: 0x22,
                model: 0x22,
                em_set: (0, 0, NO_ENEMY_FLAG),
                arms: Arms::default(),
                be_flg: 1,
                work: player2::Work::default(),
                body: body.clone(),
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
                zombie: Some(zombie.clone()),
            };
            let mut game = GameState::default();
            game.re2.characters.0 = vec![Some(Box::new(character))];
            let targets = targets_of(&game.re2);
            assert_eq!(targets[0].widen, widen);
            let hits = |widen: u16| {
                let mut targets = targets.clone();
                targets[0].widen = widen;
                let blast = crate::weapon2::Blast {
                    at: [-25203, -8682 + 1000, 9875],
                    dir: 3048,
                    word: 0x3_0009,
                };
                let mut rect = [-1400, 0, 350, 250];
                crate::weapon2::blast_ck(&blast, &mut rect, &mut targets)
                    .iter()
                    .any(|effect| matches!(effect, crate::weapon2::BlastEffect::Hit(_)))
            };
            assert!(hits(widen));
            assert!(!hits(0x180));
        }
    }

    #[test]
    fn a_grenade_meets_the_g_adult_by_its_wider_blast_box() {
        // `FUN_004771d0`, the G adult's init, sets `+0x1EE` to 800: a blast's
        // box is 200 wider for it, where arklay's old 500 gave 125. Room
        // 309's second acid round (`claire_a_309_30a`, tick 1528) meets the
        // adult by that margin.
        let room = RoomId::parse("3090").unwrap();
        let mut body = crate::player::spawn(room, &RoomState::default());
        body.pos = [650, 0, 0];
        let mut zombie = crate::zombie2::Zombie::default();
        zombie.life = 479;
        zombie.atd = crate::zombie2::Atd {
            ofs_x: 0,
            ofs_y: -1000,
            at_w: 1000,
            at_h: 1000,
            w: 1000,
            at: [650, 0, 0],
        };
        let character = Character {
            id: crate::zombie2::ADULT_ID,
            model: crate::zombie2::ADULT_ID,
            em_set: (0, 0, NO_ENEMY_FLAG),
            arms: Arms::default(),
            be_flg: 1,
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
            zombie: Some(zombie),
        };
        let mut game = GameState::default();
        game.re2.characters.0 = vec![Some(Box::new(character))];
        let targets = targets_of(&game.re2);
        assert_eq!(targets[0].widen, 800);
        let hits = |widen: u16| {
            let mut targets = targets.clone();
            targets[0].widen = widen;
            let blast = crate::weapon2::Blast {
                at: [0, -1000, 0],
                dir: 0,
                word: 0x3_000B,
            };
            let mut rect = [-1400, 0, 350, 250];
            crate::weapon2::blast_ck(&blast, &mut rect, &mut targets)
                .iter()
                .any(|effect| matches!(effect, crate::weapon2::BlastEffect::Hit(_)))
        };
        assert!(hits(800));
        assert!(!hits(500));
    }

    #[test]
    fn a_crow_flying_low_clears_claires_head_but_not_leons() {
        // Room 109's crow (`claire_a_108_10b`, tick 934): its box (`Ofs_y`
        // -350, `At_w` 350, `At_h` 530) at -3310 high, 697 from the player on
        // the floor. Leon's box (`At_h` 1530) reaches it; Claire's
        // (`Pl_init`'s 1350) doesn't, so she isn't pushed.
        let room = RoomId::parse("1090").unwrap();
        let mut body = crate::player::spawn(room, &RoomState::default());
        body.pos = [-2477, -2960, -12521];
        let mut zombie = crate::zombie2::Zombie::default();
        zombie.atd = crate::zombie2::Atd {
            ofs_x: 0,
            ofs_y: -350,
            at_w: 350,
            at_h: 530,
            w: 350,
            at: [-2477, -3310, -12521],
        };
        let crow = Character {
            id: 0x21,
            model: 0x21,
            em_set: (0, 0, NO_ENEMY_FLAG),
            arms: Arms::default(),
            be_flg: 1,
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
            zombie: Some(zombie),
        };
        // Each 110 inside its reach (`At_w` 450 or 350, and the crow's 350).
        for (player_flag, pushed) in [(0, true), (1, false)] {
            let mut game = GameState::default();
            game.id = room;
            game.id.player_flag = player_flag;
            game.re2.pl_id = player_flag;
            game.re2.characters.0 = vec![Some(Box::new(crow.clone()))];
            let mut player = crate::player::spawn(room, &RoomState::default());
            let at = [
                -2477 + 690 - 450 + i32::from(player_width(player_flag)),
                0,
                -12521,
            ];
            player.pos = at;
            oba_ck_em2(&mut game, &mut player, at);
            assert_eq!(player.pos != at, pushed, "player {player_flag}");
            assert_eq!(game.re2.player.at_em_flg, u32::from(pushed));
            assert_eq!(
                game.re2
                    .player
                    .atd
                    .map(|atd| (atd.ofs_y, atd.at_h, atd.at[1])),
                Some(if pushed {
                    (-0x5FA, 0x5FA, -0x5FA)
                } else {
                    (-0x546, 0x546, -0x546)
                })
            );
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
        game.re2.pl_id = 1;
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
            partner: Some(Box::new(crate::partner2::Partner::placed(0, 0))),
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

    /// Ben in his death (`Spl_sp44_00`, `Routine_0` 8) on `frame` of clip 0
    /// at `step`, with his part 0 drawn at (100, 200, 300) unturned.
    fn ben_dying(step: u8, frame: u8) -> Character {
        let mut ben = sherry(0);
        ben.id = BEN;
        ben.model = BEN;
        ben.partner = None;
        ben.work.routine = [player2::MODE_SPECIAL, 0, step, 0];
        ben.work.move_cnt = frame;
        ben.body.angle = 0x400;
        ben.work.parts = vec![Some(player2::Matrix {
            m: [[0x1000, 0, 0], [0, 0x1000, 0], [0, 0, 0x1000]],
            t: [100, 200, 300],
        })];
        ben
    }

    /// `Spl_sp44_00`'s start: clip 0 from its start through `Hokan_flg` 0xf,
    /// two drops of blood (kind 0, row 0, 0x1770) on part 0, the second
    /// turned half round, and then the first frame's two (rows 1 and 0)
    /// with `Timer0` 0.
    #[test]
    fn bens_death_starts_clip_0_with_his_blood() {
        let mut ben = ben_dying(0, 9);
        let tick = spl_tick(&RoomState::default(), &Default::default());
        let out = player2::spl_special(&mut ben.work, &mut ben.body, &tick, spl_sp44_00);
        let calls: Vec<_> = out
            .calls
            .iter()
            .map(|&(kind, row, size, dir, _, point)| (kind, row, size, dir, point))
            .collect();
        assert_eq!(
            calls,
            [
                (0, 0, 0x1770, 0x400, [200, -400, -200]),
                (0, 0, 0x1770, 0xC00, [0x96, -0x15E, -200]),
                (0, 1, 0xBB8, 0x400, [200, -400, -200]),
                (0, 0, 0x1388, 0xC00, [100, -0x15E, -200]),
            ]
        );
        assert_eq!((ben.work.move_no, ben.work.hokan), (0, 0xF));
        assert_eq!(ben.work.timers[0], 1);
        assert!(!out.burst);
    }

    /// On clip 0's frame 0x46 the embryo comes out of him: `enemy_work[1]`'s
    /// `Routine_2` 5. Off the fourth frames he doesn't bleed, and from 0x24 to
    /// 0x36 every fourth frame bleeds row 2 at part 0 itself.
    #[test]
    fn bens_death_lets_the_embryo_out_on_frame_0x46() {
        let mut game = GameState::default();
        game.re2.characters.0 = vec![None; 3];
        let mut embryo = sherry(0);
        embryo.id = crate::zombie2::EMBRYO_ID;
        embryo.work.routine = [1, 3, 2, 0];
        game.re2.characters.0[1] = Some(Box::new(embryo));
        let tick = spl_tick(&RoomState::default(), &Default::default());
        let mut ben = ben_dying(1, 0x45);
        spl_sp_move(&mut game, &mut ben, &tick, 0);
        let routine = |game: &GameState| game.re2.characters.0[1].as_ref().unwrap().work.routine[2];
        assert_eq!(routine(&game), 2);
        let mut ben = ben_dying(1, 0x46);
        spl_sp_move(&mut game, &mut ben, &tick, 0);
        assert_eq!(routine(&game), 5);
        let mut ben = ben_dying(1, 0x28);
        let out = player2::spl_special(&mut ben.work, &mut ben.body, &tick, spl_sp44_00);
        let rows: Vec<u8> = out.calls.iter().map(|call| call.1).collect();
        assert_eq!(rows, [1, 0, 2]);
        let mut ben = ben_dying(1, 0x29);
        let out = player2::spl_special(&mut ben.work, &mut ben.body, &tick, spl_sp44_00);
        assert!(out.calls.is_empty());
    }

    /// `Spl_sp44_00`'s timers: step 0 sets `Timer2` 5 and `Timer3` 0 and
    /// part 0's weight 0x200 on the room's bank; below frame 0x2a `Timer2`
    /// counts down as a byte and round from 0 to 5, part 0 shrinking by 0x50
    /// while it's under 4 and swelling by 200 from 4; past frame 0x32 it
    /// swells by 200 until it passes 0x1000, which sets `Timer3`. Step 2
    /// sets `Timer0` 0x96 and goes on to 3.
    #[test]
    fn bens_death_counts_its_timers_and_part_0s_weight() {
        let tick = spl_tick(&RoomState::default(), &Default::default());
        let mut ben = ben_dying(0, 0);
        ben.work.bank = player2::Bank::Weapon;
        player2::spl_special(&mut ben.work, &mut ben.body, &tick, spl_sp44_00);
        assert_eq!(ben.work.bank, player2::Bank::Room);
        // Step 0 falls into 1 on frame 0: `Timer2` 5 (swell), then 4.
        assert_eq!((ben.work.timers[2], ben.work.sp_timer3), (4, 0));
        assert_eq!(ben.work.sp_weight, 0x200 + 200);
        let mut ben = ben_dying(1, 1);
        ben.work.timers[2] = 3;
        ben.work.sp_weight = 0x400;
        player2::spl_special(&mut ben.work, &mut ben.body, &tick, spl_sp44_00);
        assert_eq!((ben.work.timers[2], ben.work.sp_weight), (2, 0x400 - 0x50));
        let mut ben = ben_dying(1, 1);
        ben.work.timers[2] = 0;
        player2::spl_special(&mut ben.work, &mut ben.body, &tick, spl_sp44_00);
        assert_eq!(ben.work.timers[2], 5);
        for (frame, weight, timer3) in [(0x32, 0xF80, 0), (0x33, 0xF00, 0), (0x33, 0xF80, 1)] {
            let mut ben = ben_dying(1, frame);
            ben.work.sp_weight = weight;
            player2::spl_special(&mut ben.work, &mut ben.body, &tick, spl_sp44_00);
            let swollen = if frame == 0x32 {
                weight
            } else {
                (weight + 200).min(0x1000)
            };
            assert_eq!(
                (ben.work.sp_weight, ben.work.sp_timer3),
                (swollen, timer3),
                "{frame:#x} {weight:#x}"
            );
        }
        let mut ben = ben_dying(2, 0);
        player2::spl_special(&mut ben.work, &mut ben.body, &tick, spl_sp44_00);
        assert_eq!((ben.work.timers[0], ben.work.routine[2]), (0x96, 3));
    }

    /// `Spl_Sp_Move`'s table has Ben's death for ids 0x44 and 0x46, on
    /// `Routine_1` 0 only; anyone else in it stands still.
    #[test]
    fn spl_sp_move_runs_ben_s_death_for_both_his_ids() {
        let tick = spl_tick(&RoomState::default(), &Default::default());
        for (id, one, runs) in [
            (BEN, 0, true),
            (BEN_ALT, 0, true),
            (BEN, 1, false),
            (IRONS, 0, true),
            (IRONS, 1, false),
            (0x41, 0, false),
        ] {
            let mut game = GameState::default();
            let mut ben = ben_dying(0, 0);
            ben.id = id;
            ben.work.routine[1] = one;
            spl_sp_move(&mut game, &mut ben, &tick, 0);
            assert_eq!(ben.work.routine[2] != 0, runs, "{id:#x} {one}");
        }
    }

    /// Irons in his death (`Spl_sp40_00`) at `step` with `Timer0` `count`,
    /// and the embryo in `enemy_work[1]` at `Routine_2` `embryo`.
    fn irons_dying(step: u8, count: u16, embryo: u8) -> (GameState, Character) {
        let mut game = GameState::default();
        game.re2.characters.0 = vec![None; 3];
        let mut g = sherry(0);
        g.id = crate::zombie2::EMBRYO_ID;
        g.work.routine = [1, 3, embryo, 0];
        game.re2.characters.0[1] = Some(Box::new(g));
        let mut irons = ben_dying(step, 0);
        irons.id = IRONS;
        irons.model = IRONS;
        irons.work.timers[0] = count;
        (game, irons)
    }

    fn embryo_step(game: &GameState) -> u8 {
        game.re2.characters.0[1].as_ref().unwrap().work.routine[2]
    }

    /// Irons's death starts as Ben's does: clip 0 of the room's bank through
    /// `Hokan_flg` 0xf, his blood on part 0, and on frame 0x46 the embryo
    /// out of him (`Routine_2` 5).
    #[test]
    fn irons_s_death_plays_clip_0_and_lets_the_embryo_out() {
        let tick = spl_tick(&RoomState::default(), &Default::default());
        let (mut game, mut irons) = irons_dying(0, 0, 2);
        irons.work.move_cnt = 9;
        irons.work.bank = player2::Bank::Weapon;
        spl_sp_move(&mut game, &mut irons, &tick, 0);
        assert_eq!(irons.work.bank, player2::Bank::Room);
        assert_eq!((irons.work.move_no, irons.work.hokan), (0, 0xF));
        assert_eq!(irons.work.timers[0], 1);
        assert_eq!(embryo_step(&game), 2);
        let (mut game, mut irons) = irons_dying(1, 0, 2);
        irons.work.move_cnt = 0x46;
        spl_sp_move(&mut game, &mut irons, &tick, 0);
        assert_eq!(embryo_step(&game), 5);
    }

    /// Step 2 sets `Timer0` 0x96 and falls into step 3, which counts it
    /// down; at 0x82 it moves the embryo's `Routine_2` on one, and on the
    /// tick it counts from 0 it goes on to step 4 and moves it on again.
    /// Ben's step 2 doesn't fall through, and his step 3 does nothing.
    #[test]
    fn irons_s_death_counts_down_and_moves_the_embryo_on() {
        let tick = spl_tick(&RoomState::default(), &Default::default());
        let (mut game, mut irons) = irons_dying(2, 0, 9);
        spl_sp_move(&mut game, &mut irons, &tick, 0);
        assert_eq!((irons.work.routine[2], irons.work.timers[0]), (3, 0x95));
        assert_eq!(embryo_step(&game), 9);
        let (mut game, mut irons) = irons_dying(3, 0x82, 9);
        spl_sp_move(&mut game, &mut irons, &tick, 0);
        assert_eq!((irons.work.routine[2], irons.work.timers[0]), (3, 0x81));
        assert_eq!(embryo_step(&game), 10);
        let (mut game, mut irons) = irons_dying(3, 1, 10);
        spl_sp_move(&mut game, &mut irons, &tick, 0);
        assert_eq!((irons.work.routine[2], irons.work.timers[0]), (3, 0));
        assert_eq!(embryo_step(&game), 10);
        spl_sp_move(&mut game, &mut irons, &tick, 0);
        assert_eq!((irons.work.routine[2], irons.work.timers[0]), (4, 0xFFFF));
        assert_eq!(embryo_step(&game), 11);
        spl_sp_move(&mut game, &mut irons, &tick, 0);
        assert_eq!(irons.work.timers[0], 0xFFFF, "step 4 does nothing");
        let (mut game, mut ben) = irons_dying(2, 0, 9);
        ben.id = BEN;
        spl_sp_move(&mut game, &mut ben, &tick, 0);
        assert_eq!((ben.work.routine[2], ben.work.timers[0]), (3, 0x96));
        ben.work.timers[0] = 0x82;
        spl_sp_move(&mut game, &mut ben, &tick, 0);
        assert_eq!((ben.work.routine[2], ben.work.timers[0]), (3, 0x82));
        assert_eq!(embryo_step(&game), 9);
    }

    /// Step 3 swells part 0 by 100 from 0x81 to 0x78, shrinks it by 200
    /// from 0x76 to 0x72, swells it by 0x50 from 0x70 to 0x67 and shrinks
    /// it by 0xa0 from 0x65 to 0x61; it bleeds kind 0 row 2 (0xfa0) on
    /// part 0 at the counts of eight from 0x88 to 0x78.
    #[test]
    fn irons_s_death_swells_part_0_and_bleeds_by_its_count() {
        let tick = spl_tick(&RoomState::default(), &Default::default());
        for (count, swell, bleeds) in [
            (0x90, 0, false),
            (0x88, 0, true),
            (0x82, 0, false),
            (0x81, 100, false),
            (0x80, 100, true),
            (0x78, 100, true),
            (0x77, 0, false),
            (0x76, -200, false),
            (0x72, -200, false),
            (0x71, 0, false),
            (0x70, 0x50, false),
            (0x67, 0x50, false),
            (0x66, 0, false),
            (0x65, -0xA0, false),
            (0x61, -0xA0, false),
            (0x60, 0, false),
        ] {
            let (_, mut irons) = irons_dying(3, count, 9);
            irons.work.sp_weight = 0x800;
            let out = player2::spl_special(&mut irons.work, &mut irons.body, &tick, spl_sp40_00);
            assert_eq!(irons.work.sp_weight, 0x800 + swell, "{count:#x}");
            let calls: Vec<_> = out
                .calls
                .iter()
                .map(|&(kind, row, size, dir, _, point)| (kind, row, size, dir, point))
                .collect();
            let expected: &[_] = if bleeds {
                &[(0, 2, 0xFA0, 0x400, [0; 3])]
            } else {
                &[]
            };
            assert_eq!(calls, expected, "{count:#x}");
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

    fn ada(kind: u16) -> Character {
        Character {
            id: 0x41,
            model: 0x41,
            ..sherry(kind)
        }
    }

    /// `Oba_ck_hit` and `Oba_ck_esp` take `nOba` boxes of each body: one
    /// with none (room 301's embryo in Ben) has nothing to push against.
    #[test]
    fn a_body_with_no_oba_has_no_box_for_the_others() {
        let mut game = GameState::default();
        let mut embryo = sherry(0);
        embryo.id = crate::zombie2::EMBRYO_ID;
        embryo.zombie = Some(Default::default());
        game.re2.characters.0 = vec![None, Some(Box::new(embryo))];
        assert_eq!(slot_bodies(&game).len(), 1);
        let embryo = game.re2.characters.0[1].as_deref_mut().unwrap();
        embryo.zombie.as_mut().unwrap().no_oba = true;
        assert!(slot_bodies(&game).is_empty());
    }

    /// `Sca_ck_em` leaves a character with `Be_flg`'s 8 where it stands:
    /// room 301's Ben, whom `main_00` sets it on, lies in his bunk's box
    /// rather than being pushed 634 units out of it while the room loads.
    #[test]
    fn a_scene_character_with_be_flg_8_is_not_pushed_out_of_the_walls() {
        let bunk = crate::state::ScaRect {
            x: -19000,
            z: -25000,
            width: 2000,
            depth: 2000,
            id: crate::sca::CHARACTER_MASK,
            kind: 0xF,
            floor: 1,
        };
        let room = RoomState {
            sca: Some(crate::state::Sca {
                records: vec![bunk],
                ..Default::default()
            }),
            ..RoomState::default()
        };
        let placed = |be_flg| {
            let mut character = ada(0);
            character.be_flg = be_flg;
            character.body.pos = [-17950, 0, -24000];
            character.body.collision_mirror = [-17950, -24000];
            spl_sca_ck_em(&room, &mut character);
            (character.body.pos, character.body.collision_mirror)
        };
        let (pushed, _) = placed(1);
        assert_ne!(pushed, [-17950, 0, -24000], "pushed out of the box");
        let (kept, mirror) = placed(1 | crate::zombie2::BE_NO_SCA);
        assert_eq!(kept, [-17950, 0, -24000]);
        assert_eq!(mirror, [-17950, -24000]);
    }

    /// A room's load keeps the player's box where the last room left it,
    /// sized for the player `Pl_init` makes: room 21A's sleeping dog in slot
    /// 0 is pushed out of where Leon stood at 219's door 2.
    #[test]
    fn pl_init_keeps_the_players_box_where_the_last_room_left_it() {
        let mut game = GameState::default();
        game.re2.player.atd = Some(crate::zombie2::Atd {
            ofs_x: 0,
            ofs_y: -100,
            at_w: 1,
            at_h: 2,
            w: 3,
            at: [-6299, -1530, -12275],
        });
        game.re2.player = game.re2.player.entering_room();
        pl_init_atd(&mut game);
        let atd = game.re2.player.atd.unwrap();
        assert_eq!(atd.at, [-6299, -1530, -12275]);
        let sized = player_atd(&game, [0; 3]);
        assert_eq!(
            (atd.ofs_y, atd.at_w, atd.at_h, atd.w),
            (sized.ofs_y, sized.at_w, sized.at_h, sized.w)
        );
        game.re2.player.atd = None;
        game.re2.player = game.re2.player.entering_room();
        pl_init_atd(&mut game);
        assert_eq!(game.re2.player.atd, None, "none before: none yet");
    }

    /// `Game_loop` runs `Oba_ck_em2` once a tick, a held one too: where the
    /// player's own tick didn't push, the push runs from where the player
    /// stands; where it did, it doesn't run twice; a player whose `Type` has
    /// 0x8000 isn't pushed.
    #[test]
    fn the_push_runs_once_a_tick_with_the_player_held_too() {
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
        let setup = || {
            let mut game = GameState::default();
            game.id = room;
            game.re2.characters.0 = vec![Some(Box::new(kendo.clone()))];
            let mut player = crate::player::spawn(room, &RoomState::default());
            player.pos = [-850, 0, 0];
            (game, player)
        };

        let (mut game, mut player) = setup();
        oba_ck_em2_unmoved(&mut game, &mut player);
        assert_eq!(
            player.pos,
            [-901, 0, 0],
            "held: pushed from where it stands"
        );
        assert_eq!(game.re2.player.at_em_flg, 1);
        assert_eq!(game.re2.player.atd.map(|atd| atd.at[0]), Some(-901));
        assert!(!game.re2.player.oba_ran);

        let (mut game, mut player) = setup();
        oba_ck_em2(&mut game, &mut player, [-850, 0, 0]);
        player.pos = [-850, 0, 0];
        oba_ck_em2_unmoved(&mut game, &mut player);
        assert_eq!(player.pos, [-850, 0, 0], "moved: pushed once, not twice");
        assert!(!game.re2.player.oba_ran, "the next tick starts unpushed");

        let (mut game, mut player) = setup();
        game.re2.player.kind |= player2::KIND_NOT_MOVED;
        oba_ck_em2_unmoved(&mut game, &mut player);
        assert_eq!(player.pos, [-850, 0, 0], "Type 0x8000: not pushed");
        assert_eq!(game.re2.player.atd, None);
    }

    /// Ben's part 8 at (1000, -500, 2000), turned a quarter round.
    fn bens_part_8() -> player2::Matrix {
        player2::Matrix {
            m: player2::rot_matrix([0, 0x400, 0]),
            t: [1000, -500, 2000],
        }
    }

    fn hung_on_ben() -> crate::zombie2::Hung {
        crate::zombie2::Hung {
            part: 0x12,
            on_slot: 0,
            on_part: 8,
            at: [0xF6, -0xFB, 0],
            in_play: true,
        }
    }

    /// What a hung part hangs on: a scene character's latched `Workm`
    /// (`Work::parts`), a zombie's `drawn`, and
    /// nothing for the body's own slot or an empty one.
    #[test]
    fn a_hung_part_hangs_on_the_other_works_drawn_part() {
        let mut ben = ada(0);
        ben.id = 0x44;
        ben.work.parts = vec![None; 9];
        ben.work.parts[8] = Some(bens_part_8());
        let mut characters = vec![Some(Box::new(ben)), None];
        assert_eq!(hung_on(&characters, 1, hung_on_ben()), Some(bens_part_8()));
        assert_eq!(hung_on(&characters, 0, hung_on_ben()), None, "its own slot");
        let off = crate::zombie2::Hung {
            on_slot: 1,
            ..hung_on_ben()
        };
        assert_eq!(hung_on(&characters, 2, off), None, "an empty slot");
        let mut g1 = ada(0);
        let mut zombie = crate::zombie2::Zombie::default();
        zombie.drawn = vec![None; 9];
        zombie.drawn[8] = Some(bens_part_8());
        g1.zombie = Some(zombie);
        g1.work.parts = vec![None; 9];
        characters[0] = Some(Box::new(g1));
        assert_eq!(
            hung_on(&characters, 1, hung_on_ben()),
            Some(bens_part_8()),
            "a zombie's drawn part"
        );
    }

    /// `run_zombie` on the G1 in slot 1 composes its hung part 0x12 on Ben's
    /// part 8 in slot 0 as Ben's tick latched it.
    #[test]
    fn run_zombie_draws_a_hung_part_on_the_scene_characters_part() {
        let model = posable_model();
        let pack = Pack::from_bytes(crate::pack::PackWriter::new().to_bytes().unwrap()).unwrap();
        let mut ben = ada(0);
        ben.id = 0x44;
        ben.work.parts = vec![None; 9];
        ben.work.parts[8] = Some(bens_part_8());
        let mut g1 = sherry(0);
        g1.id = 0x10;
        g1.model = 0x10;
        g1.partner = None;
        g1.work.routine = [1, 0, 0, 0];
        let mut zombie = crate::zombie2::Zombie::default();
        zombie.hung = Some(hung_on_ben());
        let mut game = GameState::default();
        game.re2.characters.0 = vec![Some(Box::new(ben)), None];
        let _ = run_zombie(
            &mut game,
            &RoomState::default(),
            &pack,
            None,
            &mut g1,
            &mut zombie,
            1,
            &model,
            false,
            false,
        );
        let drawn = zombie.drawn[0x12].unwrap();
        assert_eq!(drawn.m, bens_part_8().m);
        assert_eq!(drawn.t, [1000, -500 - 0xFB, 2000 - 0xF6]);
    }

    /// `Game_loop` runs `splayer_work`, then the enemy slots from 0: slot 0's
    /// scene character latches its parts before slot 1's G1 hangs on them.
    #[test]
    fn the_partner_runs_first_then_the_slots_from_0() {
        let order: Vec<usize> = run_order().take(3).collect();
        assert_eq!(order, [SPLAYER_SLOT, 0, 1]);
    }

    /// After `Sca_ck_em` pushes a scene character out of a wall, the draw
    /// (`latch_drawn`) composes its parts where it was pushed to.
    #[test]
    fn a_scene_characters_parts_are_drawn_where_the_pushes_leave_it() {
        let wall = crate::state::ScaRect {
            x: -19000,
            z: -25000,
            width: 2000,
            depth: 2000,
            id: crate::sca::CHARACTER_MASK,
            kind: 0xF,
            floor: 1,
        };
        let room = RoomState {
            sca: Some(crate::state::Sca {
                records: vec![wall],
                ..Default::default()
            }),
            ..RoomState::default()
        };
        let model = posable_model();
        let mut character = ada(0);
        character.body.pos = [-17950, 0, -24000];
        character.body.collision_mirror = [-17950, -24000];
        let tick = spl_tick(&room, &model.skeleton);
        let tick = Tick {
            banks: Banks {
                pld: (&model.clips, &model.keyframes),
                ..tick.banks
            },
            ..tick
        };
        character.work.bank = player2::Bank::Pld;
        spl_sca_ck_em(&room, &mut character);
        let pushed = character.body.pos;
        assert_ne!(pushed, [-17950, 0, -24000], "pushed out of the box");
        player2::latch_drawn(&mut character.work, &mut character.body, &tick);
        let part0 = character.work.parts[0].unwrap();
        assert_eq!([part0.t[0], part0.t[2]], [pushed[0], pushed[2]]);
    }

    #[test]
    fn a_scene_adas_gun_hand_swaps_once_and_marks_her_set_up() {
        // Room 216's `sce_em_set` gives her `Type` 0x0800: the original's
        // work then holds 0x5080.
        let mut game = GameState::default();
        let mut character = ada(0x0800);
        let tick = spl_tick(&RoomState::default(), &Default::default());
        spl(&mut game, &mut character, &tick);
        assert_eq!(character.kind, 0x5080);
        assert!(character.arms.swapped);
        assert_ne!(character.work.hidden_parts & 1 << ADA_SPARE_HAND, 0);
        let meshes = joint_meshes(&character, 16, 16).unwrap();
        assert_eq!(meshes[ADA_HAND], ADA_SPARE_HAND);
        assert!(!game.re2.partner_bit());
        assert_eq!(character.work.routine[0], player2::MODE_SCENARIO);
    }

    #[test]
    fn a_partner_ada_raises_the_partner_bit_and_follows() {
        let mut game = GameState::default();
        game.re2.partner_life = 150;
        let mut character = ada(0x400);
        let tick = spl_tick(&RoomState::default(), &Default::default());
        spl(&mut game, &mut character, &tick);
        assert!(game.re2.partner_bit());
        assert_eq!(character.kind, 0x480);
        assert_eq!(character.work.routine, [1, 0, 0, 0]);
        let partner = character.partner.as_deref().unwrap();
        assert_eq!((partner.life, partner.max_life), (150, 200));
        assert_eq!(character.work.neck.target, Some(0));
        // The partner's 0x400 takes 0x800, so the gun hand swaps here too.
        assert!(character.arms.swapped);
        assert_ne!(character.work.hidden_parts & 1 << ADA_SPARE_HAND, 0);
        // `Spl_Auto` runs `Ada_move` from the next tick: 2000 from the point,
        // Sherry's stand would leave (past 1500) and Ada's stays.
        character.partner.as_deref_mut().unwrap().dist = 2000;
        spl(&mut game, &mut character, &tick);
        assert_eq!(character.work.routine, [1, 0, 1, 0]);
        let mut sherry = sherry(0x400);
        spl(&mut game, &mut sherry, &tick);
        sherry.partner.as_deref_mut().unwrap().dist = 2000;
        spl(&mut game, &mut sherry, &tick);
        assert_ne!(sherry.work.routine[1], 0, "Sherry leaves the stand");
    }

    #[test]
    fn ada_0x43_as_the_partner_follows_whatever_0x4000_chose() {
        let mut game = GameState::default();
        let mut character = Character {
            id: 0x43,
            model: 0x43,
            ..ada(0x4400)
        };
        let tick = spl_tick(&RoomState::default(), &Default::default());
        spl(&mut game, &mut character, &tick);
        assert!(game.re2.partner_bit());
        assert_eq!(character.work.routine, [1, 0, 0, 0]);
    }

    /// Room 21B's body on the desk (`Spl_in4b`) keeps its first pose, at the
    /// height of its place, rather than starting `Spl_sce06`'s idle, whose
    /// clip her one-clip model doesn't have (and which drew nothing).
    #[test]
    fn the_desk_body_holds_its_first_pose_with_its_part_at_height_0() {
        let mut game = GameState::default();
        let mut character = Character {
            id: DESK_BODY,
            model: DESK_BODY,
            partner: None,
            ..sherry(0)
        };
        // Her model: one part and one clip of one frame, in the body's own bank.
        let mut tick = spl_tick(&RoomState::default(), &Default::default());
        let clips: &'static [Clip] = Box::leak(Box::new([Clip {
            frames: vec![crate::model::ClipFrame::default()],
        }]));
        let keyframes: &'static [Keyframe] = Box::leak(Box::new([Keyframe {
            offset: [10, -1810, 20],
            rotations: vec![[0; 3]],
            ..Default::default()
        }]));
        tick.banks.pld = (clips, keyframes);
        tick.id = DESK_BODY;
        spl(&mut game, &mut character, &tick);
        assert_eq!(character.work.routine, [4, SCE_STILL, 0, 0]);
        assert_eq!(character.work.neck.flags, 1);
        assert_eq!(character.work.neck.joint, Some(0));
        assert_eq!(character.be_flg & 0xB, 0xB);
        spl(&mut game, &mut character, &tick);
        assert_eq!(character.work.routine, [4, SCE_STILL, 0, 0]);
        // Drawn: her own bank's first frame, at height 0.
        let (clips, keyframes) = tick.banks.get(character.work.bank);
        let mut pose = character
            .body
            .anim
            .pose_keyframe(clips, keyframes)
            .expect("a pose to draw");
        character
            .work
            .apply_to_pose(&character.body.anim, &mut pose);
        assert_eq!(pose.offset, [10, 0, 20]);
    }

    #[test]
    fn adas_part_16_is_on_only_with_scenario_flag_80() {
        let hidden = |flag: bool| {
            let mut game = GameState::default();
            if flag {
                game.re2.set_flag(3, ADA_PART_16_FLAG, 1);
            }
            let mut character = ada(0x0800);
            let tick = spl_tick(&RoomState::default(), &Default::default());
            spl(&mut game, &mut character, &tick);
            character.work.hidden_parts & 1 << ADA_PART_16 != 0
        };
        assert!(hidden(false));
        assert!(!hidden(true));
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
        spl_oba_ck_em(&game, &mut character, [0; 3]);
        assert!(character.body.pos[0] > 0, "{:?}", character.body.pos);
        assert_eq!(character.body.pos[2], 0);
    }

    /// `_Spl` pushes every character it runs, not only the partner, and runs
    /// before the player's move: a scene character on the player is pushed
    /// off it, and the player's own `Oba_ck_em2` then stays where it is.
    #[test]
    fn a_scene_character_on_the_player_is_pushed_off_it_first() {
        let mut game = GameState::default();
        game.id = RoomId::parse("4040").unwrap();
        let mut player = crate::player::spawn(game.id, &RoomState::default());
        player.pos = [-400, 0, 0];
        game.entities[0].pos = player.pos;
        let at = player.pos;
        oba_ck_em2(&mut game, &mut player, at);
        let mut character = sherry(0x400);
        character.id = 0x50;
        character.model = 0x50;
        character.partner = None;
        spl_after_routine(&mut game, &mut character, [0; 3]);
        assert!(character.body.pos[0] > 0, "{:?}", character.body.pos);
        game.re2.characters.0 = vec![Some(Box::new(character))];
        oba_ck_em2(&mut game, &mut player, at);
        assert_eq!(player.pos, [-400, 0, 0]);
        assert_eq!(game.re2.player.at_em_flg, 0);
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
        // The spares and the gun hand go off, but not black: only the fade's
        // end blackens them.
        assert_eq!(character.work.hidden_parts, 0xFE8000);
        assert!((0..24).all(|part| character.work.part_rgb[part] == player2::DEFAULT_PART_RGB));
        let meshes = joint_meshes(&character, 15, 24).unwrap();
        assert_eq!(&meshes[8..15], &[17, 20, 21, 22, 18, 15, 19]);
        // As a zombie he keeps them: the model decides.
        character.id = 0x1E;
        assert!(joint_meshes(&character, 15, 24).is_some());
        character.model = 0x45;
        assert!(joint_meshes(&character, 15, 24).is_none());
    }

    /// `Spl_Root_ck_sys` counts a scene character's turn (`+0x150`) every
    /// tick from the slot `Sce_em_set` seeded it with, and room 202's script
    /// making Marvin's work a zombie's hands it on to `Root_ck` (the
    /// original's `rc` over his scene: 6, 5, ... 0, 6, and on as id 30).
    #[test]
    fn a_scene_characters_root_turn_counts_and_goes_on_as_a_zombie() {
        let mut character = sherry(0x1000);
        character.partner = None;
        character.id = MARVIN;
        character.model = MARVIN;
        character.work.root_turn = 0;
        let game = GameState::default();
        let room = RoomState::default();
        let turns: Vec<u8> = (0..9)
            .map(|_| {
                root_ck_sys(&game, &room, &mut character, false);
                character.work.root_turn
            })
            .collect();
        assert_eq!(turns, [6, 5, 4, 3, 2, 1, 0, 6, 5]);
        character.id = 0x1E;
        crate::scd2::host::become_zombie(&mut character, 0);
        assert_eq!(character.zombie.as_ref().unwrap().root_turn(), 5);
    }

    /// `Sce_em_set` seeds the turn with the work's number plus 2 (in 0 to
    /// 7), and each of `Em_init_move`'s `_Spl` calls counts it, the second
    /// only with a start: room 202's Marvin (slot 0, start 261) loads at 0.
    #[test]
    fn sce_em_set_seeds_the_root_turn_and_em_init_move_counts_it_per_spl() {
        use crate::scd2::vm::Host;
        let cases = [
            (0, 261, 2, 0),
            (0, 0, 2, 1),
            (6, 261, 0, 5),
            (6, 0, 0, 6),
            (7, 0, 1, 0),
        ];
        for (slot, start, seed, loaded) in cases {
            let mut game = GameState::default();
            let operands = [
                0, slot, 74, 0, 8, 0, 0, 0, 28, -6100, 0, -16100, 2048, start, 20,
            ];
            let op = crate::scd2::opcode::by_mnemonic("sce_em_set").unwrap();
            crate::scd2::host::GameHost { game: &mut game }.exec(0, op, &operands);
            let mut character = game.re2.characters.0[slot as usize].take().unwrap();
            assert_eq!(character.id, MARVIN);
            assert_eq!(character.work.root_turn, seed, "slot {slot}'s seed");
            let room = RoomState::default();
            let tick = spl_tick(&room, &Default::default());
            em_init_move(&mut game, &room, &mut character, &tick);
            assert_eq!(
                character.work.root_turn, loaded,
                "slot {slot}, start {start}"
            );
        }
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
        assert_eq!(
            character.work.hidden_parts,
            1 << MARVIN_GUN_HAND,
            "only the gun hand, from init, is hidden before the swap"
        );
        spl_mv4a(&mut character);
        assert!(character.arms.swapped);
        // The spares (15, 17 to 22) and the gun hand (23) go off, the spares
        // black: with `Em10_Init`'s 16 that's the original's `zp` for him.
        assert_eq!(character.work.hidden_parts, 0xFE8000);
        let black: Vec<usize> = (0..24)
            .filter(|&part| character.work.part_rgb[part] == 0)
            .collect();
        assert_eq!(black, [15, 17, 18, 19, 20, 21, 22]);
        assert_eq!(character.work.kind, 0);
        // A second fade never ends.
        character.work.kind = 0x200;
        for _ in 0..200 {
            spl_mv4a(&mut character);
        }
        assert_eq!(character.work.kind, 0x200);
    }

    #[test]
    fn pl_init_box_by_id() {
        // `Pl_init`: Claire's and Ada's 350 by 1350, Sherry's 250 by 810,
        // Leon's `Player_set` 450 by 0x5FA.
        for (id, width, height) in [
            (0, 450, 0x5FA),
            (1, 350, 1350),
            (0xE, 350, 1350),
            (0xF, 250, 810),
        ] {
            let mut game = GameState::default();
            game.re2.pl_id = id;
            let atd = player_atd(&game, [100, 0, 200]);
            assert_eq!(
                (atd.at_w, atd.w, atd.at_h),
                (width, width, height),
                "id {id:#x}"
            );
            assert_eq!(atd.ofs_y, -(height as i16));
            assert_eq!(atd.at, [100, -i32::from(height), 200]);
            assert_eq!(crate::sca::player_radius(id), i32::from(width));
        }
    }
}
