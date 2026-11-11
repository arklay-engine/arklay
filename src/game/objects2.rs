//! RE2's room objects in play: `Om_move` (`0x4CCD70`), which pushes the
//! pushable ones and holds the characters out of them, and `Om_trans`' copies
//! of where each was drawn.
//!
//! Not ported: `Om_move_box` and the falls of the `Id` 2 boxes (rooms 203,
//! 216, 306, 603 and F03), and `Oma_pull`, standing on an object.

use std::sync::atomic::{AtomicBool, Ordering};

use super::GameState;
use crate::anim::Mat4x3;
use crate::character2::{PLAYER_HEIGHT, player_width};
use crate::objects::{
    CharBox, ObjectRecord, Oma, SCA_HIT_WALL, SuperParent, floor_of_height, oma_ck_front,
    oma_ck_oba, oma_ck_om, sca_ck_hit_om,
};
use crate::player::PlayerState;
use crate::state::RoomState;

/// `Be_flg`: the object is in play.
const BE_ACTIVE: u32 = 1;
/// `Be_flg`: nothing touches the object.
const BE_GHOST: u32 = 2;
/// `Be_flg`: the player can push the object.
const BE_PUSHABLE: u32 = 0x80;
/// `Be_flg`: the player pushes the object out of the way (mode 1) instead.
const BE_SHOVED: u32 = 0x200;
/// `Be_flg`: `Om_trans` doesn't draw the object.
const BE_HIDDEN: u32 = 0x8_0000;
/// `Id` of the boxes that fall (`Om_move_box`).
const ID_BOX: u8 = 2;
/// `GsIdMatrix`.
const IDENTITY: Mat4x3 = Mat4x3 {
    r: [[4096, 0, 0], [0, 4096, 0], [0, 0, 4096]],
    t: [0, 0, 0],
};
/// `Routine_1`s that keep `Push_cnt`: the walk and the push.
const ROUTINE_WALK: u8 = 1;
const ROUTINE_PUSH: u8 = 10;
/// `Push_cnt` from which `Om_move` tries a step, the count that only tries
/// it, and the one that keeps it.
const PUSH_TRY: u8 = 6;
const PUSH_STEP: u8 = 7;

static BOX_WARNED: AtomicBool = AtomicBool::new(false);

impl GameState {
    /// `Om_move`, after the player's move and before `Sce_at`: each object's
    /// box and floor, the characters held out of it, the push, and then the
    /// player held out of every object.
    ///
    /// The push needs the player to touch exactly one object, on its floor
    /// and nearly square on (`Oma_ck_front`), with `Be_flg` `0x80`. Each such
    /// tick counts (`Push_cnt`, kept only while the player walks or pushes).
    /// From the sixth the object takes the player's step into it, unless the
    /// step meets a wall record, a character or another object: on the sixth
    /// `fg_status`'s `0x2000` comes up and the step is undone, from the
    /// seventh it stays. A refused step puts the count back to 0.
    ///
    /// Returns the first falling box (`Id` 2) a character touched, whose
    /// `Om_move_box` isn't ported: the first such touch warns.
    pub fn om_move(&mut self, room: &RoomState, player: &mut PlayerState) -> Option<usize> {
        if self.re2.objects_stopped() {
            return None;
        }
        self.re2.set_pushing(false);
        let keeps_count = matches!(self.re2.player.routine[1], ROUTINE_WALK | ROUTINE_PUSH);
        let width = player_width(self.id.player_flag);
        let ofs_y = -(PLAYER_HEIGHT as i16);
        let mut pl = CharBox::standing(player.pos, player.pos, ofs_y, width, PLAYER_HEIGHT);
        if let Some(old) = self.re2.player.old_box {
            pl.old2 = old.map(|v| v as i16);
        }
        pl.be_flg = self.re2.player.be_flg;

        let mut touched = 0;
        let mut candidate = None;
        let mut box_touched = None;
        for index in 0..self.objects.records.len() {
            let mut record = self.objects.records[index];
            if record.be_flg & BE_ACTIVE == 0 {
                continue;
            }
            record.set_ofs();
            record.floor = floor_of_height(record.pos[1]);
            if !keeps_count {
                record.push_cnt = 0;
            }
            let mut hit = self.characters_oma(&mut record, Oma::Hold) != 0;
            if record.be_flg & BE_SHOVED == 0 {
                if oma_ck_oba(&mut pl, &mut record, Oma::Touch) {
                    hit = true;
                    touched += 1;
                    if self.re2.player.floor == record.floor
                        && oma_ck_front(pl.pos, player.angle, width, width, &record)
                    {
                        candidate = Some(index);
                    }
                }
            } else {
                hit |= oma_ck_oba(&mut pl, &mut record, Oma::Push);
            }
            if hit && record.id == ID_BOX {
                box_touched = box_touched.or(Some(index));
            }
            self.objects.records[index] = record;
        }

        if let Some(index) = candidate.filter(|_| touched == 1) {
            let record = self.objects.records[index];
            if record.be_flg & BE_GHOST == 0 && record.be_flg & BE_PUSHABLE != 0 {
                let record = self.om_push(room, &mut pl, record, index);
                self.objects.records[index] = record;
            }
        }

        for (index, record) in self.objects.records.iter_mut().enumerate() {
            if record.be_flg & BE_ACTIVE != 0
                && oma_ck_oba(&mut pl, record, Oma::Hold)
                && record.id == ID_BOX
            {
                box_touched = box_touched.or(Some(index));
            }
        }
        player.pos = pl.pos;
        self.entities[0].pos = player.pos;
        // The head's `+0x5C` is made after `Om_move`, from the body the objects
        // held out, as after `Sca_ck_em`'s pushes.
        crate::player2::follow_pushes(&mut self.re2.player, player.pos);
        if let Some(index) = box_touched
            && !BOX_WARNED.swap(true, Ordering::Relaxed)
        {
            crate::warning!(
                "warning: room object {index} is a falling box (Id 2), and Om_move_box isn't ported"
            );
        }
        box_touched
    }

    /// `Om_move`'s count and step for the one object the player walks into.
    fn om_push(
        &mut self,
        room: &RoomState,
        pl: &mut CharBox,
        mut record: ObjectRecord,
        index: usize,
    ) -> ObjectRecord {
        let saved = [record.pos[0], record.pos[2]];
        record.push_cnt = record.push_cnt.wrapping_add(1);
        if record.push_cnt >= PUSH_TRY {
            oma_ck_oba(pl, &mut record, Oma::Push);
            let walls = room
                .sca
                .as_ref()
                .map_or(0, |sca| sca_ck_hit_om(&record, sca, 0));
            if walls & SCA_HIT_WALL == 0 {
                // Both move the pushed object as they test it.
                let mut blocked = self.characters_oma(&mut record, Oma::Push);
                for (other, object) in self.objects.records.iter().enumerate() {
                    if other != index
                        && object.be_flg & BE_ACTIVE != 0
                        && oma_ck_om(object, &mut record)
                    {
                        blocked += 1;
                    }
                }
                if blocked == 0 {
                    self.re2.set_pushing(true);
                    if record.push_cnt != PUSH_TRY {
                        record.push_cnt = PUSH_STEP;
                        return record;
                    }
                } else {
                    record.push_cnt = 0;
                }
            } else {
                record.push_cnt = 0;
            }
        }
        // Only x and z go back: a height `Oma_ck_om` gave stays.
        record.pos[0] = i32::from(saved[0] as i16);
        record.pos[2] = i32::from(saved[1] as i16);
        record.set_ofs();
        record
    }

    /// `Oma_ck_oba` for the characters in the enemy slots and the partner
    /// against `record`: how many touched it.
    fn characters_oma(&mut self, record: &mut ObjectRecord, mode: Oma) -> u32 {
        let mut touched = 0;
        for character in self.re2.characters.0.iter_mut().flatten() {
            if character.be_flg & BE_ACTIVE == 0 {
                continue;
            }
            let atd = crate::character2::body_atd(character);
            // Where the draw left its box, else where it is.
            let old = character.work.old_box.unwrap_or(atd.at);
            let mut chr = CharBox {
                be_flg: character.be_flg,
                pos: character.body.pos,
                at: atd.at,
                old2: old.map(|v| v as i16),
                ofs_y: atd.ofs_y,
                at_w: atd.at_w,
                at_h: atd.at_h,
            };
            if oma_ck_oba(&mut chr, record, mode) {
                touched += 1;
            }
            character.body.pos = chr.pos;
            if let Some(zombie) = character.zombie.as_mut() {
                zombie.atd.at = chr.at;
            }
        }
        touched
    }

    /// `Om_trans` as it draws, object by object in slot order: the world
    /// matrix (`+0x54`, composed with the parent's), then the copies `Old`
    /// and `Old2`. An `Id` 2 box's world y is lifted 900 after the compose.
    /// An object's parent of a later slot gives last frame's matrix, as in
    /// the original. `player` is the player's position and angle.
    ///
    /// `Game_loop` runs it while `Can_draw` is 0, and `WinMain` runs it after
    /// the frame's draw when a script's `sce_line_main` has set `Can_draw`
    /// (and clears it), so it composes every frame either way.
    pub fn om_trans_old(&mut self, player: ([i32; 3], u16)) {
        // Room 409's cut 12 turns slot 4 to -40 about z each frame it draws
        // (`Get_room_id(1) == 0xC0409`).
        let room_409_cut_12 = self.is_re2_room
            && (self.id.stage, self.id.room) == (4, 9)
            && self.camera.current_cut == 12;
        for index in 0..self.objects.records.len() {
            let record = self.objects.records[index];
            if record.be_flg & BE_ACTIVE != 0 && record.be_flg & BE_HIDDEN == 0 {
                if room_409_cut_12 && index == 4 {
                    self.objects.records[index].rotation[2] = -40;
                }
                self.compose_object_world(index, player);
                let record = &mut self.objects.records[index];
                if record.id == ID_BOX {
                    record.world.t[1] -= 900;
                }
                record.om_trans_old();
            }
        }
    }

    /// `CompM(+0x80, local, +0x54)`, as `Obj_model_set` and `Om_trans` do
    /// it: object `index`'s world matrix from its parent's and its own
    /// rotation and position.
    pub fn compose_object_world(&mut self, index: usize, player: ([i32; 3], u16)) {
        let Some(record) = self.objects.records.get(index).copied() else {
            return;
        };
        let parent = self.super_matrix(record.super_parent, player);
        let local = crate::anim::Mat4x3 {
            r: crate::objects::rotation(&record),
            t: record.pos,
        };
        self.objects.records[index].world = crate::anim::compose(&parent, &local);
    }

    /// The matrix an object's `+0x80` points at (`Super_set`, `Super_on`):
    /// the identity, the player's own matrix or a joint of its pose, the
    /// second player's or an enemy's own matrix or a joint of its pose
    /// ([`crate::scd2::host::Re2State::character_joints`]), or another
    /// object's world matrix. A joint is the one the frame composed for the
    /// draw: `Game_loop` runs `Joint_trans2` for every character before
    /// `Om_trans`.
    pub(crate) fn super_matrix(
        &self,
        parent: SuperParent,
        (pos, angle): ([i32; 3], u16),
    ) -> Mat4x3 {
        let player = crate::anim::entity_matrix(pos, angle);
        let player_joint = |joint: u8| {
            self.joint_worlds[0]
                .get(usize::from(joint))
                .copied()
                .unwrap_or(player)
        };
        let character_slot = |work: u8| match work {
            1 => crate::scd2::host::SPLAYER_SLOT,
            n => usize::from(n.wrapping_sub(2)),
        };
        let character = |work: u8| {
            self.re2
                .characters
                .0
                .get(character_slot(work))
                .and_then(|character| character.as_deref())
                .map_or(IDENTITY, |character| {
                    crate::anim::entity_matrix(character.body.pos, character.body.angle)
                })
        };
        match parent {
            SuperParent::World => IDENTITY,
            SuperParent::Player | SuperParent::Work(0) => player,
            SuperParent::PlayerJoint(joint) | SuperParent::WorkJoint(0, joint) => {
                player_joint(joint)
            }
            SuperParent::Work(work) => character(work),
            SuperParent::WorkJoint(work, joint) => self
                .re2
                .character_joints
                .get(character_slot(work))
                .and_then(|joints| joints.get(usize::from(joint)))
                .copied()
                .unwrap_or_else(|| character(work)),
            SuperParent::Object(n) => self
                .objects
                .records
                .get(usize::from(n))
                .map_or(IDENTITY, |record| record.world),
        }
    }

    /// The trace's `om` field: for each of the room's `count` objects,
    /// `n:Be_flg,x,y,z,Cdir_y,Push_cnt,At_sce_no,nFloor`, joined with `;`.
    pub fn re2_objects_trace(&self, count: usize) -> String {
        let rows: Vec<String> = (0..count)
            .map(|index| {
                let record = self.objects.records.get(index).copied().unwrap_or_default();
                format!(
                    "{index}:{:x},{},{},{},{},{},{},{}",
                    record.be_flg,
                    record.pos[0],
                    record.pos[1],
                    record.pos[2],
                    record.rotation[1] as u16,
                    record.push_cnt,
                    record.at_sce_no,
                    record.floor,
                )
            })
            .collect();
        if rows.is_empty() {
            "om=-".to_string()
        } else {
            format!("om={}", rows.join(";"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::{ObjectAtd, ObjectTable};
    use crate::state::{Sca, ScaRect};

    const REST: i32 = -13000 + 432 + 450 + 1;
    const STEP: i32 = 40;

    fn statue(z: i32) -> ObjectRecord {
        let mut record = ObjectRecord {
            flag: crate::objects::OBJECT_FLAG_ACTIVE,
            pos: [-22900, -3600, z],
            be_flg: 0x85,
            atd2: ObjectAtd {
                ofs: [0, -900, 0],
                at_w: 432,
                at_h: 900,
                at_d: 432,
            },
            ..ObjectRecord::default()
        };
        record.set_ofs();
        record.om_trans_old();
        record
    }

    /// Room 10C's hall: Leon on floor 2 at statue 1's +z face, facing -z.
    fn hall(objects: &[ObjectRecord]) -> (GameState, RoomState, PlayerState) {
        let mut game = GameState {
            is_re2_room: true,
            objects: ObjectTable::new(objects.len()),
            ..GameState::default()
        };
        game.objects.records.copy_from_slice(objects);
        game.re2.player.floor = 2;
        game.re2.player.routine = [1, ROUTINE_WALK, 1, 0];
        let room = RoomState {
            sca: Some(Sca::default()),
            ..RoomState::default()
        };
        let mut player = crate::player::spawn(crate::state::RoomId::default(), &room);
        player.pos = [-22905, -3600, REST];
        player.angle = 1024;
        crate::character2::keep_old_boxes(&mut game, player.pos);
        (game, room, player)
    }

    /// A tick of walking into the object: a step of [`STEP`] toward -z, then
    /// `Om_move`, then `Om_trans`.
    fn walk(game: &mut GameState, room: &RoomState, player: &mut PlayerState) {
        player.pos[2] -= STEP;
        game.om_move(room, player);
        game.om_trans_old((player.pos, player.angle));
        crate::character2::keep_old_boxes(game, player.pos);
    }

    #[test]
    fn the_head_is_held_out_with_the_body() {
        let (mut game, room, mut player) = hall(&[statue(-13000)]);
        // The move placed the head on the body's step into the statue.
        player.pos[2] -= STEP;
        game.re2.player.neck.position = Some([-22905, -5900, REST - STEP - 30]);
        game.re2.player.neck.body = Some(player.pos);
        game.om_move(&room, &mut player);
        assert_eq!(player.pos[2], REST, "held out");
        assert_eq!(
            game.re2.player.neck.position,
            Some([-22905, -5900, REST - 30]),
            "the head is on the held-out body"
        );
    }

    #[test]
    fn a_push_moves_the_object_on_the_seventh_count() {
        let (mut game, room, mut player) = hall(&[statue(-13000)]);
        for count in 1..=5 {
            walk(&mut game, &room, &mut player);
            let record = game.objects.records[0];
            assert_eq!(record.push_cnt, count);
            assert!(!game.re2.pushing(), "no 0x2000 on count {count}");
            assert_eq!(record.pos[2], -13000);
            assert_eq!(player.pos, [-22905, -3600, REST], "held out");
        }
        walk(&mut game, &room, &mut player);
        assert_eq!(game.objects.records[0].push_cnt, 6);
        assert!(game.re2.pushing());
        assert_eq!(
            game.objects.records[0].pos[2], -13000,
            "the sixth step is undone"
        );
        assert_eq!(player.pos[2], REST);
        game.re2.player.routine = [1, ROUTINE_PUSH, 5, 0];
        walk(&mut game, &room, &mut player);
        let record = game.objects.records[0];
        assert_eq!(record.push_cnt, 7);
        assert!(game.re2.pushing());
        assert_eq!(record.pos[2], -13000 - STEP, "the player's step into it");
        assert_eq!(record.at[2], -13000 - STEP);
        assert_eq!(player.pos[2], REST - STEP, "the player keeps the step");
        walk(&mut game, &room, &mut player);
        assert_eq!(game.objects.records[0].push_cnt, 7);
        assert_eq!(game.objects.records[0].pos[2], -13000 - 2 * STEP);
    }

    #[test]
    fn a_push_into_a_wall_is_refused_and_resets_the_count() {
        // A wall record whose high z edge is one step and a unit beyond the
        // box's -z face: the step there touches it.
        let face = -13000 - 432;
        let wall = |z_far: i32| ScaRect {
            x: -24000,
            z: (z_far - 1000) as i16,
            width: 3000,
            depth: 1000,
            id: 0xFC80,
            kind: 0xF,
            floor: 0x1F,
        };
        for (z_far, refused) in [(face - STEP + 1, true), (face - STEP, false)] {
            let (mut game, mut room, mut player) = hall(&[statue(-13000)]);
            room.sca.as_mut().unwrap().records.push(wall(z_far));
            for _ in 0..5 {
                walk(&mut game, &room, &mut player);
            }
            walk(&mut game, &room, &mut player);
            let record = game.objects.records[0];
            assert_eq!(game.re2.pushing(), !refused, "wall's far edge at {z_far}");
            assert_eq!(record.push_cnt, if refused { 0 } else { 6 });
            assert_eq!(record.pos[2], -13000, "put back");
        }
    }

    #[test]
    fn only_walking_and_pushing_keep_the_count() {
        for routine in [0, 2, 3, 4, 1, 10] {
            let mut object = statue(-13000);
            object.push_cnt = 3;
            let (mut game, room, mut player) = hall(&[object]);
            player.pos[2] += 1000;
            game.re2.player.routine[1] = routine;
            game.om_move(&room, &mut player);
            let kept = matches!(routine, 1 | 10);
            assert_eq!(game.objects.records[0].push_cnt, if kept { 3 } else { 0 });
        }
    }

    #[test]
    fn two_touched_objects_push_neither() {
        let mut beside = statue(-13000);
        beside.pos[0] += 432 * 2 + 400;
        beside.set_ofs();
        beside.om_trans_old();
        let (mut game, room, mut player) = hall(&[statue(-13000), beside]);
        player.pos[0] = -22900 + 432 + 200;
        for _ in 0..8 {
            walk(&mut game, &room, &mut player);
        }
        assert!(!game.re2.pushing());
        assert_eq!(game.objects.records[0].push_cnt, 0);
        assert_eq!(game.objects.records[0].pos[2], -13000);
        assert_eq!(game.objects.records[1].pos[2], -13000);
    }

    #[test]
    fn an_object_without_0x80_is_never_pushed() {
        let mut fixed = statue(-13000);
        fixed.be_flg &= !BE_PUSHABLE;
        let (mut game, room, mut player) = hall(&[fixed]);
        for _ in 0..8 {
            walk(&mut game, &room, &mut player);
        }
        assert!(!game.re2.pushing());
        assert_eq!(game.objects.records[0].pos[2], -13000);
        assert_eq!(player.pos[2], REST);
    }

    #[test]
    fn a_push_into_another_object_is_refused() {
        // Another statue one step and a unit beyond the pushed one's -z face.
        let other = statue(-13000 - 864 - STEP + 1);
        let (mut game, room, mut player) = hall(&[statue(-13000), other]);
        for _ in 0..6 {
            walk(&mut game, &room, &mut player);
        }
        assert!(!game.re2.pushing());
        assert_eq!(game.objects.records[0].push_cnt, 0);
        assert_eq!(game.objects.records[0].pos[2], -13000, "x and z put back");
        assert_eq!(game.objects.records[1].pos[2], -13000 - 864 - STEP + 1);
    }

    #[test]
    fn om_lists_each_room_object() {
        let mut game = GameState {
            objects: ObjectTable::new(2),
            ..GameState::default()
        };
        game.objects.records[1] = statue(-13000);
        game.objects.records[1].floor = 2;
        game.objects.records[1].push_cnt = 7;
        assert_eq!(
            game.re2_objects_trace(3),
            "om=0:0,0,0,0,0,0,0,0;1:85,-22900,-3600,-13000,0,7,0,2;2:0,0,0,0,0,0,0,0"
        );
        assert_eq!(game.re2_objects_trace(0), "om=-");
    }

    /// A body in slot 0 whose `Atd[0]` box sits `box_ofs` from its position,
    /// as a licker's or an ivy's does.
    fn offset_body(at: [i32; 3], box_ofs: [i32; 3]) -> crate::scd2::host::Character {
        let mut body = crate::player::spawn(crate::state::RoomId::default(), &RoomState::default());
        body.pos = [at[0] - box_ofs[0], at[1] + 1000, at[2] - box_ofs[2]];
        let mut zombie = crate::zombie2::Zombie::default();
        zombie.atd = crate::zombie2::Atd {
            ofs_x: 672,
            ofs_y: -1000,
            at_w: 300,
            at_h: 1000,
            w: 300,
            at,
        };
        crate::scd2::host::Character {
            id: 0x22,
            model: 0x22,
            em_set: (0, 0, crate::scd2::host::NO_ENEMY_FLAG),
            arms: crate::scd2::host::Arms::default(),
            be_flg: 1,
            work: crate::player2::Work::default(),
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
        }
    }

    #[test]
    fn a_body_is_parted_by_where_its_box_was_drawn() {
        // The box came in from -z, 100 units this tick; the body's position
        // is 672 units +z of the box, inside the statue's reach.
        let (mut game, room, mut player) = hall(&[statue(-13000)]);
        player.pos[0] += 3000;
        let ofs = [0, 0, -672];
        let mut body = offset_body([-22900 + 720, -4600, -13800], ofs);
        game.re2.characters.0 = vec![Some(Box::new(body.clone()))];
        crate::character2::keep_old_boxes(&mut game, player.pos);
        body = *game.re2.characters.0[0].take().unwrap();
        body.zombie.as_mut().unwrap().atd.at[2] += 100;
        body.body.pos[2] += 100;
        game.re2.characters.0[0] = Some(Box::new(body));

        assert_eq!(game.om_move(&room, &mut player), None);
        let body = game.re2.characters.0[0].as_ref().unwrap();
        // Out along z, the way the box came in, and not along x, as the
        // body's own last position would have it.
        assert_eq!(
            body.zombie.as_ref().unwrap().atd.at,
            [-22900 + 720, -4600, -13000 - 432 - 300 - 1]
        );
        assert_eq!(body.body.pos[2], -13000 - 432 - 300 - 1 + 672);
    }

    #[test]
    fn the_draw_keeps_every_box_in_play_even_a_stopped_partners() {
        let mut game = GameState::default();
        let mut stopped = offset_body([500, -1000, 900], [0, 0, 672]);
        stopped.work.kind = crate::player2::KIND_NOT_MOVED;
        let mut out = offset_body([0, 0, 0], [0, 0, 0]);
        out.be_flg = 0;
        game.re2.characters.0 = vec![Some(Box::new(stopped)), Some(Box::new(out))];
        crate::character2::keep_old_boxes(&mut game, [100, 0, 200]);
        let kept = |slot: usize| game.re2.characters.0[slot].as_ref().unwrap().work.old_box;
        assert_eq!(
            kept(0),
            Some([500, -1000, 900]),
            "the box, not the position"
        );
        assert_eq!(kept(1), None, "out of play");
        assert_eq!(
            game.re2.player.old_box,
            Some([100, -i32::from(PLAYER_HEIGHT), 200])
        );
    }

    #[test]
    fn only_a_touched_falling_box_warns() {
        let mut far = statue(-20000);
        far.id = ID_BOX;
        let (mut game, room, mut player) = hall(&[statue(-13000), far]);
        walk(&mut game, &room, &mut player);
        let mut near = statue(-13000);
        near.id = ID_BOX;
        let (mut game2, room2, mut player2) = hall(&[statue(-20000), near]);
        assert_eq!(game.om_move(&room, &mut player), None, "nothing touches it");
        player2.pos[2] -= STEP;
        assert_eq!(
            game2.om_move(&room2, &mut player2),
            Some(1),
            "the player walks in"
        );
    }

    /// An object in play, at `pos` on `parent`.
    fn hung(parent: SuperParent, pos: [i32; 3]) -> ObjectRecord {
        let mut record = ObjectRecord {
            flag: crate::objects::OBJECT_FLAG_ACTIVE,
            be_flg: BE_ACTIVE,
            pos,
            ..ObjectRecord::default()
        };
        record.set_super_parent(parent);
        record
    }

    fn objects(records: &[ObjectRecord]) -> GameState {
        let mut game = GameState {
            is_re2_room: true,
            objects: ObjectTable::new(records.len()),
            ..GameState::default()
        };
        game.objects.records.copy_from_slice(records);
        game
    }

    fn on(parent: &Mat4x3, pos: [i32; 3]) -> Mat4x3 {
        let r = crate::anim::rotation_matrix(0, 0, 0);
        crate::anim::compose(parent, &Mat4x3 { r, t: pos })
    }

    #[test]
    fn om_trans_composes_a_hung_object_on_each_frames_joint() {
        // Room 115's `main_18`: object 8 on Claire's (enemy slot 0, work 2)
        // joint 14.
        let mut game = objects(&[hung(SuperParent::WorkJoint(2, 14), [-1, 365, -40])]);
        let mut joints = vec![IDENTITY; 15];
        joints[14] = crate::anim::entity_matrix([-24000, -800, -19000], 1024);
        game.re2.character_joints = vec![joints.clone()];
        game.om_trans_old(([0; 3], 0));
        assert_eq!(
            game.objects.records[0].world,
            on(&joints[14], [-1, 365, -40])
        );
        // The next frame's pose moves the hand, and the object with it.
        joints[14] = crate::anim::entity_matrix([-23900, -820, -19050], 1100);
        game.re2.character_joints = vec![joints.clone()];
        game.om_trans_old(([0; 3], 0));
        assert_eq!(
            game.objects.records[0].world,
            on(&joints[14], [-1, 365, -40])
        );
        // With no pose kept for the slot, the character's own matrix; with
        // no character, the identity.
        game.re2.character_joints.clear();
        game.om_trans_old(([0; 3], 0));
        assert_eq!(game.objects.records[0].world, on(&IDENTITY, [-1, 365, -40]));
        // The player's joint: the pose the tick composed.
        game.objects.records[0].set_super_parent(SuperParent::PlayerJoint(1));
        game.joint_worlds[0] = joints.clone();
        game.om_trans_old(([0; 3], 0));
        assert_eq!(
            game.objects.records[0].world,
            on(&joints[1], [-1, 365, -40])
        );
    }

    #[test]
    fn a_parent_in_a_later_slot_gives_last_frames_matrix() {
        let mut game = objects(&[
            hung(SuperParent::Object(1), [0, 100, 0]),
            hung(SuperParent::World, [1000, 0, 2000]),
            hung(SuperParent::Object(1), [0, 100, 0]),
        ]);
        game.om_trans_old(([0; 3], 0));
        let first = game.objects.records[1].world;
        game.objects.records[1].pos = [3000, 0, 4000];
        game.om_trans_old(([0; 3], 0));
        let second = game.objects.records[1].world;
        assert_eq!(second.t, [3000, 0, 4000]);
        // Slot 0 composes before slot 1 does: last frame's matrix.
        assert_eq!(game.objects.records[0].world, on(&first, [0, 100, 0]));
        // Slot 2 composes after it: this frame's.
        assert_eq!(game.objects.records[2].world, on(&second, [0, 100, 0]));
        // The frame after, slot 0 catches up.
        game.om_trans_old(([0; 3], 0));
        assert_eq!(game.objects.records[0].world, on(&second, [0, 100, 0]));
        // An object out of play isn't composed.
        game.objects.records[2].be_flg = BE_ACTIVE | BE_HIDDEN;
        game.objects.records[1].pos = [0; 3];
        game.om_trans_old(([0; 3], 0));
        assert_eq!(game.objects.records[2].world, on(&second, [0, 100, 0]));
    }

    #[test]
    fn room_409s_cut_12_turns_slot_4_about_z() {
        let records = [hung(SuperParent::World, [0; 3]); 6];
        for (room, cut, turned) in [((4, 9), 12, true), ((4, 9), 11, false), ((4, 8), 12, false)] {
            let mut game = objects(&records);
            game.id.stage = room.0;
            game.id.room = room.1;
            game.camera.current_cut = cut;
            game.om_trans_old(([0; 3], 0));
            let z = |slot: usize| game.objects.records[slot].rotation[2];
            assert_eq!(z(4), if turned { -40 } else { 0 }, "{room:?} cut {cut}");
            assert_eq!(z(3), 0);
            assert_eq!(z(5), 0);
        }
    }
}
