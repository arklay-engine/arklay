//! RE2's room objects in play: `Om_move` (`0x4CCD70`), which pushes the
//! pushable ones and holds the characters out of them, and `Om_trans`' copies
//! of where each was drawn.
//!
//! Not ported: `Om_move_box` and the falls of the `Id` 2 boxes (rooms 203,
//! 216, 306, 603 and F03), and `Oma_pull`, standing on an object.

use std::sync::atomic::{AtomicBool, Ordering};

use super::GameState;
use crate::character2::{PLAYER_HEIGHT, player_width};
use crate::objects::{
    CharBox, ObjectRecord, Oma, SCA_HIT_WALL, floor_of_height, oma_ck_front, oma_ck_oba, oma_ck_om,
    sca_ck_hit_om,
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
    pub fn om_move(&mut self, room: &RoomState, player: &mut PlayerState) {
        if self.re2.objects_stopped() {
            return;
        }
        self.re2.set_pushing(false);
        let keeps_count = matches!(self.re2.player.routine[1], ROUTINE_WALK | ROUTINE_PUSH);
        let width = player_width(self.id.player_flag);
        let ofs_y = -(PLAYER_HEIGHT as i16);
        let old = self.re2.player.old.unwrap_or(player.pos);
        let mut pl = CharBox::standing(player.pos, old, ofs_y, width, PLAYER_HEIGHT);
        pl.be_flg = self.re2.player.be_flg;

        let mut touched = 0;
        let mut candidate = None;
        for index in 0..self.objects.records.len() {
            let mut record = self.objects.records[index];
            if record.be_flg & BE_ACTIVE == 0 {
                continue;
            }
            if record.id == ID_BOX && !BOX_WARNED.swap(true, Ordering::Relaxed) {
                crate::warning!(
                    "warning: room object {index} is a falling box (Id 2): Om_move_box isn't ported"
                );
            }
            record.set_ofs();
            record.floor = floor_of_height(record.pos[1]);
            if !keeps_count {
                record.push_cnt = 0;
            }
            self.characters_oma(&mut record, Oma::Hold);
            if record.be_flg & BE_SHOVED == 0 {
                if oma_ck_oba(&mut pl, &mut record, Oma::Touch) {
                    touched += 1;
                    if self.re2.player.floor == record.floor
                        && oma_ck_front(pl.pos, player.angle, width, width, &record)
                    {
                        candidate = Some(index);
                    }
                }
            } else {
                oma_ck_oba(&mut pl, &mut record, Oma::Push);
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

        for record in &mut self.objects.records {
            if record.be_flg & BE_ACTIVE != 0 {
                oma_ck_oba(&mut pl, record, Oma::Hold);
            }
        }
        player.pos = pl.pos;
        self.entities[0].pos = player.pos;
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
            let pos = character.body.pos;
            let old = character.work.old.unwrap_or(pos);
            let mut chr = match character.zombie.as_ref() {
                Some(zombie) => {
                    let atd = zombie.atd;
                    let lift = i32::from(atd.ofs_y);
                    CharBox {
                        be_flg: 0,
                        pos,
                        at: atd.at,
                        old2: [old[0] as i16, (old[1] + lift) as i16, old[2] as i16],
                        ofs_y: atd.ofs_y,
                        at_w: atd.at_w,
                        at_h: atd.at_h,
                    }
                }
                None => {
                    let height = crate::player2::character_height(character.id);
                    CharBox::standing(
                        pos,
                        old,
                        -(height as i16),
                        character.body.radius as u16,
                        height,
                    )
                }
            };
            chr.be_flg = character.be_flg;
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

    /// `Om_trans`' copies as it draws: each drawn object's `Old` and `Old2`.
    pub fn om_trans_old(&mut self) {
        for record in &mut self.objects.records {
            if record.be_flg & BE_ACTIVE != 0 && record.be_flg & BE_HIDDEN == 0 {
                record.om_trans_old();
            }
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
        (game, room, player)
    }

    /// A tick of walking into the object: a step of [`STEP`] toward -z, then
    /// `Om_move`, then `Om_trans`.
    fn walk(game: &mut GameState, room: &RoomState, player: &mut PlayerState) {
        game.re2.player.old = Some(player.pos);
        player.pos[2] -= STEP;
        game.om_move(room, player);
        game.om_trans_old();
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
}
