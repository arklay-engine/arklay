//! The `e` of an RE2 enemy script: the fields of the original's work, and the
//! low-level calls behaviour is written with. See `lua-types/re2-enemy.lua`.

use mlua::{UserData, UserDataMethods};

use super::licker::Licker;
use super::{Atd, Em, PLAYER_BODY, direction_ck, goto, square_root0, surface, turn_within};
use crate::player2::{self, Bank};

/// One script call's view of its enemy, valid only for that call: the host's
/// `Lua::scope` seals the userdata so the script can't keep it.
pub(super) struct Api {
    em: *mut Em<'static, 'static, 'static>,
    /// Null on a zombie, which keeps its `var`s and `flags` on the `Zombie`.
    l: *mut Licker,
}

impl Api {
    pub(super) fn new(em: &mut Em<'_, '_, '_>, l: &mut Licker) -> Self {
        let em: *mut Em<'_, '_, '_> = em;
        Self { em: em.cast(), l }
    }

    pub(super) fn zombie(em: &mut Em<'_, '_, '_>) -> Self {
        let em: *mut Em<'_, '_, '_> = em;
        Self {
            em: em.cast(),
            l: std::ptr::null_mut(),
        }
    }

    fn em(&mut self) -> &mut Em<'static, 'static, 'static> {
        // SAFETY: `new` took these from live borrows that outlast the script
        // call this `Api` exists for, and `&mut self` makes this the only path
        // to them.
        unsafe { &mut *self.em }
    }

    fn l(&mut self) -> mlua::Result<&mut Licker> {
        // SAFETY: as in `em`; the `Licker` is out of the `Em` for the call.
        unsafe { self.l.as_mut() }
            .ok_or_else(|| mlua::Error::RuntimeError("this isn't a licker".to_owned()))
    }

    fn var(&mut self, index: i64) -> mlua::Result<&mut i32> {
        let vars: &mut [i32] = if self.l.is_null() {
            &mut self.em().z.vars[..]
        } else {
            &mut self.l()?.vars
        };
        usize::try_from(index)
            .ok()
            .and_then(|index| vars.get_mut(index))
            .ok_or_else(|| mlua::Error::RuntimeError(format!("no var {index}")))
    }

    fn flags(&mut self) -> &mut u16 {
        // SAFETY: as in `l`.
        match unsafe { self.l.as_mut() } {
            Some(l) => &mut l.flags,
            None => &mut self.em().z.flags,
        }
    }
}

/// The numbers scripts have for the original's motion tables.
fn bank(index: i64) -> mlua::Result<Bank> {
    match index {
        0 => Ok(Bank::Pld),
        1 => Ok(Bank::Weapon),
        2 => Ok(Bank::Room),
        _ => Err(mlua::Error::RuntimeError(format!("no bank {index}"))),
    }
}

/// A collision record a script names, from 1.
fn sca_record(e: &mut Api, record: i64) -> mlua::Result<&'static crate::state::ScaRect> {
    let room = e.em().mover.tick.room;
    room.sca
        .as_ref()
        .and_then(|sca| sca.records.get(usize::try_from(record - 1).ok()?))
        .ok_or_else(|| mlua::Error::RuntimeError(format!("no record {record}")))
}

fn parent(e: &mut Api) -> mlua::Result<&super::ParentView> {
    e.em()
        .world
        .parent
        .as_ref()
        .ok_or_else(|| mlua::Error::RuntimeError("no parent".to_owned()))
}

fn routine_index(index: i64) -> mlua::Result<usize> {
    usize::try_from(index)
        .ok()
        .filter(|index| *index < 4)
        .ok_or_else(|| mlua::Error::RuntimeError(format!("no routine {index}")))
}

impl UserData for Api {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // The fields the engine doesn't read.
        methods.add_method_mut("var", |_, e, index: i64| Ok(*e.var(index)?));
        methods.add_method_mut("set_var", |_, e, (index, value): (i64, i32)| {
            *e.var(index)? = value;
            Ok(())
        });

        // `Routine_0` to `Routine_3`.
        methods.add_method_mut("routine", |_, e, index: i64| {
            Ok(e.em().work().routine[routine_index(index)?])
        });
        methods.add_method_mut("set_routine", |_, e, (index, value): (i64, i64)| {
            e.em().work().routine[routine_index(index)?] = value as u8;
            Ok(())
        });
        methods.add_method_mut("routines", |_, e, r: (i64, i64, i64, i64)| {
            e.em().work().routine = [r.0 as u8, r.1 as u8, r.2 as u8, r.3 as u8];
            Ok(())
        });
        methods.add_method_mut("restore_routine", |_, e, ()| {
            let em = e.em();
            let last = em.z.last_routine;
            em.work().routine = last;
            Ok(())
        });
        methods.add_method_mut("go", |_, e, state: i64| {
            e.em().state(state as u8, 0);
            Ok(())
        });
        methods.add_method_mut("step", |_, e, ended: bool| {
            e.em().step(ended);
            Ok(())
        });

        // The animation.
        methods.add_method_mut("clip", |_, e, (clip, count, hokan): (i64, i64, i64)| {
            e.em().clip(clip as u8, count as u8, hokan as u8);
            Ok(())
        });
        methods.add_method_mut("move_no", |_, e, ()| Ok(e.em().work().move_no));
        methods.add_method_mut("move_cnt", |_, e, ()| Ok(e.em().work().move_cnt));
        methods.add_method_mut("set_move_cnt", |_, e, count: i64| {
            e.em().work().move_cnt = count as u8;
            Ok(())
        });
        methods.add_method_mut("joint", |_, e, (late, reverse): (i64, Option<bool>)| {
            let em = e.em();
            em.work().bank = Bank::Pld;
            Ok(em
                .mover
                .joint_move_late_reversed(late as u16, reverse.unwrap_or(false)))
        });
        methods.add_method_mut("spd", |_, e, ()| {
            e.em().spd(Bank::Pld);
            Ok(())
        });
        methods.add_method_mut("spd_reversed", |_, e, ()| {
            let em = e.em();
            em.work().bank = Bank::Pld;
            em.mover.spd_get_reversed(true);
            Ok(())
        });
        methods.add_method_mut("add_speed_xz", |_, e, ()| {
            e.em().mover.add_speed_xz();
            Ok(())
        });
        methods.add_method_mut("add_speed_xyz", |_, e, ()| {
            e.em().mover.add_speed_xyz();
            Ok(())
        });
        methods.add_method_mut("speed", |_, e, ()| {
            let [x, y, z] = e.em().work().spd;
            Ok((x, y, z))
        });
        methods.add_method_mut("set_speed", |_, e, (x, y, z): (i64, i64, i64)| {
            e.em().work().spd = [x as i16, y as i16, z as i16];
            Ok(())
        });
        // `Spd_base`, which `Spd_get` takes the next frame's place from.
        methods.add_method_mut("set_speed_base", |_, e, (x, y, z): (i64, i64, i64)| {
            e.em().work().spd_base = [x as i16, y as i16, z as i16];
            Ok(())
        });

        // The body.
        methods.add_method_mut("pos", |_, e, ()| {
            let [x, y, z] = e.em().mover.player.pos;
            Ok((x, y, z))
        });
        methods.add_method_mut("add_pos", |_, e, (x, y, z): (i32, i32, i32)| {
            let pos = &mut e.em().mover.player.pos;
            pos[0] += x;
            pos[1] += y;
            pos[2] += z;
            Ok(())
        });
        methods.add_method_mut("set_pos_y", |_, e, y: i32| {
            e.em().mover.player.pos[1] = y;
            Ok(())
        });
        methods.add_method_mut("angle", |_, e, ()| Ok(e.em().mover.player.angle));
        methods.add_method_mut("set_angle", |_, e, angle: i64| {
            e.em().mover.player.angle = angle as u16;
            Ok(())
        });
        methods.add_method_mut("goto_angle", |_, e, (to, rate): (i64, i32)| {
            let body = &mut *e.em().mover.player;
            body.angle = goto(body.angle, to as u16, rate);
            Ok(())
        });
        methods.add_method_mut("rotate", |_, e, (angle, x, z): (i64, i32, i32)| {
            let _ = e;
            Ok(player2::rotate(angle as u16, x, z))
        });
        methods.add_method_mut("be_flg", |_, e, ()| Ok(*e.em().be_flg));
        methods.add_method_mut("set_be_flg", |_, e, value: i64| {
            *e.em().be_flg = value as u32;
            Ok(())
        });
        methods.add_method_mut("kind", |_, e, ()| Ok(e.em().work().kind));
        methods.add_method_mut("slot", |_, e, ()| Ok(e.em().world.slot));
        methods.add_method_mut("clear_neck", |_, e, ()| {
            e.em().work().neck.flags = 0;
            Ok(())
        });
        // A point given on `part`'s matrix as this tick's frame poses it, or
        // nils without a pose of it (`drawn_part_point` reads the last
        // draw's).
        methods.add_method_mut(
            "part_point",
            |_, e, (part, x, y, z): (usize, i32, i32, i32)| {
                Ok(e.em()
                    .mover
                    .part_point(part, [x, y, z])
                    .map_or((None, None, None), |[x, y, z]| (Some(x), Some(y), Some(z))))
            },
        );
        methods.add_method_mut(
            "first_wall",
            |_, e, (x, y, z, radius): (i32, i32, i32, i32)| {
                let em = e.em();
                let floor = em.z.floor;
                let hit = em.mover.tick.room.sca.as_ref().and_then(|sca| {
                    crate::sca::first_hit(sca, super::MASK, [x, y, z], radius, floor)
                });
                Ok(hit.map_or((None, None, None, None), |record| {
                    (
                        Some(i32::from(record.x)),
                        Some(i32::from(record.z)),
                        Some(i32::from(record.width)),
                        Some(i32::from(record.depth)),
                    )
                }))
            },
        );

        // `EM_WORK`.
        methods.add_method_mut("life", |_, e, ()| Ok(e.em().z.life));
        methods.add_method_mut("set_life", |_, e, life: i16| {
            e.em().z.life = life;
            Ok(())
        });
        methods.add_method_mut("damage_cnt", |_, e, ()| Ok(e.em().z.damage_cnt));
        methods.add_method_mut("set_damage_cnt", |_, e, value: i64| {
            e.em().z.damage_cnt = value as u8;
            Ok(())
        });
        methods.add_method_mut("damage_no", |_, e, ()| Ok(e.em().z.damage_no));
        methods.add_method_mut("set_damage_no", |_, e, value: i64| {
            e.em().z.damage_no = value as u8;
            Ok(())
        });
        methods.add_method_mut("damage_flg", |_, e, ()| Ok(i64::from(e.em().z.damage_flg)));
        methods.add_method_mut("set_damage_flg", |_, e, value: i64| {
            e.em().z.damage_flg = value as _;
            Ok(())
        });
        methods.add_method_mut("sca_info", |_, e, ()| Ok(e.em().z.sca_info));
        methods.add_method_mut("set_died", |_, e, ()| {
            e.em().z.died = true;
            Ok(())
        });
        methods.add_method_mut("sound", |_, e, slot: i64| {
            let em = e.em();
            em.z.sounds.push((slot as u8, em.mover.player.pos));
            Ok(())
        });
        methods.add_method_mut("rnd", |_, e, ()| Ok(e.em().rnd()));
        // `At_em_no` is the player or their partner.
        methods.add_method_mut("touching", |_, e, ()| {
            let at = e.em().z.at_em_no;
            Ok(at == PLAYER_BODY || at == crate::scd2::host::SPLAYER_SLOT as u8)
        });

        // `Em_22`'s end of the work.
        methods.add_method_mut("flags", |_, e, ()| Ok(*e.flags()));
        methods.add_method_mut("set_flags", |_, e, flags: i64| {
            *e.flags() = flags as u16;
            Ok(())
        });
        methods.add_method_mut("reach", |_, e, ()| Ok(e.l()?.reach));
        methods.add_method_mut("set_reach", |_, e, reach: i16| {
            e.l()?.reach = reach;
            Ok(())
        });
        methods.add_method_mut("timer1", |_, e, ()| Ok(e.l()?.timer1));
        methods.add_method_mut("set_timer1", |_, e, timer: i16| {
            e.l()?.timer1 = timer;
            Ok(())
        });
        methods.add_method_mut("set_fading", |_, e, fading: i64| {
            e.l()?.fading = fading as u8;
            Ok(())
        });
        methods.add_method_mut("tongue", |_, e, ()| Ok(e.l()?.tongue));
        methods.add_method_mut("tongue_out", |_, e, ()| {
            e.l()?.tongue = 1;
            Ok(())
        });
        methods.add_method_mut(
            "set_atd",
            |_, e, (which, ofs_x, ofs_y, at_w, at_h, w): (i64, i16, i16, u16, u16, u16)| {
                let atd = Atd {
                    ofs_x,
                    ofs_y,
                    at_w,
                    at_h,
                    w,
                    at: [0; 3],
                };
                match which {
                    0 => e.em().z.atd = atd,
                    1 => e.l()?.head = atd,
                    _ => e.l()?.back = Some(atd),
                }
                Ok(())
            },
        );
        methods.add_method_mut("clear_back_atd", |_, e, ()| {
            e.l()?.back = None;
            Ok(())
        });

        // The surface it walks on (`Super_matrix`).
        methods.add_method_mut("super_add", |_, e, (x, y, z): (i64, i64, i64)| {
            e.l()?.super_add = [x as i16, y as i16, z as i16];
            Ok(())
        });
        methods.add_method_mut("init_super", |_, e, ()| {
            e.em().work().super_matrix = None;
            Ok(())
        });
        methods.add_method_mut(
            "super_rotate",
            |_, e, (angle, x, y, z): (i64, i32, i32, i32)| {
                let surface = e.em().work().super_matrix;
                let [x, y, z] = surface::rotate(surface, angle as u16, [x, y, z]);
                Ok((x, y, z))
            },
        );
        methods.add_method_mut("super_axis", |_, e, ()| {
            Ok(surface::axis(e.em().work().super_matrix))
        });
        methods.add_method_mut("super_plane", |_, e, ()| {
            Ok(surface::plane(e.em().work().super_matrix))
        });
        methods.add_method_mut("add_speed_super", |_, e, (turn_y, turn_z): (i64, i64)| {
            let em = e.em();
            let on = em.work().super_matrix;
            if on.is_none() && turn_y == 0 && turn_z == 0 {
                em.mover.add_speed_xyz();
                return Ok(());
            }
            let spd = em.work().spd.map(i32::from);
            let angle = em.mover.player.angle.wrapping_add(turn_y as u16);
            let turn = player2::rot_matrix([0, angle as i16, turn_z as i16]);
            let m = on.map_or(turn, |on| player2::mul_matrix(&on, &turn));
            let by = surface::apply(&m, spd);
            let body = &mut *em.mover.player;
            for (pos, by) in body.pos.iter_mut().zip(by) {
                *pos += i32::from(by as i16);
            }
            Ok(())
        });
        methods.add_method_mut(
            "goto_super",
            |_, e, (x, y, z, rate): (i32, i32, i32, i16)| {
                let em = e.em();
                let surface = em.work().super_matrix;
                let body = &mut *em.mover.player;
                body.angle = surface::goto_point(surface, body.angle, body.pos, [x, y, z], rate);
                Ok(())
            },
        );
        methods.add_method_mut("goto_angle_super", |_, e, (to, rate): (i32, i16)| {
            let em = e.em();
            let surface = em.work().super_matrix;
            let body = &mut *em.mover.player;
            body.angle = surface::goto_angle(surface, body.angle, to, rate);
            Ok(())
        });
        methods.add_method_mut(
            "direction",
            |_, _, (x0, z0, x1, z1): (i32, i32, i32, i32)| Ok(direction_ck(x0, z0, x1, z1)),
        );
        // `Approach`: whether it has got there.
        methods.add_method_mut("approach", |_, e, (x, z, rate): (i32, i32, i32)| {
            let body = &mut *e.em().mover.player;
            let [old_x, _, old_z] = body.pos;
            let angle = direction_ck(old_x, old_z, x, z);
            let (dx, dz) = player2::rotate(angle, rate, 0);
            let (new_x, new_z) = (old_x + dx, old_z + dz);
            if (new_x - x < 0) == (old_x - x < 0) && (new_z - z < 0) == (old_z - z < 0) {
                body.pos[0] = new_x;
                body.pos[2] = new_z;
                return Ok(false);
            }
            body.pos[0] = x;
            body.pos[2] = z;
            Ok(true)
        });
        // `Approach_one`, on its height.
        methods.add_method_mut("approach_y", |_, e, (y, rate): (i32, i32)| {
            let body = &mut *e.em().mover.player;
            let old = body.pos[1];
            let new = if old - y < 0 { old + rate } else { old - rate };
            if (new - y < 0) == (old - y < 0) {
                body.pos[1] = new;
                return Ok(false);
            }
            body.pos[1] = y;
            Ok(true)
        });

        // The room's collision records, which scripts name by number from 1.
        methods.add_method_mut("floor", |_, e, ()| Ok(e.em().z.floor));
        methods.add_method_mut(
            "sca_list",
            |_, e, (x, z, floor, mask, radius): (i32, i32, i32, i64, Option<i32>)| {
                let room = e.em().mover.tick.room;
                Ok(room.sca.as_ref().map_or_else(Vec::new, |sca| {
                    let floor = (floor & 0x1F) as u8;
                    let radius = radius.unwrap_or(10);
                    let list = crate::sca::list_at(sca, mask as u16, x, z, radius, floor);
                    list.into_iter().map(|record| record + 1).collect()
                }))
            },
        );
        methods.add_method_mut("sca_all", |_, e, (areas, mask): (i64, i64)| {
            let room = e.em().mover.tick.room;
            Ok(room.sca.as_ref().map_or_else(Vec::new, |sca| {
                let list = crate::sca::list_all(sca, areas as u16, mask as u16);
                list.into_iter().map(|record| record + 1).collect()
            }))
        });
        methods.add_method_mut("sca_rect", |_, e, record: i64| {
            let rect = sca_record(e, record)?;
            Ok((
                i32::from(rect.x),
                i32::from(rect.z),
                i32::from(rect.width),
                i32::from(rect.depth),
                rect.id,
                rect.floor,
            ))
        });
        // `Sca_ck_info` with no push: its answer, and its list as `sca_list`'s.
        methods.add_method_mut(
            "sca_ck_info",
            |_, e, (x, z, floor, mask, radius): (i32, i32, i32, i64, i32)| {
                let room = e.em().mover.tick.room;
                Ok(room.sca.as_ref().map_or_else(
                    || (0, Vec::new()),
                    |sca| {
                        let floor = (floor & 0x1F) as u8;
                        let mask = mask as u16;
                        let list = crate::sca::list_at(sca, mask, x, z, radius, floor);
                        (
                            crate::sca::info_flags(sca, mask, x, z, radius, floor),
                            list.into_iter().map(|record| record + 1).collect(),
                        )
                    },
                ))
            },
        );
        methods.add_method_mut("sca_side", |_, e, (record, x, z): (i64, i32, i32)| {
            Ok(crate::sca::side_of(sca_record(e, record)?, x, z))
        });
        methods.add_method_mut("sca_edge", |_, e, (record, side): (i64, i64)| {
            Ok(crate::sca::edge_of(sca_record(e, record)?, side as u8))
        });
        methods.add_method_mut(
            "sca_face",
            |_, e, (record, x, y, z): (i64, i32, i32, i32)| {
                let plane = surface::plane(e.em().work().super_matrix);
                Ok(crate::sca::face_of(
                    sca_record(e, record)?,
                    [x, y, z],
                    plane,
                ))
            },
        );
        methods.add_method_mut(
            "sca_onto_edge",
            |_, e, (record, x, y, z): (i64, i32, i32, i32)| {
                let plane = surface::plane(e.em().work().super_matrix);
                let [x, y, z] = crate::sca::onto_edge(sca_record(e, record)?, [x, y, z], plane);
                Ok((x, y, z))
            },
        );
        // `Sca_ck_hit`'s two answers `FUN_004654f0` reads: the surface over the
        // point and the one under it.
        methods.add_method_mut("sca_hit", |_, e, (x, y, z, mask): (i32, i32, i32, i64)| {
            let room = e.em().mover.tick.room;
            Ok(room.sca.as_ref().map_or((0, 0), |sca| {
                let ground = crate::sca::ground_at(sca, [x, y, z], 0, mask as u16);
                let over = if ground.blocked || ground.under_record {
                    ground.ceiling
                } else {
                    ground.height
                };
                (over, ground.below.max(ground.above))
            }))
        });
        // `Sca_ck_hit` of a point grown by `radius` without the room's
        // objects: whether `DWORD_00695e84` is raised, by a record that
        // holds the point or one that starts above it.
        methods.add_method_mut(
            "sca_struck",
            |_, e, (x, y, z, radius, mask): (i32, i32, i32, i32, i64)| {
                let room = e.em().mover.tick.room;
                Ok(room.sca.as_ref().is_some_and(|sca| {
                    let ground = crate::sca::ground_at(sca, [x, y, z], radius, mask as u16);
                    ground.blocked || ground.under_record
                }))
            },
        );
        methods.add_method_mut("init_parts", |_, e, ()| {
            let pos = e.em().mover.player.pos;
            let l = e.l()?;
            l.part0 = pos;
            l.part1 = pos;
            Ok(())
        });

        // The tongue's one owner in a room, `DWORD_006894f8`.
        methods.add_method_mut("tongue_owner", |_, e, ()| Ok(*e.em().world.tongue_owner));
        methods.add_method_mut("set_tongue_owner", |_, e, owner: i64| {
            *e.em().world.tongue_owner = owner as u8;
            Ok(())
        });

        // Turning, and how the player lies.
        methods.add_method_mut("distance", |_, e, ()| Ok(e.em().distance));
        methods.add_method_mut("distance_now", |_, e, ()| {
            let em = e.em();
            let body = em.mover.player.pos;
            let [x, _, z] = em.world.player;
            let (dx, dz) = (x - body[0], z - body[2]);
            Ok(square_root0(
                dx.wrapping_mul(dx).wrapping_add(dz.wrapping_mul(dz)),
            ))
        });
        methods.add_method_mut("goto_player", |_, e, rate: i32| {
            e.em().goto_player(rate);
            Ok(())
        });
        methods.add_method_mut("goto_dest", |_, e, rate: i32| {
            e.em().goto_dest(rate);
            Ok(())
        });
        methods.add_method_mut("turn_to_player", |_, e, rate: i32| {
            Ok(e.em().turn_to_player(rate))
        });
        // The turn to the player, `0..0x1000`.
        methods.add_method_mut("angle_to_player", |_, e, ()| {
            let em = e.em();
            let body = em.mover.player.pos;
            let [x, _, z] = em.world.player;
            Ok(direction_ck(body[0], body[2], x, z).wrapping_sub(em.mover.player.angle) & 0xFFF)
        });
        methods.add_method_mut("player_turn_here", |_, e, rate: i32| {
            let em = e.em();
            let [x, _, z] = em.mover.player.pos;
            let [from_x, _, from_z] = em.world.player;
            let target = direction_ck(from_x, from_z, x, z);
            Ok(turn_within(em.world.player_angle, target, rate))
        });
        methods.add_method_mut("cdir_player", |_, e, ()| Ok(e.em().cdir_player()));

        // The player.
        methods.add_method_mut("player_life", |_, e, ()| Ok(*e.em().world.player_life));
        methods.add_method_mut("set_player_life", |_, e, life: i16| {
            *e.em().world.player_life = life;
            Ok(())
        });
        methods.add_method_mut("player_angle", |_, e, ()| Ok(e.em().world.player_angle));
        methods.add_method_mut("pl_routine", |_, e, index: i64| {
            Ok(e.em().world.pl.routine[routine_index(index)?])
        });
        methods.add_method_mut("pl_set_routine", |_, e, r: (i64, i64, i64, i64)| {
            e.em().world.pl.routine = [r.0 as u8, r.1 as u8, r.2 as u8, r.3 as u8];
            Ok(())
        });
        methods.add_method_mut("pl_damage_cnt", |_, e, ()| Ok(e.em().world.pl.damage_cnt));
        // The player's `Spd_x`, `Spd_y` and `Spd_z`.
        methods.add_method_mut("pl_speed", |_, e, ()| {
            let [x, y, z] = e.em().world.pl.spd;
            Ok((x, y, z))
        });
        // The player's `pSub1_kan_t_ptr` and `pSub1_seq_t_ptr` become this
        // body's (its model's second bank), for `Pl_damage`'s falls.
        // The player's `Move_no`, `Move_cnt` and `Hokan_flg` on `pSub1`'s
        // clips, and their speed.
        methods.add_method_mut(
            "pl_set_motion",
            |_,
             e,
             (clip, count, hokan, x, y, z, neck): (i64, i64, i64, i64, i64, i64, Option<i64>)| {
                let pl = &mut *e.em().world.pl;
                pl.neck.flags |= neck.unwrap_or(0) as u8;
                pl.bank = player2::Bank::Held;
                pl.move_no = clip as u8;
                pl.move_cnt = count as u8;
                pl.hokan = hokan as u8;
                pl.spd = [x as i16, y as i16, z as i16];
                Ok(())
            },
        );
        methods.add_method_mut("pl_set_sub1", |_, e, ()| {
            e.em().world.sub1 = true;
            Ok(())
        });
        methods.add_method_mut("pl_hold", |_, e, ()| {
            let world = &mut *e.em().world;
            world.pl.held_by = Some(world.slot);
            Ok(())
        });
        methods.add_method_mut("pl_held_kill", |_, e, ()| {
            let pl = &mut *e.em().world.pl;
            pl.held_kill = true;
            pl.die_held_back = false;
            Ok(())
        });
        methods.add_method_mut("pl_hold_back_die", |_, e, ()| {
            e.em().world.pl.die_held_back = true;
            Ok(())
        });
        methods.add_method_mut("pl_life_down", |_, e, (down, low): (i32, Option<bool>)| {
            Ok(e.em().pl_life_down(down, low.unwrap_or(false)))
        });
        methods.add_method_mut("player_root_point", |_, e, (x, y, z): (i32, i32, i32)| {
            let point = [x, y, z];
            let at = e.em().world.pl.root.as_ref().map(|root| {
                std::array::from_fn::<i32, 3, _>(|axis| {
                    let turned: i32 = (0..3)
                        .map(|index| i32::from(root.m[axis][index]) * point[index])
                        .sum();
                    root.t[axis] + (turned >> 12)
                })
            });
            Ok(at.map_or((None, None, None), |[x, y, z]| (Some(x), Some(y), Some(z))))
        });

        // `CompM(Workm, M).t` for the player's part `part` as the last tick
        // left it (`Work::parts`), with `M.t` the point: nils without a pose
        // of it. Part 0 is `player_root_point`'s.
        methods.add_method_mut(
            "player_part_point",
            |_, e, (part, x, y, z): (usize, i32, i32, i32)| {
                let point = [x, y, z];
                let at = e
                    .em()
                    .world
                    .pl
                    .parts
                    .get(part)
                    .copied()
                    .flatten()
                    .map(|matrix| {
                        std::array::from_fn::<i32, 3, _>(|axis| {
                            let turned: i32 = (0..3)
                                .map(|index| i32::from(matrix.m[axis][index]) * point[index])
                                .sum();
                            matrix.t[axis] + (turned >> 12)
                        })
                    });
                Ok(at.map_or((None, None, None), |[x, y, z]| (Some(x), Some(y), Some(z))))
            },
        );
        // The cockroaches' globals (`Re2State::swarm`), 0 to 3.
        methods.add_method_mut("swarm", |_, e, index: usize| {
            e.em()
                .world
                .swarm
                .get(index)
                .copied()
                .map(i64::from)
                .ok_or_else(|| mlua::Error::RuntimeError(format!("no swarm word {index}")))
        });
        methods.add_method_mut("set_swarm", |_, e, (index, value): (usize, i64)| {
            let word = e
                .em()
                .world
                .swarm
                .get_mut(index)
                .ok_or_else(|| mlua::Error::RuntimeError(format!("no swarm word {index}")))?;
            *word = value as u32;
            Ok(())
        });

        // The rest of the game.
        methods.add_method_mut("weapon", |_, e, ()| Ok(e.em().world.weapon));
        methods.add_method_mut("noise", |_, e, ()| Ok(e.em().world.noise));
        methods.add_method_mut("easy", |_, e, ()| Ok(e.em().world.easy));
        methods.add_method_mut("common_a4", |_, e, ()| Ok(e.em().world.common_a4));

        // `Em_Zombie`'s end of the work: `Timer0` to `Timer3`, which keep their
        // own widths.
        methods.add_method_mut("timer", |_, e, index: i64| {
            let timers = e.em().z.timers;
            match index {
                0 => Ok(i64::from(timers.0)),
                1 => Ok(i64::from(timers.1)),
                2 => Ok(i64::from(timers.2)),
                3 => Ok(i64::from(timers.3)),
                _ => Err(mlua::Error::RuntimeError(format!("no timer {index}"))),
            }
        });
        methods.add_method_mut("set_timer", |_, e, (index, value): (i64, i64)| {
            let timers = &mut e.em().z.timers;
            match index {
                0 => timers.0 = value as u16,
                1 => timers.1 = value as u16,
                2 => timers.2 = value as u8,
                3 => timers.3 = value as i8,
                _ => return Err(mlua::Error::RuntimeError(format!("no timer {index}"))),
            }
            Ok(())
        });
        methods.add_method_mut("last_routine", |_, e, index: i64| {
            Ok(e.em().z.last_routine[routine_index(index)?])
        });
        methods.add_method_mut("d_life", |_, e, index: usize| {
            let d_life = e.em().z.d_life;
            d_life
                .get(index)
                .copied()
                .ok_or_else(|| mlua::Error::RuntimeError(format!("no d_life {index}")))
        });
        methods.add_method_mut("set_d_life", |_, e, value: i64| {
            e.em().z.d_life = [value as u8; 3];
            Ok(())
        });
        methods.add_method_mut("shot_high", |_, e, ()| Ok(e.em().z.shot_high));
        methods.add_method_mut("in_sight", |_, e, ()| {
            Ok(e.em().z.status & super::STATUS_IN_SIGHT != 0)
        });
        methods.add_method_mut("at_em_no", |_, e, ()| Ok(e.em().z.at_em_no));
        methods.add_method_mut("knock", |_, e, slot: i64| {
            e.em().world.knocks.push(slot as u8);
            Ok(())
        });
        // `Atd[0]`'s size, leaving where it stands.
        methods.add_method_mut("body_box", |_, e, ()| {
            let atd = e.em().z.atd;
            Ok((atd.ofs_y, atd.at_w, atd.at_h, atd.w))
        });
        methods.add_method_mut(
            "set_body_box",
            |_, e, (ofs_y, at_w, at_h, w): (i16, u16, u16, u16)| {
                let atd = &mut e.em().z.atd;
                atd.ofs_y = ofs_y;
                atd.at_w = at_w;
                atd.at_h = at_h;
                atd.w = w;
                Ok(())
            },
        );
        methods.add_method_mut("pool", |_, e, ()| Ok(e.em().z.pool));
        methods.add_method_mut("set_pool", |_, e, pool: i64| {
            e.em().z.pool = pool as u16;
            Ok(())
        });
        methods.add_method_mut("charred", |_, e, ()| Ok(e.em().z.charred));
        methods.add_method_mut("set_charred", |_, e, charred: i64| {
            e.em().z.charred = charred as u8;
            Ok(())
        });
        methods.add_method_mut("set_burnt", |_, e, burnt: bool| {
            e.em().z.burnt = burnt;
            Ok(())
        });
        methods.add_method_mut("model_type", |_, e, ()| Ok(e.em().z.model_type));
        methods.add_method_mut("set_skin", |_, e, skin: i64| {
            e.em().z.skin = skin as u8;
            Ok(())
        });
        methods.add_method_mut(
            "light_flame",
            |_, e, (size, part, x, y, z): (u16, u8, i32, i32, i32)| {
                e.em().light_flame(size, part, [x, y, z]);
                Ok(())
            },
        );

        // The work, as `Em_Zombie` uses it.
        methods.add_method_mut("set_kind", |_, e, kind: i64| {
            e.em().work().kind = kind as u16;
            Ok(())
        });
        methods.add_method_mut("set_move_no", |_, e, clip: i64| {
            e.em().work().move_no = clip as u8;
            Ok(())
        });
        methods.add_method_mut("hokan", |_, e, ()| Ok(e.em().work().hokan));
        methods.add_method_mut("set_hokan", |_, e, hokan: i64| {
            e.em().work().hokan = hokan as u8;
            Ok(())
        });
        methods.add_method_mut("neck_flags", |_, e, ()| Ok(e.em().work().neck.flags));
        methods.add_method_mut("set_neck_flags", |_, e, flags: i64| {
            e.em().work().neck.flags = flags as _;
            Ok(())
        });
        methods.add_method_mut("set_lean", |_, e, lean: i64| {
            e.em().work().lean = lean as i16;
            Ok(())
        });
        methods.add_method_mut("set_sce_flg", |_, e, flags: i64| {
            e.em().work().sce_flg = flags as u16;
            Ok(())
        });
        methods.add_method_mut("sce_free", |_, e, index: usize| {
            let free = e.em().work().sce_free;
            free.get(index)
                .copied()
                .ok_or_else(|| mlua::Error::RuntimeError(format!("no sce_free {index}")))
        });
        methods.add_method_mut("set_sce_free", |_, e, (index, value): (usize, i64)| {
            let free = e.em().work().sce_free.get_mut(index);
            *free.ok_or_else(|| mlua::Error::RuntimeError(format!("no sce_free {index}")))? =
                value as i16;
            Ok(())
        });
        methods.add_method_mut("set_parts0_y", |_, e, scale: i32| {
            e.em().work().set_parts0_y(scale);
            Ok(())
        });
        methods.add_method_mut("hidden_parts", |_, e, ()| Ok(e.em().work().hidden_parts));
        methods.add_method_mut("set_hidden_parts", |_, e, parts: i64| {
            e.em().work().hidden_parts = parts as u32;
            Ok(())
        });
        // A part the original throws away as a part of its own, just no longer
        // drawn.
        methods.add_method_mut("hide_part", |_, e, part: i64| {
            player2::set_part_flags(e.em().work(), part as u8, 0);
            Ok(())
        });
        // `Sa_parts_allmv`: the part's children placed by its morph's weights,
        // as the model's morph data has them.
        methods.add_method_mut(
            "morph",
            |_, e, (part, weights): (u8, mlua::Variadic<i16>)| {
                let em = e.em();
                let Some(morph) = em.mover.tick.morphs.iter().find(|morph| morph.part == part)
                else {
                    return Ok(());
                };
                let joints = morph.joint_offsets(&weights);
                let offsets = &mut em.work().joint_offsets;
                offsets.retain(|(index, _)| joints.iter().all(|(child, _)| child != index));
                offsets.extend(joints);
                Ok(())
            },
        );
        // Where `pSin_parts_ptr[part]` hangs from its parent.
        methods.add_method_mut("part_offset", |_, e, part: usize| {
            let [x, y, z] = e
                .em()
                .mover
                .tick
                .skeleton
                .relative
                .get(part)
                .map_or([0; 3], |at| at.map(i32::from));
            Ok((x, y, z))
        });

        // The motion, on any of the banks.
        methods.add_method_mut("set_bank", |_, e, index: i64| {
            e.em().work().bank = bank(index)?;
            Ok(())
        });
        methods.add_method_mut(
            "joint_bank",
            |_, e, (index, late, reverse): (i64, i64, Option<bool>)| {
                let em = e.em();
                let bank = bank(index)?;
                Ok(match reverse {
                    Some(reverse) => {
                        em.work().bank = bank;
                        em.mover.joint_move_late_reversed(late as u16, reverse)
                    }
                    None => em.joint(bank, late as u16),
                })
            },
        );
        methods.add_method_mut("spd_bank", |_, e, index: i64| {
            let bank = bank(index)?;
            e.em().spd(bank);
            Ok(())
        });
        methods.add_method_mut("speed_x", |_, e, ()| Ok(e.em().work().spd[0]));
        methods.add_method_mut("set_speed_x", |_, e, x: i64| {
            e.em().work().spd[0] = x as i16;
            Ok(())
        });
        methods.add_method_mut("set_speed_z", |_, e, z: i64| {
            e.em().work().spd[2] = z as i16;
            Ok(())
        });
        methods.add_method_mut("add_speed_xz_at", |_, e, dir: i64| {
            e.em().mover.add_speed_xz_at(dir as u16);
            Ok(())
        });
        methods.add_method_mut("skip_frame", |_, e, ()| {
            e.em().mover.skip_frame();
            Ok(())
        });
        methods.add_method_mut("now_seq", |_, e, ()| Ok(e.em().mover.now_seq()));
        methods.add_method_mut("root_y", |_, e, ()| Ok(e.em().mover.root_y()));
        methods.add_method_mut("foot_position", |_, e, ()| {
            Ok(e.em()
                .mover
                .foot_position()
                .map_or((None, None, None), |[x, y, z]| (Some(x), Some(y), Some(z))))
        });
        methods.add_method_mut(
            "foot_set",
            |_, e, (x, y, z): (Option<i32>, Option<i32>, Option<i32>)| {
                let planted = x.zip(y).zip(z).map(|((x, y), z)| [x, y, z]);
                e.em().mover.foot_set(planted);
                Ok(())
            },
        );
        methods.add_method_mut("null_pos_set", |_, e, turn: i64| {
            e.em().mover.null_pos_set(turn as u16);
            Ok(())
        });
        // `Spl_Scenario`, on the model's own bank.
        methods.add_method_mut("scenario", |_, e, ()| {
            let em = e.em();
            em.work().bank = Bank::Pld;
            em.mover.scenario();
            Ok(())
        });

        // Turning.
        methods.add_method_mut("goto_point", |_, e, (x, z, rate): (i32, i32, i32)| {
            e.em().goto(x, z, rate);
            Ok(())
        });
        // `Goto01_ck`: 0 once within `rate` of `angle`, or the way to turn.
        methods.add_method_mut("turn_to", |_, e, (angle, rate): (i64, i32)| {
            Ok(turn_within(e.em().mover.player.angle, angle as u16, rate))
        });
        methods.add_method_mut("direction_to_player", |_, e, ()| {
            let em = e.em();
            let body = em.mover.player.pos;
            let [x, _, z] = em.world.player;
            Ok(direction_ck(body[0], body[2], x, z))
        });
        // `Dir_pos_ck`.
        methods.add_method_mut("player_within", |_, e, (angle, within): (i64, i32)| {
            Ok(e.em().player_within(angle as u16, within))
        });

        // Parts leaving the body.
        methods.add_method_mut(
            "throw_part",
            |_, e, (part, along, up, gravity): (usize, i16, i16, i16)| {
                e.em().throw_part(part, along, up, gravity);
                Ok(())
            },
        );
        methods.add_method_mut(
            "blow_part",
            |_,
             e,
             (part, dir, speed, up, gravity): (
                usize,
                i32,
                Option<i16>,
                Option<i16>,
                Option<i16>,
            )| {
                e.em().blow_part(
                    part,
                    dir as u16,
                    speed.unwrap_or(800),
                    up.unwrap_or(0),
                    gravity.unwrap_or(10),
                );
                Ok(())
            },
        );
        methods.add_method_mut(
            "fling_part",
            |_, e, (part, dir, along, gravity, rest): (usize, i32, i16, i16, i32)| {
                e.em()
                    .fling_part(part, dir as u16, along, gravity, rest as i16);
                Ok(())
            },
        );
        // Both arms, as `FUN_00458d40` reads them before it starts its clip.
        methods.add_method_mut("keep_arms", |_, e, ()| {
            let em = e.em();
            em.kept_arms = [em.mover.part_matrix(9), em.mover.part_matrix(12)];
            Ok(())
        });
        methods.add_method_mut(
            "throw_kept_arm",
            |_, e, (part, along, up, gravity): (usize, i16, i16, i16)| {
                let em = e.em();
                let kept = em.kept_arms[usize::from(part != 9)];
                em.throw_matrix(part, kept, along, up, gravity);
                Ok(())
            },
        );
        methods.add_method_mut("pull_part", |_, e, (part, behind): (usize, bool)| {
            e.em().start_pull(part, behind);
            Ok(())
        });

        // The player, in a grab.
        methods.add_method_mut("same_floor", |_, e, ()| {
            let em = e.em();
            Ok(em.z.floor == em.world.player_floor)
        });
        methods.add_method_mut("player_id", |_, e, ()| Ok(e.em().world.player_id));
        methods.add_method_mut("gacha", |_, e, ()| Ok(e.em().world.gacha));
        methods.add_method_mut("pl_set_damage_cnt", |_, e, value: i64| {
            e.em().world.pl.damage_cnt = value as u8;
            Ok(())
        });
        methods.add_method_mut("pl_set_routine_at", |_, e, (index, value): (i64, i64)| {
            e.em().world.pl.routine[routine_index(index)?] = value as u8;
            Ok(())
        });
        methods.add_method_mut("pl_or_neck_flags", |_, e, bits: i64| {
            e.em().world.pl.neck.flags |= bits as u8;
            Ok(())
        });
        methods.add_method_mut("pl_or_be_flg", |_, e, bits: i64| {
            e.em().world.pl.be_flg |= bits as u32;
            Ok(())
        });
        methods.add_method_mut("pl_clear_be_flg", |_, e, bits: i64| {
            e.em().world.pl.be_flg &= !(bits as u32);
            Ok(())
        });
        methods.add_method_mut("pl_set_timer", |_, e, (index, value): (usize, i64)| {
            let timer = e.em().world.pl.timers.get_mut(index);
            *timer.ok_or_else(|| mlua::Error::RuntimeError(format!("no timer {index}")))? =
                value as u16;
            Ok(())
        });
        // `Set_base_pos`, for the player's hold and for a body a script names.
        methods.add_method_mut("pl_set_base", |_, e, ()| {
            let em = e.em();
            em.world.pl.base = em.mover.set_base_pos();
            Ok(())
        });
        methods.add_method_mut("grab_body", |_, e, ()| {
            let em = e.em();
            em.z.grabbed = Some(em.mover.set_base_pos());
            Ok(())
        });
        methods.add_method_mut("let_go", |_, e, ()| {
            e.em().let_go();
            Ok(())
        });

        // The room.
        // `Root_ck`, with the target its caller gives.
        methods.add_method_mut("root_ck", |_, e, (x, z): (i32, i32)| {
            Ok(e.em().root_ck_to([x as i16, z as i16], None))
        });
        methods.add_method_mut("root_ck_area", |_, e, area: i64| {
            Ok(e.em().root_ck_to([0, 0], Some(area as u8)))
        });
        // `Rnd_area`, or -1 in a room without guide areas.
        methods.add_method_mut("rnd_area", |_, e, ()| {
            let em = e.em();
            let count = em.world.guides.len() as i32;
            if count == 0 {
                return Ok(-1);
            }
            Ok(i32::from(em.world.rnd.draw()) % count)
        });
        methods.add_method_mut("area_count", |_, e, ()| Ok(e.em().world.guides.len()));
        // `Goto00_ck`: 0 once within `rate`, or the way to turn.
        // `Hit_ck_point4` of the body's place on the current cut's view
        // (`DWORD_00988630`); true when the cut has none.
        methods.add_method_mut("in_cut_view", |_, e, ()| {
            let em = e.em();
            let [x, _, z] = em.mover.player.pos;
            Ok(em.mover.tick.room.in_cut_view(x, z).unwrap_or(true))
        });
        methods.add_method_mut("goto_ck", |_, e, (x, z, rate): (i32, i32, i32)| {
            let body = &*e.em().mover.player;
            let target = direction_ck(body.pos[0], body.pos[2], x, z);
            Ok(turn_within(body.angle, target, rate))
        });
        methods.add_method_mut("add_speed_xyz_at", |_, e, dir: i64| {
            let em = e.em();
            em.mover.add_speed_xz_at(dir as u16);
            let up = i32::from(em.work().spd[1]);
            em.mover.player.pos[1] += up;
            Ok(())
        });
        methods.add_method_mut("player_pos", |_, e, ()| {
            let [x, y, z] = e.em().world.player;
            Ok((x, y, z))
        });
        // `Goto01` and `Goto00` on the player, from where this tick has turned it.
        methods.add_method_mut("player_goto_angle", |_, e, (to, rate): (i64, i32)| {
            let world = &mut *e.em().world;
            let angle = world.turn_player.unwrap_or(world.player_angle);
            world.turn_player = Some(player2::goto(angle, to as u16, rate));
            Ok(())
        });
        methods.add_method_mut(
            "player_goto_point",
            |_, e, (x, z, rate): (i32, i32, i32)| {
                let world = &mut *e.em().world;
                let angle = world.turn_player.unwrap_or(world.player_angle);
                let [from_x, _, from_z] = world.player;
                let target = direction_ck(from_x, from_z, x, z);
                world.turn_player = Some(player2::goto(angle, target, rate));
                Ok(())
            },
        );
        methods.add_method_mut("player_floor", |_, e, ()| Ok(e.em().world.player_floor));
        // `Approach` on the player's position.
        methods.add_method_mut("set_player_xz", |_, e, (x, z): (i32, i32)| {
            let world = &mut *e.em().world;
            world.player_moved = true;
            world.player[0] = x;
            world.player[2] = z;
            Ok(())
        });
        methods.add_method_mut("player_approach", |_, e, (x, z, rate): (i32, i32, i32)| {
            let world = &mut *e.em().world;
            let [old_x, _, old_z] = world.player;
            let angle = direction_ck(old_x, old_z, x, z);
            let (dx, dz) = player2::rotate(angle, rate, 0);
            let (new_x, new_z) = (old_x + dx, old_z + dz);
            world.player_moved = true;
            let stays = (new_x - x < 0) == (old_x - x < 0) && (new_z - z < 0) == (old_z - z < 0);
            let (x, z) = if stays { (new_x, new_z) } else { (x, z) };
            world.player[0] = x;
            world.player[2] = z;
            Ok(!stays)
        });
        // `Old_x` and `Old_z`: where the player ended the last tick.
        methods.add_method_mut("player_old", |_, e, ()| {
            let world = &*e.em().world;
            let [x, _, z] = world.pl.old.unwrap_or(world.player);
            Ok((x, z))
        });
        methods.add_method_mut("hypot", |_, e, (dx, dz): (i32, i32)| {
            let _ = e;
            Ok(square_root0(
                dx.wrapping_mul(dx).wrapping_add(dz.wrapping_mul(dz)),
            ))
        });
        methods.add_method_mut("hand", |_, e, hand: usize| {
            let at = super::MRX_HANDS + hand.min(1) * 3;
            let vars = &e.em().z.vars;
            Ok((vars[at], vars[at + 1], vars[at + 2]))
        });
        methods.add_method_mut("player_at_w", |_, e, ()| {
            let world = &*e.em().world;
            Ok(world
                .bodies
                .iter()
                .find(|body| body.slot == PLAYER_BODY)
                .map_or(0, |body| body.atd.at_w))
        });
        methods.add_method_mut("draw_buffer", |_, e, ()| Ok(e.em().world.draw_buffer));
        methods.add_method_mut("room_code", |_, e, ()| Ok(e.em().world.room_code));
        methods.add_method_mut("set_status", |_, e, bits: i64| {
            let world = &mut e.em().world;
            world.status_or |= bits as u32;
            world.status_and_not &= !(bits as u32);
            Ok(())
        });
        methods.add_method_mut("clear_status", |_, e, bits: i64| {
            let world = &mut e.em().world;
            world.status_and_not |= bits as u32;
            world.status_or &= !(bits as u32);
            Ok(())
        });
        // Whether any of `bits` is set in `fg_status`'s first word.
        methods.add_method_mut("status", |_, e, bits: i64| {
            let world = &e.em().world;
            let word = world
                .banks
                .get(usize::from(crate::scd2::host::STATUS_BANK))
                .map_or(0, |words| words[0]);
            // The aim's `0x80` (`pl_mv_05`) is kept on the player.
            let aim = if world.pl.holds_view() { 0x80 } else { 0 };
            Ok((word | world.status_or | aim) & !world.status_and_not & bits as u32 != 0)
        });
        // `Flg_ck` of a flag bank, with what the script changed this tick.
        methods.add_method_mut("flag", |_, e, (bank, index): (u8, u8)| {
            let world = &e.em().world;
            if let Some(&(_, _, op)) = world
                .flag_ops
                .iter()
                .rev()
                .find(|&&(b, i, _)| b == bank && i == index)
            {
                return Ok(op == 1);
            }
            Ok(world.banks.get(usize::from(bank)).is_some_and(|words| {
                words[usize::from(index >> 5) & 7] & (0x8000_0000 >> (index & 31)) != 0
            }))
        });
        methods.add_method_mut("set_flag", |_, e, (bank, index, on): (u8, u8, bool)| {
            e.em().world.flag_ops.push((bank, index, u8::from(on)));
            Ok(())
        });
        // `Cut_change`.
        methods.add_method_mut("cut_change", |_, e, cut: u8| {
            e.em().world.cut = Some(cut);
            Ok(())
        });
        // Where object `index` of the room (`obj_model_set`) stands.
        methods.add_method_mut("object_pos", |_, e, index: usize| {
            let [x, y, z] = e.em().world.objects.get(index).copied().unwrap_or_default();
            Ok((x, y, z))
        });
        methods.add_method_mut("cut", |_, e, ()| Ok(e.em().world.camera_cut));
        // A room object's `Be_flg` cleared: it's gone (`Om_explosion_set`'s end).
        methods.add_method_mut("hide_object", |_, e, index: usize| {
            e.em().world.hidden_objects.push(index);
            Ok(())
        });
        methods.add_method_mut(
            "set_object_pos",
            |_, e, (index, x, y, z): (usize, i32, i32, i32)| {
                if let Some(pos) = e.em().world.objects.get_mut(index) {
                    *pos = [x, y, z];
                }
                Ok(())
            },
        );
        // A point given on `part`'s matrix as the last draw left it (`Workm`),
        // in the world (`Rot_add_matrix`'s translation), or nils before the
        // first draw; `part_point` poses the part on this tick's frame instead.
        // And back (`MulLMatrix`'s).
        methods.add_method_mut(
            "drawn_part_point",
            |_, e, (part, x, y, z): (usize, i32, i32, i32)| {
                let matrix = e.em().z.parts.get(part).copied().flatten();
                let Some(matrix) = matrix else {
                    return Ok((None, None, None));
                };
                let at = [x, y, z];
                let [wx, wy, wz] = std::array::from_fn(|row| {
                    let turned: i64 = (0..3)
                        .map(|k| i64::from(matrix.m[row][k]) * i64::from(at[k]))
                        .sum();
                    matrix.t[row].wrapping_add((turned >> 12) as i32)
                });
                Ok((Some(wx), Some(wy), Some(wz)))
            },
        );
        methods.add_method_mut(
            "part_local",
            |_, e, (part, x, y, z): (usize, i32, i32, i32)| {
                let matrix = e.em().z.parts.get(part).copied().flatten();
                let Some(matrix) = matrix else {
                    return Ok((x, y, z));
                };
                let point = player2::Matrix {
                    m: [[0x1000, 0, 0], [0, 0x1000, 0], [0, 0, 0x1000]],
                    t: [x, y, z],
                };
                let [lx, ly, lz] = point.relative_to(&matrix).t;
                Ok((lx, ly, lz))
            },
        );
        // `Rot_neck` with `Neck_flg`'s 4: `Neck_no` `joint` turns toward
        // `Neck_point` (`+0x170`) rather than the player's head. Only the
        // yaw is turned; the pitch (`+0x9A`) isn't ported.
        methods.add_method_mut(
            "rot_neck_point",
            |_, e, (joint, rate, limit, x, y, z): (usize, i32, i32, i32, i32, i32)| {
                let em = e.em();
                let from = em.z.parts.get(joint).copied().flatten().map(|m| m.t);
                let angle = em.mover.player.angle as i16;
                em.work().neck.joint = Some(joint as u8);
                em.mover
                    .rot_neck_joint(joint, from, Some([x, y, z]), angle, rate, limit);
                Ok(())
            },
        );
        // `Rot_neck_em` on `Neck_no` `joint`, with the joint's rate and limit
        // (its `+0x9C` and `+0xA0`), from where the last draw left it toward
        // the player's head.
        methods.add_method_mut(
            "rot_neck_part",
            |_, e, (joint, rate, limit): (usize, i32, i32)| {
                let em = e.em();
                let from = em.z.parts.get(joint).copied().flatten().map(|m| m.t);
                let target = em.world.player_neck;
                let angle = em.mover.player.angle as i16;
                em.work().neck.joint = Some(joint as u8);
                em.mover
                    .rot_neck_joint(joint, from, target, angle, rate, limit);
                Ok(())
            },
        );
        // The alligator's jaws: the roll of parts 6 and 7 on top of their pose.
        methods.add_method_mut("set_jaw", |_, e, (six, seven): (i16, i16)| {
            e.em().work().neck.jaw = [six, seven];
            Ok(())
        });
        // The player's `At_em_flg`: a body touched them this tick.
        methods.add_method_mut("player_touched", |_, e, ()| {
            Ok(e.em().world.pl.at_em_flg != 0)
        });
        methods.add_method_mut("set_player_angle", |_, e, angle: i64| {
            e.em().world.turn_player = Some(angle as u16 & 0xFFF);
            Ok(())
        });
        methods.add_method_mut("room_enemy", |_, e, ()| Ok(*e.em().world.room_enemy));
        methods.add_method_mut("set_room_enemy", |_, e, value: i64| {
            *e.em().world.room_enemy = value as u8;
            Ok(())
        });
        // `Ck_em_id`, of this one's id.
        methods.add_method_mut("same_id", |_, e, ()| Ok(e.em().world.same_id));
        methods.add_method_mut("take_player_floor", |_, e, ()| {
            let em = e.em();
            em.z.floor = em.world.player_floor;
            Ok(())
        });
        methods.add_method_mut("ground", |_, e, ()| Ok(e.em().z.vars[super::DOG_GROUND]));
        methods.add_method_mut("dest", |_, e, ()| {
            let [x, z] = e.em().work().dest;
            Ok((i32::from(x), i32::from(z)))
        });
        // How far apart two points are, on the floor, against `within`.
        methods.add_method_mut(
            "point_within",
            |_, _, (x0, z0, x1, z1, within): (i32, i32, i32, i32, i32)| {
                let (dx, dz) = (i64::from(x1 - x0), i64::from(z1 - z0));
                let within = i64::from(within);
                Ok(dx * dx + dz * dz < within * within)
            },
        );
        methods.add_method_mut("set_body_ofs_x", |_, e, ofs: i16| {
            e.em().z.atd.ofs_x = ofs;
            Ok(())
        });
        methods.add_method_mut("body_ofs_x", |_, e, ()| Ok(e.em().z.atd.ofs_x));
        // `Sca_ck_info` at a point, on this body's floor: whether a record of
        // `mask`, grown by `radius`, holds it.
        methods.add_method_mut(
            "sca_touch",
            |_, e, (x, y, z, radius, mask): (i32, i32, i32, i32, u16)| {
                let em = e.em();
                let floor = em.z.floor;
                Ok(em.mover.tick.room.sca.as_ref().is_some_and(|sca| {
                    crate::sca::first_hit(sca, mask, [x, y, z], radius, floor).is_some()
                }))
            },
        );
        // `Ck_em_id`: the slot of another work in play with `id`, or nil.
        methods.add_method_mut("other_of", |_, e, id: u8| {
            let world = &*e.em().world;
            Ok(world
                .works
                .iter()
                .find(|&&(other, other_id, in_play)| {
                    other != world.slot && other_id == Some(id) && in_play
                })
                .map(|&(other, ..)| other))
        });
        // Its own entity id, `Id`.
        methods.add_method_mut("id", |_, e, ()| {
            let world = &*e.em().world;
            Ok(world
                .works
                .iter()
                .find(|&&(other, ..)| other == world.slot)
                .and_then(|&(_, id, _)| id))
        });
        // Where the jaw was when this tick began.
        methods.add_method_mut("jaw", |_, e, ()| {
            let em = e.em();
            Ok((em.z.vars[super::DOG_JAW], em.z.vars[super::DOG_JAW + 1]))
        });
        // `Rot_neck_em`.
        methods.add_method_mut("rot_neck", |_, e, ()| {
            let em = e.em();
            let target = em.world.player_neck;
            em.mover.rot_neck_dog(target);
            Ok(())
        });
        // `Em_seq_sound`.
        methods.add_method_mut("seq_sound", |_, e, ()| {
            let em = e.em();
            let seq = em.mover.now_seq();
            if seq & super::SEQ_SOUND != 0 {
                em.z.sounds.push(((seq >> 28) as u8, em.mover.player.pos));
            }
            Ok(())
        });
        methods.add_method_mut("set_pos", |_, e, (x, y, z): (i32, i32, i32)| {
            e.em().mover.player.pos = [x, y, z];
            Ok(())
        });
        // `Old_x` to `Old_z`: where the body was when the tick began.
        methods.add_method_mut("old_pos", |_, e, ()| {
            let [x, y, z] = e.em().old_pos;
            Ok((x, y, z))
        });
        // `Water_ck`.
        methods.add_method_mut("water_ck", |_, e, (x, z): (i32, i32)| {
            Ok(super::water_ck(&e.em().world.water, x, z))
        });
        // A part's `Workm.t` as this work's last `FUN_004886f0` left it.
        methods.add_method_mut("own_part", |_, e, part: usize| {
            let t = e
                .em()
                .z
                .parts
                .get(part)
                .copied()
                .flatten()
                .map_or([0; 3], |matrix| matrix.t);
            Ok((t[0], t[1], t[2]))
        });
        // `Neck_no`'s turn (`+0x98`).
        methods.add_method_mut("neck_turn", |_, e, ()| Ok(e.em().work().neck.turn[0]));
        methods.add_method_mut("set_neck_turn", |_, e, turn: i16| {
            e.em().work().neck.turn[0] = turn;
            Ok(())
        });
        // `Goto00_ck` from a point, facing `angle`.
        methods.add_method_mut(
            "goto_ck_at",
            |_, _, (x0, z0, angle, x, z, rate): (i32, i32, i64, i32, i32, i32)| {
                let target = direction_ck(x0, z0, x, z);
                Ok(turn_within(angle as u16 & 0xFFF, target, rate))
            },
        );
        // The ivy a vine grew from (`pEnemy_ptr`), as it stood after its run.
        methods.add_method_mut("has_parent", |_, e, ()| Ok(e.em().world.parent.is_some()));
        // Keep another work as this one's `parent_*`, from the next tick: the
        // giant spider's partner (`+0x240`).
        methods.add_method_mut("set_parent", |_, e, slot: Option<u8>| {
            e.em().z.parent = slot;
            Ok(())
        });
        methods.add_method_mut("parent_var", |_, e, index: usize| {
            Ok(parent(e)?.vars.get(index).copied().unwrap_or(0))
        });
        methods.add_method_mut("parent_pos", |_, e, ()| {
            let [x, y, z] = parent(e)?.pos;
            Ok((x, y, z))
        });
        methods.add_method_mut("parent_angle", |_, e, ()| Ok(parent(e)?.angle));
        // The ivy's `Ground`, which `Game_loop` finds under its body.
        methods.add_method_mut("parent_ground", |_, e, ()| {
            let at = parent(e)?.pos;
            Ok(e.em().mover.tick.room.sca.as_ref().map_or(0, |sca| {
                crate::sca::ground_at(sca, at, 0, super::MASK).height
            }))
        });
        methods.add_method_mut("parent_life", |_, e, ()| Ok(parent(e)?.life));
        methods.add_method_mut("parent_kind", |_, e, ()| Ok(parent(e)?.kind));
        methods.add_method_mut("parent_routine", |_, e, index: i64| {
            Ok(parent(e)?.routine[routine_index(index)?])
        });
        // A part's `Workm.t` (`+0x5c`), as the shorts a vine reads of it.
        methods.add_method_mut("parent_part", |_, e, part: usize| {
            let t = parent(e)?
                .parts
                .get(part)
                .copied()
                .flatten()
                .map_or([0; 3], |matrix| matrix.t);
            Ok((t[0] as i16, t[1] as i16, t[2] as i16))
        });
        // A part's own rotation this tick (`+0x68`).
        methods.add_method_mut("parent_part_rot", |_, e, part: usize| {
            let r = parent(e)?.rotations.get(part).copied().unwrap_or([0; 3]);
            Ok((r[0], r[1], r[2]))
        });
        // `Set_enemy_work(id)`: the slot it gave, with `kind` and the fields in
        // `vars` set before the work's first run, or nil.
        methods.add_method_mut("spawn", |_, e, (id, kind, vars): (u8, u16, mlua::Table)| {
            let world = &mut *e.em().world;
            let Some((slot, reused)) = world.set_enemy_work(id) else {
                return Ok(None);
            };
            let mut fields = Vec::new();
            for pair in vars.pairs::<usize, i32>() {
                let (index, value) = pair?;
                if index >= super::VARS {
                    return Err(mlua::Error::RuntimeError(format!("no var {index}")));
                }
                fields.push((index, value));
            }
            fields.sort_unstable();
            world.spawns.push(super::Spawn {
                slot,
                id,
                kind,
                vars: fields,
                reused,
                place: None,
                run_now: false,
            });
            Ok(Some(slot))
        });
        // The last `spawn`'s place and facing, and fields set as well: its
        // routine then runs on it at once, as `FUN_004732a0` runs `Em_26`.
        methods.add_method_mut(
            "place_spawn",
            |_, e, (x, y, z, angle, vars): (i32, i32, i32, i64, mlua::Table)| {
                let world = &mut *e.em().world;
                let Some(spawn) = world.spawns.last_mut() else {
                    return Err(mlua::Error::RuntimeError("no spawn to place".into()));
                };
                for pair in vars.pairs::<usize, i32>() {
                    let (index, value) = pair?;
                    if index >= super::VARS {
                        return Err(mlua::Error::RuntimeError(format!("no var {index}")));
                    }
                    spawn.vars.retain(|&(other, _)| other != index);
                    spawn.vars.push((index, value));
                }
                spawn.vars.sort_unstable();
                spawn.place = Some(([x, y, z], angle as u16));
                spawn.run_now = true;
                Ok(())
            },
        );
        // `Ck_em_id(work, 0, id, 0)`: how many others in play have `id`.
        methods.add_method_mut("count_of", |_, e, id: u8| {
            let world = &*e.em().world;
            Ok(world
                .works
                .iter()
                .filter(|&&(other, other_id, in_play)| {
                    other != world.slot && other_id == Some(id) && in_play
                })
                .count())
        });
        // `Kill_enemy_work`: `Be_flg` and the routines go to 0, and the room
        // counts one enemy fewer.
        methods.add_method_mut("kill", |_, e, ()| {
            let em = e.em();
            *em.be_flg = 0;
            em.work().routine = [0; 4];
            em.world.killed = true;
            Ok(())
        });
        // `Rot_neck_em` on `Neck_no` `joint`, facing `angle`, from where the
        // joint was last tick toward the player's head.
        methods.add_method_mut(
            "rot_neck_joint",
            |_, e, (joint, angle, rate, limit): (usize, i64, i32, i32)| {
                let em = e.em();
                let from = em.work().neck.position;
                // Only the yaw is used, which the player's position gives
                // before their neck has one.
                let target = em.world.player_neck.or(Some(em.world.player));
                em.mover
                    .rot_neck_joint(joint, from, target, angle as i16, rate, limit);
                Ok(())
            },
        );
        // A part's own rotation this tick (`+0x68`).
        methods.add_method_mut("part_rot", |_, e, part: usize| {
            let em = e.em();
            let (clips, keyframes) = em.mover.tick.banks.get(em.mover.work.bank);
            let r = em
                .mover
                .player
                .anim
                .pose_keyframe(clips, keyframes)
                .and_then(|pose| pose.rotations.get(part).copied())
                .unwrap_or([0; 3]);
            Ok((r[0], r[1], r[2]))
        });
        methods.add_method_mut("partner", |_, e, ()| Ok(e.em().world.partner));
        methods.add_method_mut("enemies", |_, e, ()| Ok(e.em().world.enemies));
        methods.add_method_mut("has_guides", |_, e, ()| Ok(!e.em().world.guides.is_empty()));
        methods.add_method_mut("scene_target", |_, e, ()| {
            let [x, y, z] = e.em().world.scene_target;
            Ok((x, y, z))
        });

        // `Em_21`'s end of the work.
        methods.add_method_mut("line_to_player", |_, e, mask: i64| {
            let em = e.em();
            let from = em.mover.player.pos;
            Ok(!super::sca_ck_line_plan(
                em.world.sca,
                from,
                em.world.player,
                mask as u16,
            ))
        });
        // `FUN_004e2ec0` on `pSca_hit_data`, from where the body stands, and
        // the middle of that record: nothing if `Sca_ck_em` hit none.
        methods.add_method_mut("hit_side", |_, e, ()| {
            let em = e.em();
            let [x, _, z] = em.mover.player.pos;
            Ok(em.z.sca_hit.map(|[left, near, width, depth]| {
                crate::sca::side_across(x, z, [left, near], [width, depth])
            }))
        });
        methods.add_method_mut("hit_middle", |_, e, ()| {
            Ok(match e.em().z.sca_hit {
                Some([left, near, width, depth]) => {
                    (Some(left + (width >> 1)), Some(near + (depth >> 1)))
                }
                None => (None, None),
            })
        });
        methods.add_method_mut("wall_side", |_, e, return_to: i32| {
            Ok(e.em().wall_side(return_to))
        });
        methods.add_method_mut("tilt", |_, e, ()| {
            let [pitch, roll] = e.em().z.tilt;
            Ok((pitch, roll))
        });
        methods.add_method_mut("set_tilt", |_, e, (pitch, roll): (i64, i64)| {
            e.em().z.tilt = [pitch as u16, roll as u16];
            Ok(())
        });
        // `Cdir_x` and `Cdir_z`, which turn the body as it's posed as well as
        // drawn.
        methods.add_method_mut("cdir", |_, e, ()| {
            let [x, z] = e.em().work().tilt;
            Ok((i64::from(x), i64::from(z)))
        });
        methods.add_method_mut("set_cdir", |_, e, (x, z): (i64, i64)| {
            let em = e.em();
            em.work().tilt = [x as i16, z as i16];
            em.z.tilt = [x as u16, z as u16];
            Ok(())
        });
        // `RotMatrix(angle)` then `ApplyMatrixSV` on a point.
        methods.add_method_mut(
            "rot_apply",
            |_, _, (ax, ay, az, x, y, z): (i64, i64, i64, i32, i32, i32)| {
                let m = player2::rot_matrix([ax as i16, ay as i16, az as i16]);
                let [x, y, z] = std::array::from_fn(|row| {
                    let sum = i32::from(m[row][0]) * x
                        + i32::from(m[row][1]) * y
                        + i32::from(m[row][2]) * z;
                    i32::from((sum >> 12) as i16)
                });
                Ok((x, y, z))
            },
        );
        // `RotMatrix(&Super_vector, pSuper)` while `pSuper` is `&Super_matrix`:
        // the surface the body is drawn on.
        methods.add_method_mut("set_super", |_, e, (x, y, z): (i64, i64, i64)| {
            e.em().work().super_matrix = Some(player2::rot_matrix([x as i16, y as i16, z as i16]));
            Ok(())
        });
        methods.add_method_mut("rsin", |_, _, angle: i64| Ok(player2::rsin(angle as i32)));
        methods.add_method_mut("rcos", |_, _, angle: i64| Ok(player2::rcos(angle as i32)));
        methods.add_method_mut("set_shadow", |_, e, half: i32| {
            e.em().z.shadow_half = Some(half);
            Ok(())
        });
        // The player's `Ground`.
        methods.add_method_mut("player_ground", |_, e, ()| {
            let em = e.em();
            let at = em.world.player;
            Ok(em.mover.tick.room.sca.as_ref().map_or(0, |sca| {
                crate::sca::ground_at(sca, at, 0, crate::sca::PLAYER_MASK).height
            }))
        });
        methods.add_method_mut("direction_from_player", |_, e, ()| {
            let em = e.em();
            let [x, _, z] = em.mover.player.pos;
            let [from_x, _, from_z] = em.world.player;
            Ok(direction_ck(from_x, from_z, x, z))
        });
        methods.add_method_mut("scenario_claire", |_, e, ()| Ok(e.em().world.claire));
        // `FUN_00463070`: every body of this id, but those with `skip` in
        // `flags`, gets `bits` there and two fields cleared.
        methods.add_method_mut(
            "call_kin",
            |_, e, (flags, skip, bits, one, two): (usize, i32, i32, usize, usize)| {
                if flags.max(one).max(two) >= super::VARS {
                    return Err(mlua::Error::RuntimeError(format!("no var {flags}")));
                }
                let em = e.em();
                let call = super::KinCall {
                    flags,
                    skip,
                    bits,
                    clear: [one, two],
                };
                // It hears itself as it calls, before it can change what keeps it out.
                em.z.hear(&call);
                em.world.kin.push(call);
                Ok(())
            },
        );

        // `Esp_call`.
        methods.add_method_mut(
            "effect",
            |_, e, (kind, row, size, dir, part, x, y, z): (u8, u8, u16, i16, usize, i32, i32, i32)| {
                e.em().effect(kind, row, size, dir, part, [x, y, z]);
                Ok(())
            },
        );
        methods.add_method_mut(
            "call_effect",
            |_, e, (kind, row, size, dir, x, y, z): (u8, u8, u16, i16, i64, i64, i64)| {
                let at = [x as i16, y as i16, z as i16];
                e.em().world.effects.call(kind, row, size, dir, at, false);
                Ok(())
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The names a source registers through `add_method`, `add_function` or
    /// their `_mut` forms, in order.
    fn registered_names(source: &str) -> Vec<&str> {
        let mut names = Vec::new();
        for (at, _) in source.match_indices(".add_") {
            let rest = &source[at + ".add_".len()..];
            let kind_len = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            if !matches!(
                &rest[..kind_len],
                "method" | "method_mut" | "function" | "function_mut"
            ) {
                continue;
            }
            let Some(args) = rest[kind_len..].strip_prefix('(') else {
                continue;
            };
            let Some(quoted) = args.trim_start().strip_prefix('"') else {
                continue;
            };
            if let Some(end) = quoted.find('"') {
                names.push(&quoted[..end]);
            }
        }
        names
    }

    fn repeated_names<'a>(names: &[&'a str]) -> Vec<&'a str> {
        let mut seen = std::collections::HashSet::new();
        names
            .iter()
            .copied()
            .filter(|name| !seen.insert(*name))
            .collect()
    }

    /// mlua keeps the last of two methods registered under one name, so a
    /// second registration silently replaces the first for every script.
    #[test]
    fn no_enemy_script_method_is_registered_twice() {
        for (file, source) in [
            ("src/zombie2/script.rs", include_str!("script.rs")),
            ("src/enemy/lua.rs", include_str!("../enemy/lua.rs")),
        ] {
            // The tests' own samples below aren't registrations.
            let code = source.split("#[cfg(test)]").next().unwrap_or(source);
            let names = registered_names(code);
            assert!(
                names.len() > 100,
                "{file}: only {} names found",
                names.len()
            );
            assert_eq!(repeated_names(&names), Vec::<&str>::new(), "{file}");
        }
    }

    #[test]
    fn the_registration_scan_finds_a_repeated_name() {
        let source = r#"
            methods.add_method_mut("pos", |_, e, ()| Ok(()));
            methods.add_method(
                "part_point",
                |_, e, ()| Ok(()),
            );
            methods.add_function("pos", |_, ()| Ok(()));
            methods.add_meta_method(MetaMethod::ToString, |_, e, ()| Ok(()));
        "#;
        let names = registered_names(source);
        assert_eq!(names, ["pos", "part_point", "pos"]);
        assert_eq!(repeated_names(&names), ["pos"]);
    }

    const SCRIPTS: [&str; 11] = [
        include_str!("../../games/re2/enemy/20/enemy.lua"),
        include_str!("../../games/re2/enemy/21/enemy.lua"),
        include_str!("../../games/re2/enemy/23/enemy.lua"),
        include_str!("../../games/re2/enemy/25/enemy.lua"),
        include_str!("../../games/re2/enemy/26/enemy.lua"),
        include_str!("../../games/re2/enemy/29/enemy.lua"),
        include_str!("../../games/re2/enemy/2a/enemy.lua"),
        include_str!("../../games/re2/enemy/2f/enemy.lua"),
        include_str!("../../games/re2/lib/ivy.lua"),
        include_str!("../../games/re2/lib/licker.lua"),
        include_str!("../../games/re2/lib/zombie.lua"),
    ];

    /// Each licker's variant names every clip `lib/licker.lua` asks it for,
    /// and a life for each of the sixteen draws.
    #[test]
    fn each_licker_id_runs_the_shared_driver() {
        let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
        let library = include_str!("../../games/re2/lib/licker.lua");
        let mut names: Vec<&str> = library
            .split("V.clips.")
            .skip(1)
            .map(|rest| {
                rest.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .next()
                    .unwrap()
            })
            .collect();
        names.sort_unstable();
        names.dedup();
        for script in [
            include_str!("../../games/re2/enemy/22/enemy.lua"),
            include_str!("../../games/re2/enemy/24/enemy.lua"),
        ] {
            lua.globals().set("NAMES", names.clone()).unwrap();
            let ok: bool = lua
                .load(format!(
                    "local variant
                     local function require(name)
                         assert(name == '/lib/licker')
                         return {{ run = function(e, v) variant = v end }}
                     end
                     {script}
                     update(nil)
                     for _, name in ipairs(NAMES) do
                         assert(variant.clips[name], name)
                     end
                     for i = 0, 15 do
                         assert(variant.life[i] and variant.life_easy[i])
                     end
                     return variant.starts[0] ~= nil and variant.blood ~= nil
                         and variant.init_late ~= nil"
                ))
                .eval()
                .unwrap();
            assert!(ok);
        }
    }

    /// A stand-in for the work, enough for `enemy/25/enemy.lua`'s init and
    /// floor states: `Rnd` gives `DRAWS` in turn. By default the player is
    /// far away, half a turn round, and no clip ends; `DIST`, `TARGET`,
    /// `CLIP_LEN` and `NOISE` change that. `Goto00_ck` gives `rate` the way to
    /// the player's angle, `TARGET`, or 0 within `rate` of it. `ROUTINES` and
    /// `VARS` start it past its init, `DAMAGE_NO` and `D_LIFE` are the last
    /// hit's, `WATER_LEVEL` is `Water_ck`'s, and a clip's end shows `SEQ_END`.
    /// For the ceiling and the walls, `Sca_ck_info` with mask 0x400 gives
    /// `SCA_FLAGS` and `SCA_LIST`, every record faces `SCA_SIDE` with its
    /// sides' edges at `EDGES`, and nothing else is a spider. `POS` places it.
    const SPIDER_STUB: &str = "
        local draws, drawn = DRAWS, 0
        local dist, target, clip_len = DIST or 20000, TARGET or 0x800, CLIP_LEN or 1000
        local w = { r = ROUTINES or { [0] = 0, 0, 0, 0 }, vars = VARS or {},
            timers = { [0] = 0, 0, 0 }, bloods = 0, blown = 0, died = 0, y = 0, life = 0,
            be = 1, kind = KIND, angle = 0, spd = { 0, 0, 0 }, moved = 0,
            clips = {}, frame = 0, add_dir = -1, pl = { [0] = 1, 0, 0, 0 },
            pl_life = PL_LIFE or 200, pl_dc = 0, sounds = {}, effect = { -1, -1 },
            rotated = { -1, -1 }, cdir = { 0, 0 }, pos = POS or { 0, 0, 0 }, room_enemy = 0 }
        local edges = EDGES or { [0] = 0, 0, 0, 0 }
        local function way(rate)
            local d = (target - w.angle) & 0xFFF
            if d > 0x800 then d = d - 0x1000 end
            if d >= -rate and d <= rate then return 0, d end
            return d < 0 and -rate or rate, d
        end
        local e = {}
        function e:routine(i) return w.r[i] end
        function e:set_routine(i, v) w.r[i] = v end
        function e:routines(a, b, c, d) w.r = { [0] = a, b, c, d } end
        function e:var(i) return w.vars[i] or 0 end
        function e:set_var(i, v) w.vars[i] = v end
        function e:timer(i) return w.timers[i] end
        function e:set_timer(i, v) w.timers[i] = v & 0xFFFF end
        function e:rnd() drawn = drawn + 1; return draws[drawn] end
        function e:be_flg() return w.be end
        function e:set_be_flg(v) w.be = v end
        function e:kind() return w.kind end
        function e:easy() return false end
        function e:set_life(v) w.life = v end
        function e:clip(n) w.clips[#w.clips + 1] = n; w.frame = 0 end
        function e:joint() w.frame = w.frame + 1; return w.frame >= clip_len end
        function e:angle() return w.angle end
        function e:set_angle(v) w.angle = v end
        function e:speed() return w.spd[1], w.spd[2], w.spd[3] end
        function e:set_speed(x, y, z) w.spd = { x, y, z } end
        function e:set_speed_x(x) w.spd[1] = x end
        function e:add_speed_xz_at(dir) w.moved = w.moved + 1; w.add_dir = dir end
        function e:distance() return dist end
        function e:pos() return w.pos[1], w.pos[2], w.pos[3] end
        function e:add_pos(x, y, z)
            w.pos = { w.pos[1] + x, w.pos[2] + y, w.pos[3] + z }
        end
        function e:rot_apply(ax, ay, az, x, y, z) return x, y, z end
        function e:cdir() return w.cdir[1], w.cdir[2] end
        function e:set_cdir(x, z) w.cdir = { x & 0xFFFF, z & 0xFFFF } end
        function e:floor() return 0 end
        function e:sca_ck_info(x, z, floor, mask)
            if mask == 0x400 then return SCA_FLAGS or 0, SCA_LIST or {} end
            return 0, {}
        end
        function e:sca_side() return SCA_SIDE or 0 end
        function e:sca_edge(record, side) return edges[side] end
        function e:sca_rect() return 0, 0, 0, 0, 0, 0 end
        function e:other_of() return nil end
        function e:has_parent() return false end
        function e:room_enemy() return w.room_enemy end
        function e:set_room_enemy(v) w.room_enemy = v end
        function e:goto_ck_at(x, z, angle, px, pz, rate) return (way(rate)) end
        function e:player_pos() return 0, 0, 20000 end
        function e:goto_ck(x, z, rate) return (way(rate)) end
        function e:goto_point(x, z, rate)
            local turn, d = way(rate)
            w.angle = (w.angle + (turn == 0 and d or turn)) & 0xFFFF
        end
        function e:direction() return 0x400 end
        function e:rotate(dir, x, z) w.rotated = { dir, x }; return 11, 22 end
        function e:part_point() return 30, 0, 40 end
        function e:hypot(x, z) return math_hypot(x, z) end
        function e:now_seq() return w.frame >= clip_len and (SEQ_END or 0) or 0 end
        function e:damage_no() return DAMAGE_NO or 0 end
        function e:d_life() return D_LIFE or 6 end
        function e:water_ck() return WATER_LEVEL or 0 end
        function e:own_part(part) return part, 0, 0 end
        function e:call_effect() w.bloods = w.bloods + 1 end
        function e:blow_part() w.blown = w.blown + 1 end
        function e:set_died() w.died = 1 end
        function e:set_pos_y(y) w.y = y; w.pos[2] = y end
        function e:neck_flags() return 0 end
        function e:noise() return NOISE or 0 end
        function e:at_em_no() return 0xFF end
        function e:sca_info() return 0 end
        function e:cdir_player() return false end
        function e:pl_damage_cnt() return w.pl_dc end
        function e:pl_set_damage_cnt(v) w.pl_dc = v end
        function e:pl_set_routine(a, b, c, d) w.pl = { [0] = a, b, c, d } end
        function e:pl_life_down(down)
            w.pl_life = w.pl_life - down
            return w.pl_life < 0 and 2 or 0
        end
        function e:sound(n) w.sounds[#w.sounds + 1] = n end
        function e:effect(kind, row, size, dir) w.effect = { row, dir } end
        w.spawns, w.free = {}, FREE or 0
        function e:count_of() return IN_PLAY or 0 end
        function e:spawn(id, kind)
            if w.free == 0 then return nil end
            w.free = w.free - 1
            w.spawns[#w.spawns + 1] = { kind = kind }
            return #w.spawns
        end
        function e:place_spawn(x, y, z, angle, vars)
            local spawn = w.spawns[#w.spawns]
            spawn.angle, spawn.drawn = angle, vars[6]
        end
        for _, name in ipairs({ 'set_d_life', 'set_damage_flg', 'set_sce_flg',
            'set_neck_flags', 'set_shadow', 'set_body_ofs_x', 'set_body_box', 'seq_sound',
            'set_parent' }) do
            e[name] = function() end
        end
    ";

    /// What `run_spider` leaves: its fields by name, and its clips.
    struct Spider(std::collections::HashMap<String, i64>, Vec<u8>);

    impl Spider {
        fn get(&self, name: &str) -> i64 {
            self.0[name]
        }
    }

    fn run_spider(kind: u16, draws: &[u8], ticks: usize) -> Spider {
        run_spider_with(kind, draws, ticks, "")
    }

    /// `run_spider` with `setup`'s globals set before the stub.
    fn run_spider_with(kind: u16, draws: &[u8], ticks: usize, setup: &str) -> Spider {
        let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
        lua.load(setup).exec().unwrap();
        let script = include_str!("../../games/re2/enemy/25/enemy.lua");
        lua.globals().set("DRAWS", draws.to_vec()).unwrap();
        lua.globals().set("KIND", kind).unwrap();
        lua.globals()
            .set(
                "math_hypot",
                lua.create_function(|_, (x, z): (i32, i32)| Ok(square_root0(x * x + z * z)))
                    .unwrap(),
            )
            .unwrap();
        lua.load(format!(
            "{SPIDER_STUB}
             {script}
             for _ = 1, {ticks} do update(e) end
             return {{ r0 = w.r[0], r1 = w.r[1], r2 = w.r[2], life = w.life, be = w.be,
                 clips = w.clips, angle = w.angle, spd = w.spd[1], moved = w.moved,
                 timer = w.timers[0], timer1 = w.timers[1], drawn = drawn,
                 spd_y = w.spd[2], spd_z = w.spd[3], add_dir = w.add_dir,
                 cooldown = w.vars[0] or 0, pl0 = w.pl[0], pl1 = w.pl[1],
                 pl_life = w.pl_life, pl_dc = w.pl_dc, sounds = #w.sounds,
                 bite_sound = w.sounds[#w.sounds] or -1, effect_row = w.effect[1],
                 effect_dir = w.effect[2], rot_dir = w.rotated[1],
                 rot_by = w.rotated[2], bloods = w.bloods, blown = w.blown,
                 died = w.died, y = w.y, lost = w.vars[12] or 0,
                 revived = w.vars[16] or 0, timer2 = w.timers[2],
                 kind_var = w.vars[2] or 0, add_dir_var = w.vars[1] or 0,
                 facing = w.vars[26] or 0, record = w.vars[24] or 0,
                 between = w.vars[28] or 0, top = w.vars[25] or 0,
                 t225 = w.vars[20] or 0, cdir_x = w.cdir[1], cdir_z = w.cdir[2],
                 room_enemy = w.room_enemy, pos_y = w.pos[2] }}"
        ))
        .eval::<mlua::Table>()
        .map(|table| {
            let clips = table
                .get::<mlua::Table>("clips")
                .unwrap()
                .sequence_values()
                .map(Result::unwrap)
                .collect();
            let fields = "r0 r1 r2 life be angle spd moved timer drawn timer1 spd_y spd_z \
                add_dir cooldown pl0 pl1 pl_life pl_dc sounds bite_sound effect_row \
                effect_dir rot_dir rot_by bloods blown died y lost revived timer2 kind_var add_dir_var facing record between top t225 \
                cdir_x cdir_z room_enemy pos_y"
                .split_whitespace()
                .map(|name| (name.to_string(), table.get::<i64>(name).unwrap()))
                .collect();
            Spider(fields, clips)
        })
        .unwrap()
    }

    /// `FUN_0046f0c0`: life from the table and `Rnd & 3`, the floor's
    /// `Be_flg`, then the rest until its count runs out.
    #[test]
    fn a_floor_spider_rests_then_walks() {
        // Life row 4 (0x77) plus 2; the rest counts (0 + 10) & 0x1f = 10 ticks;
        // then an even draw picks the walk, whose count is 0x2e & 0x2e.
        let mut draws = vec![4, 2, 0, 0, 0x2E];
        draws.resize(32, 0);
        let spider = run_spider(0, &draws, 1);
        assert_eq!(spider.get("life"), 0x79);
        assert_eq!(spider.get("be"), 0xCC0_0001);
        assert_eq!(spider.get("r1"), 0);
        let spider = run_spider(0, &draws, 11);
        assert_eq!(spider.get("r1"), 0);
        let spider = run_spider(0, &draws, 12);
        assert_eq!(spider.get("r1"), 1);
        let spider = run_spider(0, &draws, 13);
        assert_eq!(spider.get("r2"), 1);
        assert_eq!(spider.get("timer"), 0x2D);
        // Walk 1: clip 2, turned 0x30 toward the player, and stepped by
        // `SquareRoot0` of the foot's move, (30, 40), plus 100.
        assert_eq!(spider.1, [1, 0, 2]);
        assert_eq!(spider.get("angle"), 0x30);
        assert_eq!(spider.get("spd"), i64::from(square_root0(2500)) + 100);
        assert_eq!(spider.get("moved"), 1);
    }

    /// An odd draw as the rest ends turns it on the spot instead, for
    /// `(Rnd + 10) & 0x3f` ticks of 0x18, the other way with another odd draw.
    #[test]
    fn a_floor_spider_turns_on_the_spot() {
        let mut draws = vec![0, 0, 0, 1, 5, 1];
        draws.resize(32, 0);
        let spider = run_spider(0, &draws, 12);
        assert_eq!(spider.get("r1"), 4);
        let spider = run_spider(0, &draws, 13);
        assert_eq!(spider.get("timer"), 14);
        assert_eq!(spider.get("angle"), 0xFFE8);
        assert_eq!(spider.get("moved"), 0);
        let spider = run_spider(0, &draws, 27);
        assert_eq!(spider.get("r1"), 4);
        let spider = run_spider(0, &draws, 28);
        assert_eq!(spider.get("r1"), 0);
    }

    /// Within 8000 units and facing more than 0x400 away, the rest turns it
    /// toward the player (`Routine_1` 5): clip 2, 0x80 a tick until it's
    /// within 0x80, then back to the rest, which, facing the player 5000
    /// away with no cooldown, lunges (7).
    #[test]
    fn a_near_floor_spider_turns_to_face_the_player() {
        let near = "DIST = 5000";
        let draws = [0; 32];
        let spider = run_spider_with(0, &draws, 2, near);
        assert_eq!((spider.get("r1"), spider.get("r2")), (5, 0));
        let spider = run_spider_with(0, &draws, 3, near);
        assert_eq!(spider.1, [1, 0, 2]);
        assert_eq!(spider.get("timer1"), 0x3B);
        assert_eq!(spider.get("angle"), 0x80);
        // Fifteen turns of 0x80 leave it 0x80 short, which faces it.
        let spider = run_spider_with(0, &draws, 18, near);
        assert_eq!((spider.get("r1"), spider.get("r2")), (5, 2));
        assert_eq!(spider.get("angle"), 0x780);
        let spider = run_spider_with(0, &draws, 19, near);
        assert_eq!(spider.get("r1"), 0);
        let spider = run_spider_with(0, &draws, 20, near);
        assert_eq!(spider.get("r1"), 7);
    }

    /// `Routine_1` 7: a step (clip 4) at 100, a turn (clip 5) to within 0x40,
    /// the lunge at 300 that bites a player within 1500 for 20 and sets their
    /// hit reaction, the landing (clip 7), then the rest with `+0x218` at 15.
    #[test]
    fn a_floor_spider_lunges_and_bites() {
        let near = "DIST = 1200 TARGET = 0 CLIP_LEN = 4";
        let draws = [0; 32];
        let spider = run_spider_with(0, &draws, 2, near);
        assert_eq!(spider.get("r1"), 7);
        let spider = run_spider_with(0, &draws, 6, near);
        assert_eq!((spider.get("r2"), spider.get("spd")), (2, 100));
        assert_eq!(spider.get("add_dir"), 0);
        let spider = run_spider_with(0, &draws, 7, near);
        assert_eq!((spider.get("r2"), spider.get("timer")), (4, 0x13));
        let spider = run_spider_with(0, &draws, 8, near);
        assert_eq!(spider.1, [1, 0, 4, 5, 5]);
        assert_eq!((spider.get("r2"), spider.get("spd")), (6, 300));
        assert_eq!((spider.get("pl0"), spider.get("pl1")), (2, 2));
        assert_eq!((spider.get("pl_life"), spider.get("pl_dc")), (180, 0x80));
        assert_eq!((spider.get("sounds"), spider.get("bite_sound")), (1, 1));
        let spider = run_spider_with(0, &draws, 12, near);
        assert_eq!(spider.1, [1, 0, 4, 5, 5, 7]);
        assert_eq!(spider.get("r2"), 8);
        let spider = run_spider_with(0, &draws, 13, near);
        assert_eq!((spider.get("r1"), spider.get("cooldown")), (0, 14));
        // The bite waits for the hit reaction to clear: one lunge, one bite.
        assert_eq!(spider.get("pl_life"), 180);
        // A bite that kills sets the player's `Routine` 3.
        let spider = run_spider_with(0, &draws, 8, &format!("{near} PL_LIFE = 10"));
        assert_eq!((spider.get("pl0"), spider.get("pl1")), (3, 0));
    }

    /// A draw of 6, 8 or 9 at the rest picks the spit (`Routine_1` 8):
    /// `Esp_call` 0x1d with the floor's row 8, along the way it faces, then
    /// the rest with `+0x218` at 15 once clip 0xb ends.
    #[test]
    fn a_floor_spider_spits() {
        let near = "DIST = 1200 TARGET = 0 CLIP_LEN = 4";
        let mut draws = vec![0, 0, 0, 6];
        draws.resize(32, 0);
        let spider = run_spider_with(0, &draws, 2, near);
        assert_eq!(spider.get("r1"), 8);
        let spider = run_spider_with(0, &draws, 3, near);
        assert_eq!(spider.1, [1, 0, 0xB]);
        assert_eq!((spider.get("effect_row"), spider.get("effect_dir")), (8, 0));
        let spider = run_spider_with(0, &draws, 6, near);
        assert_eq!(spider.get("r2"), 2);
        let spider = run_spider_with(0, &draws, 7, near);
        assert_eq!((spider.get("r1"), spider.get("cooldown")), (0, 14));
    }

    /// A noise sends it to the leap (`Routine_1` 9) on an even draw: 0x380
    /// plus a draw off the way to the player, with half a turn on an odd
    /// draw, at 400 plus a draw's low six bits, which `Timer0` keeps and
    /// `Add_speedXZ` adds to `Add_dir`.
    #[test]
    fn a_floor_spider_leaps_aside_from_a_noise() {
        let noise = "NOISE = 1 CLIP_LEN = 4";
        let mut draws = vec![0, 0, 0, 0, 1, 0x10, 5];
        draws.resize(32, 0);
        let spider = run_spider_with(0, &draws, 2, noise);
        assert_eq!(spider.get("r1"), 9);
        let spider = run_spider_with(0, &draws, 3, noise);
        assert_eq!(spider.1, [1, 0, 8]);
        let dir = 0x400 + 0x800 + 0x380 + 0x10;
        assert_eq!(spider.get("timer"), dir);
        assert_eq!((spider.get("rot_dir"), spider.get("rot_by")), (dir, 405));
        assert_eq!(
            (spider.get("spd"), spider.get("spd_y"), spider.get("spd_z")),
            (11, 0, 22)
        );
        assert_eq!(spider.get("add_dir"), dir);
        let spider = run_spider_with(0, &draws, 6, noise);
        assert_eq!(spider.get("r2"), 2);
        let spider = run_spider_with(0, &draws, 7, noise);
        assert_eq!(spider.get("r1"), 0);
    }

    /// `FUN_0046f0c0` on the ceiling (`Type` 2 or 3): kind 1, `Add_dir` and
    /// `Cdir_z` half a turn, `Be_flg`'s height bit 0x10000000 only, and the
    /// floor's rest and walk.
    #[test]
    fn a_ceiling_spider_rests_then_walks() {
        // The fifth draw is the walk's `Timer0`.
        let mut draws = [0; 32];
        draws[4] = 0x2E;
        let spider = run_spider(0x43, &draws, 1);
        assert_eq!(
            (spider.get("kind_var"), spider.get("add_dir_var")),
            (1, 0x800)
        );
        assert_eq!((spider.get("cdir_x"), spider.get("cdir_z")), (0, 0x800));
        assert_eq!(spider.get("be") & 0x1C00_0000, 0x1000_0000);
        assert_eq!(spider.get("top"), 0x4E2);
        let spider = run_spider(0x43, &draws, 12);
        assert_eq!(spider.get("r1"), 1);
        let spider = run_spider(0x43, &draws, 13);
        assert_eq!((spider.get("r1"), spider.get("r2")), (1, 1));
    }

    /// `Routine_1` 1 on the ceiling: over a wall's record that faces its way,
    /// with neither end of the wall within 2500 and the player within 3000 of
    /// the wall, it takes the way down (10) and marks the room's 0x20.
    #[test]
    fn a_ceiling_spider_takes_the_way_down_a_wall() {
        // The fifth draw is the walk's `Timer0`.
        let mut draws = [0; 32];
        draws[4] = 0x2E;
        let wall = "SCA_FLAGS = 1 SCA_LIST = { 4 } EDGES = { [0] = 1000, -10000, 0, 10000 }";
        let spider = run_spider_with(0x43, &draws, 13, wall);
        assert_eq!(spider.get("r1"), 10);
        assert_eq!((spider.get("record"), spider.get("facing")), (4, 0));
        assert_eq!((spider.get("between"), spider.get("room_enemy")), (1, 0x20));
        // With the player 3000 or more from the wall it walks on.
        let far = "SCA_FLAGS = 1 SCA_LIST = { 4 } EDGES = { [0] = 3000, -10000, 0, 10000 }";
        let spider = run_spider_with(0x43, &draws, 13, far);
        assert_eq!((spider.get("r1"), spider.get("room_enemy")), (1, 0));
        // So does one near an end of the wall.
        let end = "SCA_FLAGS = 1 SCA_LIST = { 4 } EDGES = { [0] = 1000, -10000, 0, 2000 }";
        let spider = run_spider_with(0x43, &draws, 13, end);
        assert_eq!(spider.get("r1"), 1);
    }

    /// `FUN_0046f0c0` on a wall (`Type` 4 to 11): kind 3, the way it faces
    /// from `Type`, the climb's top from its high nibble, then the watch (2)
    /// until the player comes within 7200: the crawl (3), or within 3000 the
    /// spit (7). Its height bits follow the player's height below it.
    #[test]
    fn a_wall_spider_watches_for_the_player() {
        let draws = [0; 32];
        let spider = run_spider(9, &draws, 1);
        assert_eq!((spider.get("kind_var"), spider.get("facing")), (3, 2));
        assert_eq!((spider.get("angle"), spider.get("t225")), (0x800, 1));
        assert_eq!(spider.get("top"), -0xF78);
        // `+0x230` keeps the whole byte; only its low two bits are a side.
        let spider = run_spider(0x2B, &draws, 1);
        assert_eq!((spider.get("facing"), spider.get("angle")), (0x13, 0x4C00));
        assert_eq!(spider.get("top"), 2 * -0x708 + 0x4E2);
        let spider = run_spider(9, &draws, 5);
        assert_eq!((spider.get("r0"), spider.get("r1")), (1, 2));
        assert_eq!((spider.get("cdir_x"), spider.get("cdir_z")), (0, 0x400));
        assert_eq!(spider.get("be") & 0x1C00_0000, 0x1C00_0000);
        assert_eq!(spider.get("drawn"), 2);
        let spider = run_spider_with(9, &draws, 3, "DIST = 5000");
        assert_eq!(spider.get("r1"), 3);
        let spider = run_spider_with(9, &draws, 3, "DIST = 1200");
        assert_eq!(spider.get("r1"), 7);
    }

    #[test]
    fn each_zombie_id_runs_the_shared_driver() {
        let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
        for script in [
            include_str!("../../games/re2/enemy/10/enemy.lua"),
            include_str!("../../games/re2/enemy/11/enemy.lua"),
            include_str!("../../games/re2/enemy/12/enemy.lua"),
            include_str!("../../games/re2/enemy/13/enemy.lua"),
            include_str!("../../games/re2/enemy/15/enemy.lua"),
            include_str!("../../games/re2/enemy/16/enemy.lua"),
            include_str!("../../games/re2/enemy/17/enemy.lua"),
            include_str!("../../games/re2/enemy/18/enemy.lua"),
            include_str!("../../games/re2/enemy/1e/enemy.lua"),
            include_str!("../../games/re2/enemy/1f/enemy.lua"),
        ] {
            let ran: bool = lua
                .load(format!(
                    "local ran = false
                     local function require(name)
                         assert(name == '/lib/zombie')
                         return {{ run = function(e, v) ran = type(v) == 'table' end }}
                     end
                     {script}
                     update(nil)
                     return ran"
                ))
                .eval()
                .unwrap();
            assert!(ran);
        }
    }

    /// `FUN_00454f30` reaches for the player in the lunge's last step,
    /// `Routine_2` of 3, whatever clip is playing: in room 100 the lunge's
    /// clip is 0x1B when the original's zombie takes hold.
    #[test]
    fn a_lunging_zombie_reaches_in_the_lunges_last_step() {
        let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
        let library = include_str!("../../games/re2/lib/zombie.lua");
        let reach = |step: u8| -> (Vec<u8>, i64) {
            lua.globals().set("STEP", step).unwrap();
            lua.load(format!(
                "local w = {{ r = {{ [0] = 1, 0xC, STEP, 0 }}, pl_dc = 0 }}
                 local e = setmetatable({{}}, {{ __index = function() return function() return 0 end end }})
                 function e:routine(i) return w.r[i] end
                 function e:set_routine(i, v) w.r[i] = v end
                 function e:routines(a, b, c, d) w.r = {{ [0] = a, b, c, d }} end
                 function e:move_no() return 0x1B end
                 function e:move_cnt() return 5 end
                 function e:distance() return 1100 end
                 function e:same_floor() return true end
                 function e:player_id() return 0 end
                 function e:player_within() return true end
                 function e:joint_bank() return false end
                 function e:pl_damage_cnt() return w.pl_dc end
                 function e:pl_set_damage_cnt(v) w.pl_dc = v end
                 function e:cdir_player() return false end
                 function e:voice_free() return false end
                 local zombie = (function() {library} end)()
                 zombie.run(e, {{}})
                 R = {{ w.r[0], w.r[1] }}
                 PL_DC = w.pl_dc"
            ))
            .exec()
            .unwrap();
            let routines: Vec<u8> = lua.globals().get("R").unwrap();
            (routines, lua.globals().get("PL_DC").unwrap())
        };
        let (routines, held) = reach(3);
        assert_eq!(routines, [1, 3]);
        assert_ne!(held & 0x80, 0);
        let (routines, held) = reach(1);
        assert_eq!(routines, [1, 0xC]);
        assert_eq!(held & 0x80, 0);
    }

    /// `FUN_00453010`'s wall: a chasing zombie against a wall turns to it
    /// (state 0xA) when the script has raised `Sce_free0`'s high bits
    /// (`+0x1D5`), as room 103's does with `0x8C00`; `Sce_free1` doesn't.
    #[test]
    fn a_chasing_zombie_takes_the_scripts_wall_from_sce_free0() {
        let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
        let library = include_str!("../../games/re2/lib/zombie.lua");
        let chase = |free: [i32; 2]| -> Vec<u8> {
            lua.globals().set("FREE", free.to_vec()).unwrap();
            lua.load(format!(
                "local w = {{ r = {{ [0] = 1, 2, 1, 0 }}, free = {{ [0] = FREE[1], FREE[2] }} }}
                 local e = setmetatable({{}}, {{ __index = function() return function() return 0 end end }})
                 function e:routine(i) return w.r[i] end
                 function e:set_routine(i, v) w.r[i] = v end
                 function e:routines(a, b, c, d) w.r = {{ [0] = a, b, c, d }} end
                 function e:sce_free(i) return w.free[i] end
                 function e:set_sce_free(i, v) w.free[i] = v end
                 function e:sca_info() return 1 end
                 function e:distance() return 20000 end
                 function e:turn_to_player() return 0x100 end
                 function e:same_floor() return true end
                 function e:player_id() return 0 end
                 function e:player_within() return false end
                 function e:joint_bank() return false end
                 function e:pl_damage_cnt() return 0 end
                 function e:player_life() return 200 end
                 function e:voice_free() return false end
                 local zombie = (function() {library} end)()
                 zombie.run(e, {{}})
                 R = {{ w.r[0], w.r[1] }}"
            ))
            .exec()
            .unwrap();
            lua.globals().get("R").unwrap()
        };
        assert_eq!(chase([0x8C00 - 0x10000, 0]), [1, 0xA]);
        assert_ne!(chase([0, 0x8C00 - 0x10000])[1], 0xA);
    }

    #[test]
    fn the_scripts_call_only_what_the_sandbox_has() {
        let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
        for script in SCRIPTS {
            for library in ["math.", "os.", "io.", "utf8.", "debug.", "coroutine."] {
                assert!(!script.contains(library), "{library} isn't in the sandbox");
            }
            lua.load(script).exec().unwrap();
        }
        let method: mlua::Function = lua
            .load("return function(e, name) return e[name] end")
            .eval()
            .unwrap();
        let mut api = Api {
            em: std::ptr::null_mut(),
            l: std::ptr::null_mut(),
        };
        lua.scope(|scope| {
            let e = scope.create_userdata_ref_mut(&mut api)?;
            // Each name once: the lookups share the sandbox's budget.
            let names: std::collections::BTreeSet<String> = SCRIPTS
                .iter()
                .flat_map(|script| script.split("e:").skip(1))
                .map(|call| {
                    call.chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect()
                })
                // The end of a word, in a comment.
                .filter(|name: &String| !name.is_empty())
                .collect();
            for name in names {
                let found: mlua::Value = method.call((&e, name.as_str()))?;
                assert!(!found.is_nil(), "e:{name} has no binding");
            }
            Ok(())
        })
        .unwrap();
    }

    /// A handgun's hit in the first band: the flinch and its blood, then the
    /// leap aside once the clip ends, with no draw.
    #[test]
    fn a_spider_hit_in_the_first_band_leaps_aside() {
        let spider = run_spider_with(0, &[], 4, "ROUTINES = { [0] = 2, 2, 0, 0 }; CLIP_LEN = 3");
        assert_eq!(spider.1, [1]);
        assert_eq!((spider.get("r0"), spider.get("r1")), (1, 9));
        assert_eq!((spider.get("bloods"), spider.get("drawn")), (1, 0));
    }

    /// Past the first band, `BYTE_005286f4` picks the walk, the leap or the
    /// turn.
    #[test]
    fn a_spider_hit_lower_down_picks_its_way_by_rnd() {
        for (draw, routine) in [(0, 1), (1, 9), (5, 5), (7, 5)] {
            let spider = run_spider_with(
                0,
                &[draw],
                4,
                "ROUTINES = { [0] = 2, 2, 0, 0 }; CLIP_LEN = 3; DAMAGE_NO = 4",
            );
            assert_eq!((spider.get("r1"), spider.get("drawn")), (routine, 1));
        }
    }

    /// A magnum's hit takes off the leg `Rnd` picks, its two parts thrown,
    /// while more than two are left, and then it leaps aside.
    #[test]
    fn a_spiders_magnum_hit_takes_a_leg_off() {
        let spider = run_spider_with(
            0,
            &[2],
            4,
            "ROUTINES = { [0] = 2, 5, 0, 0 }; VARS = { [13] = 8 }; CLIP_LEN = 3",
        );
        assert_eq!((spider.get("lost"), spider.get("blown")), (1 << 2, 2));
        assert_eq!((spider.get("r0"), spider.get("r1")), (1, 9));
        let spider = run_spider_with(
            0,
            &[2],
            1,
            "ROUTINES = { [0] = 2, 5, 0, 0 }; VARS = { [13] = 2 }; CLIP_LEN = 3",
        );
        assert_eq!((spider.get("lost"), spider.get("blown")), (0, 0));
        assert_eq!(spider.get("bloods"), 1);
    }

    /// A handgun's hit on a wall: blood 400 out from the wall, then the climb
    /// (`Routine_1` 4) with its quarter turn.
    #[test]
    fn a_wall_spider_hit_climbs_away() {
        let spider = run_spider_with(
            9,
            &[],
            1,
            "ROUTINES = { [0] = 2, 2, 0, 0 }; VARS = { [2] = 3 }; CLIP_LEN = 3",
        );
        assert_eq!((spider.get("r0"), spider.get("r1")), (1, 4));
        assert_eq!((spider.get("bloods"), spider.get("rot_by")), (1, 400));
    }

    /// A handgun's death on the ceiling: 800 down, clip 12 with `Cdir_z`
    /// cleared, then a fall that speeds up by 0x28 a tick until it lies on
    /// `Ground` as a floor spider's body.
    #[test]
    fn a_ceiling_spider_falls_to_the_floor_when_killed() {
        let setup = "ROUTINES = { [0] = 3, 2, 0, 0 }; VARS = { [2] = 1, [30] = 0 }; \
                     POS = { 0, -7200, 0 }; CLIP_LEN = 1000";
        let spider = run_spider_with(0x43, &[0; 8], 1, setup);
        assert_eq!(spider.1, [0xC]);
        assert_eq!((spider.get("r2"), spider.get("pos_y")), (1, -6400));
        assert_eq!((spider.get("cdir_z"), spider.get("bloods")), (0, 1));
        let spider = run_spider_with(0x43, &[0; 8], 40, setup);
        assert_eq!((spider.get("kind_var"), spider.get("pos_y")), (0, 0));
        assert_eq!(spider.get("be") & 0x0400_0042, 0x0400_0042);
    }

    /// A handgun's death in water: it falls on its back, its body floats
    /// at the water's level, and it gets a life of 1 for one more shot.
    #[test]
    fn a_spiders_handgun_death_leaves_a_body_floating() {
        let spider = run_spider_with(
            0,
            &[0x25],
            4,
            "ROUTINES = { [0] = 3, 2, 0, 0 }; CLIP_LEN = 3; SEQ_END = 0x80000; \
             WATER_LEVEL = -180",
        );
        assert_eq!(spider.1, [0xC]);
        assert_eq!((spider.get("r0"), spider.get("life")), (7, 1));
        assert_eq!((spider.get("revived"), spider.get("died")), (1, 1));
        assert_eq!(spider.get("y"), 0x46 - 180);
        // Floating at -110, the body is above the player's 0 too.
        assert_eq!(spider.get("be") & 0x1C00_0000, 0xC00_0000);
        assert_eq!(
            (spider.get("r1"), spider.get("timer2")),
            (1, (0x25 & 0x1F) + 0x1E)
        );
    }

    /// What `FUN_004732a0` sent: each young spider's `Type`, facing and the
    /// init's draw it was given.
    fn sent_young(setup: &str, draws: &[u8]) -> (Vec<(i64, i64, i64)>, i64) {
        let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
        lua.load(setup).exec().unwrap();
        let script = include_str!("../../games/re2/enemy/25/enemy.lua");
        lua.globals().set("DRAWS", draws.to_vec()).unwrap();
        lua.globals().set("KIND", 0).unwrap();
        let table = lua
            .load(format!(
                "{SPIDER_STUB}
                 {script}
                 update(e)
                 local sent = {{}}
                 for _, spawn in ipairs(w.spawns) do
                     sent[#sent + 1] = {{ spawn.kind, spawn.angle, spawn.drawn }}
                 end
                 return {{ sent = sent, drawn = drawn }}"
            ))
            .eval::<mlua::Table>()
            .unwrap();
        let sent = table
            .get::<mlua::Table>("sent")
            .unwrap()
            .sequence_values::<Vec<i64>>()
            .map(|spawn| {
                let spawn = spawn.unwrap();
                (spawn[0], spawn[1], spawn[2])
            })
            .collect();
        (sent, table.get("drawn").unwrap())
    }

    /// A magnum's death on the floor sends `(Rnd & 3) + 6` young, each
    /// facing `Rnd << 4` and with its init's draw made after that.
    #[test]
    fn a_spiders_blown_death_sends_young() {
        let setup = "ROUTINES = { [0] = 3, 5, 0, 0 }; FREE = 32; IN_PLAY = 1";
        let mut draws = vec![0x41, 0x12, 0x34, 0x56, 0x78];
        draws.resize(64, 0);
        let (sent, _) = sent_young(setup, &draws);
        // `(0x41 & 3) + 6`.
        assert_eq!(sent.len(), 7);
        assert_eq!(sent[0], (0x2002, 0x12 << 4, 0x134));
        assert_eq!(sent[1], (0x2002, 0x56 << 4, 0x178));
        // Short of works, it sends what it can; with 17 in play, none.
        let (sent, _) = sent_young(&format!("{setup}; FREE = 2"), &draws);
        assert_eq!(sent.len(), 2);
        let (sent, _) = sent_young(
            "ROUTINES = { [0] = 3, 5, 0, 0 }; FREE = 32; IN_PLAY = 17",
            &draws,
        );
        assert!(sent.is_empty());
        // In water, bit 0 picks the swim.
        let (sent, _) = sent_young(&format!("{setup}; VARS = {{ [14] = -180 }}"), &draws);
        assert_eq!(sent[0].0, 0x2003);
    }

    const YOUNG_STUB: &str = "
        w.old = { 0, 0, 0 }
        w.dc = 0
        function e:old_pos() return w.old[1], w.old[2], w.old[3] end
        function e:set_pos(x, y, z) w.pos = { x, y, z } end
        function e:set_kind(v) w.kind = v end
        function e:damage_cnt() return w.dc end
        function e:set_damage_cnt(v) w.dc = v end
        function e:player_at_w() return 450 end
        function e:player_floor() return 0 end
        function e:pl_routine(i) return w.pl[i] end
        function e:kill() w.be = 0; w.r = { [0] = 0, 0, 0, 0 }; w.died = 1 end
        function e:pl_life_down(down)
            w.pl_life = w.pl_life - down
            return 0
        end
    ";

    fn run_young(kind: u16, draws: &[u8], ticks: usize, setup: &str) -> Spider {
        let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
        lua.load(setup).exec().unwrap();
        let script = include_str!("../../games/re2/enemy/26/enemy.lua");
        lua.globals().set("DRAWS", draws.to_vec()).unwrap();
        lua.globals().set("KIND", kind).unwrap();
        lua.load(format!(
            "{SPIDER_STUB}
             {YOUNG_STUB}
             {script}
             for _ = 1, {ticks} do update(e) end
             return {{ r0 = w.r[0], r1 = w.r[1], r2 = w.r[2], r3 = w.r[3], life = w.life,
                 be = w.be, kind = w.kind, angle = w.angle, spd = w.spd[1], spd_y = w.spd[2],
                 timer = w.timers[0], drawn = drawn, t21c = w.vars[0] or 0,
                 wobble = w.vars[1] or 0, rate = w.vars[4] or 0, pos_y = w.pos[2],
                 effects = w.bloods, sounds = #w.sounds, died = w.died, pl0 = w.pl[0],
                 pl_life = w.pl_life, pl_dc = w.pl_dc, bit = w.vars[2] or 0 }}"
        ))
        .eval::<mlua::Table>()
        .map(|table| {
            let fields = "r0 r1 r2 r3 life be kind angle spd spd_y timer drawn t21c wobble \
                rate pos_y effects sounds died pl0 pl_life pl_dc bit"
                .split_whitespace()
                .map(|name| (name.to_string(), table.get::<i64>(name).unwrap()))
                .collect();
            Spider(fields, Vec::new())
        })
        .unwrap()
    }

    /// `FUN_004736a0`: a life of 1, `Routine_1` by `Type`, and the wait
    /// before a jump from `Rnd`, or from the draw the spider made for it.
    #[test]
    fn a_young_spiders_init() {
        let young = run_young(0x2002, &[0x3F], 1, "");
        assert_eq!((young.get("r0"), young.get("r1")), (1, 1));
        assert_eq!((young.get("life"), young.get("t21c")), (1, 0x1F));
        assert_eq!((young.get("wobble"), young.get("drawn")), (0x78, 1));
        assert_eq!(young.get("be"), 0x4C0_0001);
        let young = run_young(0x2002, &[0x3F], 1, "VARS = { [6] = 0x125 }");
        assert_eq!((young.get("t21c"), young.get("drawn")), (5, 0));
        // The room's template only poses.
        let young = run_young(0x2000, &[0], 1, "");
        assert_eq!((young.get("r1"), young.get("life")), (0, -1));
        assert_eq!(young.get("kind"), 0x6000);
        assert_eq!(young.get("be"), 0x4C8_044B);
        // Sent from a wall or the ceiling, it falls; from water, it swims.
        assert_eq!(run_young(0x2004, &[0], 1, "").get("r1"), 2);
        assert_eq!(run_young(0x2003, &[0], 1, "").get("r1"), 4);
    }

    /// The walk's wait counts `(Rnd + 0x1E) & 0x3F`, then the run goes at
    /// `(Rnd & 0x3F) + 0x5A` for `Rnd & 0x4E` ticks, swaying by 0x78.
    #[test]
    fn a_young_spider_waits_then_runs() {
        // Init, then the wait: 2 ticks, and toward the player on draw 1.
        let draws = [0, 0x24, 1, 0x10, 0x0E, 0, 0, 0, 0];
        let young = run_young(0x2002, &draws, 2, "");
        assert_eq!((young.get("r2"), young.get("timer")), (0, 1));
        assert_eq!(young.get("rate"), 0x40);
        let young = run_young(0x2002, &draws, 4, "");
        assert_eq!((young.get("r2"), young.get("r3")), (1, 0));
        let young = run_young(0x2002, &draws, 5, "");
        assert_eq!((young.get("r3"), young.get("spd")), (1, 0x10 + 0x5A));
        assert_eq!(young.get("timer"), 0x0E - 1);
        assert_eq!(young.get("wobble"), -0x78);
        // A draw of 0 for the way runs away, as `1 << 0` is in 0x1111.
        let young = run_young(0x2002, &[0, 0x24, 0], 2, "");
        assert_eq!(young.get("rate"), 0xC0);
    }

    /// With the player in reach and faced, the jump starts at -300 up and
    /// bites once on the way.
    #[test]
    fn a_young_spider_jumps_and_bites() {
        let setup = "DIST = 300; TARGET = 0; POS = { 0, 0, 0 }";
        let young = run_young(0x2002, &[0; 16], 2, setup);
        assert_eq!(young.get("r1"), 3);
        let young = run_young(0x2002, &[0; 16], 3, setup);
        assert_eq!(young.get("spd_y"), -300 + 0x28);
        assert_eq!((young.get("pl0"), young.get("pl_dc")), (2, 0x80));
        assert_eq!((young.get("pl_life"), young.get("bit")), (199, 1));
        assert_eq!(young.get("angle"), 0x800);
    }

    /// Off the floor it falls at up to 800 a tick and lands on `Ground`.
    #[test]
    fn a_young_spider_falls_to_ground() {
        let young = run_young(0x2004, &[0; 16], 2, "POS = { 0, -500, 0 }");
        assert_eq!((young.get("r1"), young.get("spd_y")), (2, 0x50));
        let young = run_young(0x2004, &[0; 16], 8, "POS = { 0, -500, 0 }");
        assert_eq!((young.get("r1"), young.get("pos_y")), (1, 0));
        assert_eq!(young.get("be") & 0x400_0002, 0x400_0000);
    }

    /// The death: sound 7 and an effect, and the work goes the tick after.
    #[test]
    fn a_young_spider_dies() {
        let setup = "ROUTINES = { [0] = 3, 2, 0, 0 }";
        let young = run_young(0x2002, &[], 1, setup);
        assert_eq!((young.get("sounds"), young.get("effects")), (1, 1));
        assert_eq!(young.get("died"), 0);
        let young = run_young(0x2002, &[], 2, setup);
        assert_eq!((young.get("died"), young.get("be")), (1, 0));
    }

    /// A stand-in for the work, enough for `enemy/23/enemy.lua`'s init, wait,
    /// entry and swim. Each clip ends after `CLIP_LEN` steps.
    const ALLIGATOR_STUB: &str = "
        w = { r = { [0] = 0, 0, 0, 0 }, vars = {}, be = 1, angle = 1024,
            pos = { 5706, 3000, -5316 }, move_no = 0, move_cnt = 0, status = 0,
            cleared = 0, cuts = {}, steps = 0, sce = SCE, moved = 0 }
        e = {}
        function e:routine(i) return w.r[i] end
        function e:set_routine(i, v) w.r[i] = v end
        function e:routines(a, b, c, d) w.r = { [0] = a, b, c, d } end
        function e:var(i) return w.vars[i] or 0 end
        function e:set_var(i, v) w.vars[i] = v end
        function e:be_flg() return w.be end
        function e:set_be_flg(v) w.be = v end
        function e:set_life(v) w.life = v end
        function e:move_no() return w.move_no end
        function e:move_cnt() return w.move_cnt end
        function e:set_move_no(v) w.move_no = v end
        function e:clip(n, c) w.move_no = n; w.move_cnt = c; w.steps = 0 end
        function e:joint()
            w.steps = w.steps + 1
            w.move_cnt = w.move_cnt + 1
            return w.steps >= CLIP_LEN
        end
        function e:add_speed_xz() w.moved = w.moved + 1 end
        function e:angle() return w.angle end
        function e:set_angle(v) w.angle = v end
        function e:pos() return w.pos[1], w.pos[2], w.pos[3] end
        function e:set_pos(x, y, z) w.pos = { x, y, z } end
        function e:sce_free(i) return w.sce end
        function e:set_status(bits) w.status = w.status | bits end
        function e:clear_status(bits) w.cleared = w.cleared | bits; w.status = w.status & ~bits end
        function e:status(bits) return w.status & bits ~= 0 end
        function e:cut_change(cut) w.cuts[#w.cuts + 1] = cut end
        function e:distance() return DIST end
        function e:now_seq() return 0 end
        function e:neck_flags() return 0 end
        function e:object_pos() return 1836, -1800, -21254 end
        function e:flag() return false end
        w.jaws = {}
        w.pl = { 1, 0, 0, 0 }
        w.downs = {}
        w.draws = 0
        w.effects = 0
        function e:set_jaw(six, seven) w.jaws[#w.jaws + 1] = { six, seven } end
        function e:player_touched() return TOUCH end
        w.dc = 0
        function e:pl_damage_cnt() return w.dc end
        -- The player's own tick sets its `Damage_cnt` as the throw starts.
        function e:pl_hold() w.dc = 0x80 end
        function e:cdir_player() return true end
        function e:pl_set_routine(a, b, c, d) w.pl = { a, b, c, d } end
        function e:pl_set_routine_at(i, v) w.pl[i + 1] = v end
        function e:pl_life_down(down) w.downs[#w.downs + 1] = down return 0 end
        function e:rnd() w.draws = w.draws + 1 return 0 end
        function e:call_effect() w.effects = w.effects + 1 end
        function e:player_pos() return 0, -1800, 20000 end
        function e:direction_from_player() return 0 end
        function e:own_part() return 0, 0, 0 end
        function e:hypot() return 20000 end
        for _, name in ipairs({ 'set_damage_flg', 'set_sce_flg', 'set_neck_flags',
            'set_body_box', 'set_body_ofs_x', 'spd', 'seq_sound', 'sound',
            'player_goto_angle', 'rot_neck_part' }) do
            e[name] = function() end
        end
    ";

    struct Alligator(std::collections::HashMap<String, i64>, Vec<u8>);

    impl Alligator {
        fn get(&self, name: &str) -> i64 {
            self.0[name]
        }
    }

    fn run_alligator(ticks: usize, setup: &str) -> Alligator {
        let (lua, budget) = crate::lua::new_sandboxed_state().unwrap();
        let script = include_str!("../../games/re2/enemy/23/enemy.lua");
        budget.begin();
        lua.load(format!(
            "SCE = 0 DIST = 20000 CLIP_LEN = 4 TOUCH = false {setup}
             {ALLIGATOR_STUB}
             {script}"
        ))
        .exec()
        .unwrap();
        // A tick at a time, each with its own budget, as the engine runs them.
        for _ in 0..ticks {
            budget.begin();
            lua.load("update(e)").exec().unwrap();
        }
        budget.begin();
        lua.load(
            "return { r0 = w.r[0], r1 = w.r[1], r2 = w.r[2], life = w.life, be = w.be,
                 cuts = w.cuts, angle = w.angle, z = w.pos[3], move_no = w.move_no,
                 status = w.status, cleared = w.cleared, rest = w.vars[15] or 0,
                 moved = w.moved, draws = w.draws, effects = w.effects,
                 pl1 = w.pl[2], down = w.downs[1] or 0, downs = #w.downs,
                 jaw = #w.jaws > 0 and (w.jaws[#w.jaws][1] * 0x10000 + (w.jaws[#w.jaws][2] & 0xFFFF)) or 0 }",
        )
        .eval::<mlua::Table>()
        .map(|table| {
            let cuts = table
                .get::<mlua::Table>("cuts")
                .unwrap()
                .sequence_values()
                .map(Result::unwrap)
                .collect();
            let fields = [
                "r0", "r1", "r2", "life", "be", "angle", "z", "move_no", "status", "cleared",
                "rest", "moved", "draws", "effects", "pl1", "down", "downs", "jaw",
            ]
            .into_iter()
            .map(|name| (name.to_string(), table.get::<i64>(name).unwrap()))
            .collect();
            Alligator(fields, cuts)
        })
        .unwrap()
    }

    /// `FUN_00468a10`: a life of 25300 and `Be_flg`'s `0x1c00004c`; then the
    /// entry waits for the room's scene, with `Mot_play_init`'s `Move_no`
    /// 0xFF from the init's change of routines.
    #[test]
    fn an_alligator_waits_for_the_scene() {
        let alligator = run_alligator(1, "");
        assert_eq!(alligator.get("life"), 25300);
        assert_eq!(alligator.get("be"), 0x1C00_004D);
        assert_eq!(alligator.get("move_no"), 0xFF);
        let alligator = run_alligator(50, "");
        assert_eq!(
            (
                alligator.get("r0"),
                alligator.get("r1"),
                alligator.get("r2")
            ),
            (1, 0, 0)
        );
        assert_eq!(alligator.get("moved"), 0);
    }

    /// `FUN_00468d30`: `Sce_free0` starts clip 6 under `fg_status`'s `0x110`;
    /// its end cuts to camera 1 and puts it at `z` -21212, and clip 3's end
    /// turns it a quarter, clears `0x110` and rests for 110 ticks.
    #[test]
    fn an_alligator_enters_when_the_scene_says() {
        let alligator = run_alligator(3, "SCE = 1");
        assert_eq!(alligator.get("r2"), 1);
        assert_eq!(alligator.get("status"), 0x110);
        assert_eq!(alligator.get("move_no"), 6);
        // Tick 2 starts the entry, and clip 6's fourth step, on tick 6, ends it.
        let alligator = run_alligator(6, "SCE = 1");
        assert_eq!(alligator.get("r2"), 2);
        assert_eq!(alligator.1, [1]);
        assert_eq!(alligator.get("z"), -0x52DC);
        let alligator = run_alligator(10, "SCE = 1");
        assert_eq!(
            (
                alligator.get("r0"),
                alligator.get("r1"),
                alligator.get("r2")
            ),
            (1, 1, 0)
        );
        assert_eq!(alligator.get("angle"), 1024 + 0x400);
        assert_eq!(alligator.get("cleared"), 0x110);
        assert_eq!(alligator.get("rest"), 0x6E - 1);
    }

    /// `FUN_00468e20`: the rest runs out, then it swims (`Routine_1` 2) while
    /// the player is more than 90 away.
    #[test]
    fn an_alligator_swims_once_its_rest_runs_out() {
        let alligator = run_alligator(10 + 0x6D, "SCE = 1");
        assert_eq!(alligator.get("r1"), 1);
        let alligator = run_alligator(10 + 0x6E, "SCE = 1");
        assert_eq!(alligator.get("r1"), 2);
        assert_eq!(alligator.get("move_no"), 1);
        let alligator = run_alligator(10 + 0x6E, "SCE = 1 DIST = 50");
        assert_eq!(alligator.get("r1"), 1);
    }

    /// `FUN_00469480`: once the player has touched it, the jaws open and
    /// close over eight ticks (as the original's `+0x22a` and `+0x228` went
    /// in room 40A), then snap: `FUN_00469630` throws the player on from
    /// behind (`Routine` 5,3) for `Pl_life_down(40)`, and `FUN_0046b1c0`
    /// bleeds on three ticks, four draws of `Rnd` each.
    #[test]
    fn an_alligators_jaws_snap_at_the_player() {
        let jaws = [
            (1, -6),
            (5, -25),
            (11, -56),
            (20, -100),
            (29, -144),
            (35, -175),
            (39, -194),
            (40, -200),
            (38, -189),
        ];
        // The entry ends on tick 10; the rest's first tick, 11, sees the touch.
        for (tick, (six, seven)) in jaws.into_iter().enumerate() {
            let alligator = run_alligator(11 + tick, "SCE = 1 TOUCH = true");
            let jaw = alligator.get("jaw");
            assert_eq!(
                ((jaw >> 16) as i16, jaw as i16),
                (six, seven),
                "tick {tick}"
            );
        }
        let alligator = run_alligator(19, "SCE = 1 TOUCH = true");
        assert_eq!(alligator.get("downs"), 1, "the snap's first tick");
        assert_eq!(alligator.get("down"), 0x28);
        assert_eq!(alligator.get("pl1"), 3);
        assert_eq!((alligator.get("draws"), alligator.get("effects")), (4, 1));
        let alligator = run_alligator(21, "SCE = 1 TOUCH = true");
        assert_eq!((alligator.get("draws"), alligator.get("effects")), (12, 3));
        let alligator = run_alligator(30, "SCE = 1 TOUCH = true");
        assert_eq!(alligator.get("draws"), 12);
    }

    /// `FUN_004697f0`: once its swim reaches the gas tank, it raises
    /// `fg_room`'s 0x1d, opens its jaws over 13 ticks pushing the tank ahead,
    /// takes it in them and settles it 1750 ahead and 210 up on part 6 over
    /// 26 more.
    #[test]
    fn an_alligator_takes_the_gas_tank_in_its_jaws() {
        let (lua, budget) = crate::lua::new_sandboxed_state().unwrap();
        let script = include_str!("../../games/re2/enemy/23/enemy.lua");
        budget.begin();
        lua.load(format!(
            "SCE = 0 DIST = 20000 CLIP_LEN = 4 TOUCH = false
             {ALLIGATOR_STUB}
             {script}
             w.flags = {{}}
             w.tank = {{ -22816, -1984, -14000 }}
             function e:flag() return false end
             function e:set_flag(bank, index, on) w.flags[#w.flags + 1] = bank * 0x100 + index end
             function e:object_pos() return w.tank[1], w.tank[2], w.tank[3] end
             function e:set_object_pos(_, x, y, z) w.tank = {{ x, y, z }} end
             function e:part_local(_, x, y, z) return x - 1000, y, z end
             function e:drawn_part_point(_, x, y, z) return x, y, z end
             update(e)
             w.pos = {{ -24299, -1800, -18000 }}
             w.r = {{ [0] = 1, 1, 0, 0 }}
             w.vars[7] = 1"
        ))
        .exec()
        .unwrap();
        let state = |lua: &mlua::Lua| -> (i64, i64, i64, i64) {
            lua.load("return w.vars[7], w.tank[3], w.vars[24] or 0, w.vars[25] or 0")
                .eval()
                .unwrap()
        };
        budget.begin();
        lua.load("update(e)").exec().unwrap();
        // Pushed to 0x1FA4 ahead of it, and `fg_room`'s 0x1d raised.
        assert_eq!(state(&lua), (2, -18000 + 0x1FA4, 0, 0));
        assert_eq!(lua.load("return w.flags[1]").eval::<i64>().unwrap(), 0x51D);
        for _ in 0..12 {
            budget.begin();
            lua.load("update(e)").exec().unwrap();
        }
        // Taken on part 6 on the 13th tick.
        assert_eq!(state(&lua), (3, -18000 + 0x1FA4, -22816 - 1000, -1984));
        for _ in 0..26 {
            budget.begin();
            lua.load("update(e)").exec().unwrap();
        }
        assert_eq!(state(&lua).0, 7);
        budget.begin();
        lua.load("update(e)").exec().unwrap();
        let (_, _, x, y) = state(&lua);
        // Settling on 1750 and -210, by 100 and 0x1E a tick.
        assert!(x > -23816 && y > -1984, "{x} {y}");
        // Where the original's jaws (`+0x22a`, `+0x228`) rest with the tank.
        let jaws: (i64, i64) = lua.load("return w.vars[10], w.vars[9]").eval().unwrap();
        assert_eq!(jaws, (34, -161));
    }

    /// A cockroach for `enemy/29/enemy.lua`: `DRAWS` feeds `Rnd`, a clip
    /// ends on its `CLIP_LEN`th frame, the player stands at `PLAYER` and an
    /// attach point is at `PART_AT` (or nowhere). The turns, the
    /// trigonometry and `SquareRoot0` are the engine's own.
    const ROACH_STUB: &str = "
        local draws, drawn = DRAWS or {}, 0
        local clip_len = CLIP_LEN or 1000
        w = { r = ROUTINES or { [0] = 0, 0, 0, 0 }, vars = VARS or {},
            timers = { [0] = 0, 0, 0 }, be = 1, kind = 0, life = 0, angle = ANGLE or 0,
            cdir = { 0, 0 }, spd = { 0, 0, 0 }, pos = POS or { -15372, -3600, -25133 },
            clip = -1, frame = 0, hidden = 0, sounds = {}, faced = {},
            swarm = SWARM or { [0] = 0, 0, 0, 0 } }
        local player = PLAYER or { -7824, 0, -25140 }
        local e = {}
        function e:routine(i) return w.r[i] end
        function e:set_routine(i, v) w.r[i] = v end
        function e:routines(a, b, c, d) w.r = { [0] = a, b, c, d } end
        function e:var(i) return w.vars[i] or 0 end
        function e:set_var(i, v) w.vars[i] = v end
        function e:timer(i) return w.timers[i] end
        function e:set_timer(i, v) w.timers[i] = i == 2 and v & 0xFF or v & 0xFFFF end
        function e:rnd() drawn = drawn + 1; return draws[drawn] + 0 end
        function e:be_flg() return w.be end
        function e:set_be_flg(v) w.be = v end
        function e:kind() return w.kind end
        function e:set_kind(v) w.kind = v end
        function e:set_life(v) w.life = v end
        function e:swarm(i) return w.swarm[i] end
        function e:set_swarm(i, v) w.swarm[i] = v end
        function e:move_no() return w.clip end
        function e:clip(n) w.clip = n; w.frame = 0 end
        function e:joint() w.frame = w.frame + 1; return w.frame >= clip_len end
        function e:hidden_parts() return w.hidden end
        function e:set_hidden_parts(v) w.hidden = v end
        function e:init_super() w.super = nil end
        function e:set_super(x, y, z) w.super = { x, y, z } end
        function e:angle() return w.angle end
        function e:set_angle(v) w.angle = v & 0xFFFF end
        function e:cdir() return w.cdir[1], w.cdir[2] end
        function e:set_cdir(x, z) w.cdir = { x, z } end
        function e:pos() return w.pos[1], w.pos[2], w.pos[3] end
        function e:set_pos(x, y, z) w.pos = { x, y, z } end
        function e:speed() return w.spd[1], w.spd[2], w.spd[3] end
        function e:set_speed(x, y, z) w.spd = { i16(x), i16(y), i16(z) } end
        function e:sound(n) w.sounds[#w.sounds + 1] = n end
        function e:player_pos() return player[1], player[2], player[3] end
        function e:player_part_point(part, x, y, z)
            w.part = part
            if PART_AT then return PART_AT[1], PART_AT[2], PART_AT[3] end
            return nil, nil, nil
        end
        function e:direction(x0, z0, x1, z1) return rs_direction(x0, z0, x1, z1) end
        function e:rsin(a) return rs_rsin(a) end
        function e:rcos(a) return rs_rcos(a) end
        function e:turn_to(to, rate) return rs_turn(w.angle, to, rate) end
        function e:rotate(a, x, z) return rs_rotate(a, x, z) end
        function e:hypot(x, z) return rs_hypot(x, z) end
        function e:goto_point(x, z, rate)
            w.faced[#w.faced + 1] = rate
            w.angle = rs_goto(w.pos[1], w.pos[3], w.angle, x, z, rate)
        end
        function e:add_speed_xz()
            local dx, dz = rs_rotate(w.angle, w.spd[1], w.spd[3])
            w.pos = { w.pos[1] + dx, w.pos[2], w.pos[3] + dz }
        end
        for _, name in ipairs({ 'set_damage_flg', 'set_sce_flg', 'set_body_box', 'set_shadow',
            'seq_sound' }) do
            e[name] = function() end
        end
        E = e
        function drawn_count() return drawn end
    ";

    /// The roach's script on `ROACH_STUB`, with `setup`'s globals set first.
    struct Roach {
        lua: mlua::Lua,
        budget: std::rc::Rc<crate::lua::Budget>,
    }

    impl Roach {
        fn eval<T: mlua::FromLuaMulti>(&self, code: &str) -> T {
            self.budget.begin();
            self.lua.load(code).eval().unwrap()
        }

        fn run(&self, code: &str) {
            self.budget.begin();
            self.lua.load(code).exec().unwrap();
        }
    }

    fn spawn_roach(draws: &[u8], setup: &str) -> Roach {
        let (lua, budget) = crate::lua::new_sandboxed_state().unwrap();
        let globals = lua.globals();
        let set = |name: &str, function: mlua::Function| globals.set(name, function).unwrap();
        set(
            "rs_direction",
            lua.create_function(|_, (a, b, c, d): (i32, i32, i32, i32)| {
                Ok(direction_ck(a, b, c, d))
            })
            .unwrap(),
        );
        set(
            "rs_rsin",
            lua.create_function(|_, a: i32| Ok(player2::rsin(a)))
                .unwrap(),
        );
        set(
            "rs_rcos",
            lua.create_function(|_, a: i32| Ok(player2::rcos(a)))
                .unwrap(),
        );
        set(
            "rs_turn",
            lua.create_function(|_, (a, to, rate): (i64, i64, i32)| {
                Ok(turn_within(a as u16, to as u16, rate))
            })
            .unwrap(),
        );
        set(
            "rs_rotate",
            lua.create_function(|_, (a, x, z): (i64, i32, i32)| {
                Ok(player2::rotate(a as u16, x, z))
            })
            .unwrap(),
        );
        set(
            "rs_hypot",
            lua.create_function(|_, (x, z): (i32, i32)| {
                Ok(square_root0(
                    x.wrapping_mul(x).wrapping_add(z.wrapping_mul(z)),
                ))
            })
            .unwrap(),
        );
        set(
            "rs_goto",
            lua.create_function(
                |_, (x, z, a, tx, tz, rate): (i32, i32, i64, i32, i32, i32)| {
                    Ok(goto(a as u16, direction_ck(x, z, tx, tz), rate))
                },
            )
            .unwrap(),
        );
        globals.set("DRAWS", draws.to_vec()).unwrap();
        lua.load(setup).exec().unwrap();
        let script = include_str!("../../games/re2/enemy/29/enemy.lua");
        lua.load(format!(
            "local function i16(v) v = v & 0xFFFF; return v >= 0x8000 and v - 0x10000 or v end
             {ROACH_STUB}
             {script}
             function tick(n) for _ = 1, n or 1 do update(E) end end"
        ))
        .exec()
        .unwrap();
        Roach { lua, budget }
    }

    const ROUTINES: &str = "return { w.r[0], w.r[1], w.r[2], w.r[3] }";

    /// `FUN_0047c0e0`: the wait's routines, a life of 10, `Type`'s `0x2000`,
    /// the wings hidden, one draw for `Cdir_y`, the work's fields and the
    /// swarm's cleared, and the body on the pipe's wall at the angle it was
    /// at from the axis.
    #[test]
    fn a_roachs_init() {
        let roach = spawn_roach(
            &[0x37],
            "VARS = { [0] = 5, 6, 7, 8, 9, 10, 11, 12, 13, 14 }; SWARM = { [0] = 1, 2, 3, 4 }",
        );
        roach.run("tick()");
        assert_eq!(roach.eval::<[u8; 4]>(ROUTINES), [1, 0, 0, 0]);
        let (life, kind, angle): (i64, i64, i64) = roach.eval("return w.life, w.kind, w.angle");
        assert_eq!((life, kind, angle), (10, 0x2000, 0x370));
        let (be, hidden, drawn): (i64, i64, i64) =
            roach.eval("return w.be, w.hidden, drawn_count()");
        assert_eq!(be & 0x1E80_0002, 0x1E80_0002);
        assert_eq!((hidden, drawn), (0xC, 1));
        let swarm: Vec<i64> =
            roach.eval("return { w.swarm[0], w.swarm[1], w.swarm[2], w.swarm[3] }");
        assert_eq!(swarm, [0; 4]);
        let vars: Vec<i64> = roach.eval(
            "return { w.vars[1], w.vars[2], w.vars[3], w.vars[4], w.vars[5], w.vars[6],
                w.vars[7], w.vars[8], w.vars[9] }",
        );
        assert_eq!(vars, [0; 9]);
        // From (-15372, -3600, -25133), onto the circle, each coordinate
        // rounded toward zero.
        let (x, y, z, a, surface): (i32, i32, i32, i32, Vec<i64>) =
            roach.eval("return w.pos[1], w.pos[2], w.pos[3], w.vars[0], w.super");
        assert_eq!(a, i32::from(direction_ck(-25200, -1800, -25133, -3600)));
        assert_eq!(x, -15372);
        assert_eq!(z, player2::rcos(a) * 1800 / 4096 - 25200);
        assert_eq!(y, -1800 - player2::rsin(a) * 1800 / 4096);
        assert_eq!(surface, [0, 0xC00, i64::from((0xC00 - a) as i16)]);
    }

    /// The wait: clip 1, then a draw picks the crawl or clip 0, which a
    /// draw ends unless it has ended itself.
    #[test]
    fn a_roach_waits_then_crawls() {
        let wait = |draws: &[u8], sub: u8, clip_len: u8| -> ([u8; 4], i64) {
            let roach = spawn_roach(
                draws,
                &format!("ROUTINES = {{ [0] = 1, 0, {sub}, 0 }}; CLIP_LEN = {clip_len}"),
            );
            roach.run("tick()");
            (roach.eval(ROUTINES), roach.eval("return drawn_count()"))
        };
        assert_eq!(wait(&[0], 0, 1), ([1, 1, 0, 0], 1));
        assert_eq!(wait(&[2], 0, 1), ([1, 0, 1, 0], 1));
        assert_eq!(wait(&[], 0, 5), ([1, 0, 0, 0], 0));
        // Clip 0 not ended: a draw of 2 goes back to clip 1, of 0 crawls.
        assert_eq!(wait(&[2], 1, 5), ([1, 0, 0, 0], 1));
        assert_eq!(wait(&[0], 1, 5), ([1, 1, 0, 0], 1));
        // Clip 0's end draws nothing.
        assert_eq!(wait(&[], 1, 1), ([1, 0, 0, 0], 0));
    }

    /// The crawl's decision costs one, two or three draws: a jump only from
    /// the pipe's upper half.
    #[test]
    fn a_roachs_crawl_decides() {
        let decide = |draws: &[u8], y: i32| -> ([u8; 4], i64, i64, i64) {
            let roach = spawn_roach(
                draws,
                &format!(
                    "ROUTINES = {{ [0] = 1, 1, 0, 0 }}; POS = {{ -15000, {y}, -25200 }}; ANGLE = 0x100"
                ),
            );
            roach.run("tick()");
            let (drawn, sounds, heading) =
                roach.eval("return drawn_count(), #w.sounds, w.vars[2] or 0");
            (roach.eval(ROUTINES), drawn, sounds, heading)
        };
        assert_eq!(decide(&[0, 0], -3000), ([1, 2, 0, 0], 2, 0, 0));
        assert_eq!(decide(&[0, 1], -3000), ([1, 0, 0, 0], 2, 0, 0));
        assert_eq!(decide(&[0], -600), ([1, 0, 0, 0], 1, 0, 0));
        let heading = (0x50 * 8 - 0x400 + 0x100) & 0xFFFF;
        let (routines, drawn, sounds, to) = decide(&[1, 2, 0x50], -600);
        assert_eq!((routines[1], drawn, sounds, to), (1, 3, 1, heading));
        assert_eq!(decide(&[1, 1, 0x50], -600).2, 0);
    }

    /// The crawl's step: the speed builds by 0x14 to 140, the heading turns
    /// by 0x20 with `Goto01_ck`'s window, and the angle round the pipe moves
    /// by the arc's length, toward zero.
    #[test]
    fn a_roachs_crawl_steps_along_the_pipe() {
        let roach = spawn_roach(
            &[],
            "ROUTINES = { [0] = 1, 1, 2, 0 }; VARS = { [2] = 0x800 }",
        );
        let speeds: Vec<i64> = (0..9)
            .map(|_| roach.eval("tick(); return w.spd[1]"))
            .collect();
        assert_eq!(speeds, [20, 40, 60, 80, 100, 120, 140, 140, 140]);
        // Within the window it goes back to the decision.
        let edge = |heading: i64| -> (i64, i64) {
            spawn_roach(
                &[],
                &format!(
                    "ROUTINES = {{ [0] = 1, 1, 2, 0 }}; ANGLE = 0x400; VARS = {{ [2] = {heading} }}"
                ),
            )
            .eval("tick(); return w.r[2], w.angle")
        };
        assert_eq!(edge(0x400 - 0x20), (1, 0x400));
        assert_eq!(edge(0x400 + 0x20), (2, 0x420));
        // Facing -z at 140, `vz` is -140: the step is -50, not -51.
        let roach = spawn_roach(
            &[],
            "ROUTINES = { [0] = 1, 1, 2, 0 }; ANGLE = 0x800; VARS = { [0] = 0x810, [2] = 0x800 }",
        );
        roach.run("w.spd = { 120, 0, 0 }; tick()");
        let (step, angle): (i64, i32) = roach.eval("return w.vars[1], w.vars[0]");
        assert_eq!((step, angle), (-50, 0x810 - 50));
        // Where `rcos` is negative, z and y still round toward zero.
        let (y, z): (i32, i32) = roach.eval("return w.pos[2], w.pos[3]");
        assert!(player2::rcos(angle) < 0);
        assert_eq!(z, player2::rcos(angle) * 1800 / 4096 - 25200);
        assert_eq!(y, -1800 - player2::rsin(angle) * 1800 / 4096);
        // The angle round the pipe wraps at 0xFFF.
        let roach = spawn_roach(
            &[],
            "ROUTINES = { [0] = 1, 1, 2, 0 }; ANGLE = 0; VARS = { [0] = 0x10, [2] = 0 }",
        );
        roach.run("w.spd = { 120, 0, 0 }; tick()");
        assert_eq!(roach.eval::<i64>("return w.vars[0]"), 0x10 + 50);
        let roach = spawn_roach(
            &[],
            "ROUTINES = { [0] = 1, 1, 2, 0 }; ANGLE = 0x800; VARS = { [0] = 0x10, [2] = 0x800 }",
        );
        roach.run("w.spd = { 120, 0, 0 }; tick()");
        assert_eq!(roach.eval::<i64>("return w.vars[0]"), 0x1000 + 0x10 - 50);
    }

    /// `Pos_x` stays within the pipe's ends while the roach is on it.
    #[test]
    fn a_roach_on_the_pipe_is_held_between_its_ends() {
        let x = |flags: i64, x: i64| -> i64 {
            spawn_roach(
                &[],
                &format!(
                    "ROUTINES = {{ [0] = 1, 9, 0, 0 }}; VARS = {{ [3] = {flags} }}; POS = {{ {x}, 0, 0 }}"
                ),
            )
            .eval("tick(); return w.pos[1]")
        };
        assert_eq!(
            (x(8, -7000), x(8, -28000), x(8, -9000)),
            (-7300, -27000, -9000)
        );
        assert_eq!((x(0, -7000), x(0, -28000)), (-7000, -28000));
    }

    /// `FUN_0047d150`: `Rnd >> 3` among the free points, else the scan, which
    /// sets the index in the mask and not its bit.
    #[test]
    fn a_roach_picks_a_free_attach_point() {
        let pick = |draws: &[u8], taken: u32| -> (i64, i64, i64) {
            spawn_roach(
                draws,
                &format!("ROUTINES = {{ [0] = 1, 2, 0, 0 }}; SWARM = {{ [0] = 0, 0, {taken}, 0 }}"),
            )
            .eval("tick(); return w.vars[7], w.swarm[2], drawn_count()")
        };
        assert_eq!(pick(&[0x18], 0), (3, 0x8, 1));
        assert_eq!(pick(&[0x18, 0x20], 0x8), (4, 0x18, 2));
        assert_eq!(pick(&[0; 32], 0xFFFF_FFF7), (3, 0xFFFF_FFF7, 32));
        assert_eq!(pick(&[0; 32], 0xFFFF_FFFF), (0x20, 0xFFFF_FFFF, 32));
    }

    /// The jump: its first tick shows the wings, picks the point and plays
    /// clip 2 at once, and the clip's end faces the player. The flight speeds
    /// up by `Timer1`, climbs toward the point at up to 100 a tick, falls once
    /// `Timer0` reaches 0x5A, and beats its wings every 16 ticks.
    #[test]
    fn a_roachs_jump_flies_at_the_player() {
        let roach = spawn_roach(
            &[0x18],
            "ROUTINES = { [0] = 1, 2, 0, 0 }; CLIP_LEN = 1; POS = { -10000, -3500, -25200 }",
        );
        roach.run("w.hidden = 0xC; tick()");
        let (sub, hidden, clip, faced, sounds): (i64, i64, i64, Vec<i64>, Vec<i64>) =
            roach.eval("return w.r[2], w.hidden, w.clip, w.faced, w.sounds");
        assert_eq!(
            (sub, hidden, clip, faced, sounds),
            (2, 0, 2, vec![0x800], vec![3])
        );
        assert_eq!(
            roach.eval::<(i64, i64)>("return w.vars[3], w.be & 0x2000"),
            (9, 0x2000)
        );
        roach.run("PART_AT = { -12000, -500, -25200 }");
        let (speeds, climbs): (Vec<i64>, Vec<i64>) = (0..25)
            .map(|_| roach.eval::<(i64, i64)>("tick(); return w.spd[1], w.spd[2]"))
            .unzip();
        assert_eq!(&speeds[..5], [0, 1, 3, 6, 10]);
        assert_eq!(*speeds.last().unwrap(), 210);
        assert_eq!(climbs[0], 100);
        assert_eq!(roach.eval::<Vec<i64>>("return w.sounds"), [3, 3]);
        // `Timer0` stops at 0x5A, and then it falls.
        let roach = roach_flying("POS = { -10000, -1800, -25200 }");
        roach.run("w.timers[0] = 0x59");
        assert_eq!(
            roach.eval::<(i64, i64)>("tick(); return w.timers[0], w.spd[2]"),
            (0x5A, 1)
        );
        assert_eq!(
            roach.eval::<(i64, i64)>("tick(); return w.timers[0], w.spd[2]"),
            (0x5A, 2)
        );
    }

    /// A roach in the flight, its point far off.
    fn roach_flying(setup: &str) -> Roach {
        spawn_roach(
            &[],
            &format!(
                "ROUTINES = {{ [0] = 1, 2, 2, 0 }}; PART_AT = {{ -10000, -1800, -15200 }};
                 VARS = {{ [3] = 9, [7] = 4 }}; SWARM = {{ [0] = 0, 0, 0x30, 0 }}; {setup}"
            ),
        )
    }

    /// Beyond the radius it slides along the wall, or once `Timer0` is past
    /// 0x59 goes back onto it, its point freed.
    #[test]
    fn a_roachs_flight_ends_on_the_wall() {
        let roach = roach_flying("POS = { -10000, -1800, -27100 }");
        roach.run("w.timers[0] = 0x10; tick()");
        let (z, move_kind): (i64, i64) = roach.eval("return w.pos[3], w.r[1]");
        // Back on the circle, within `SquareRoot0`'s error.
        assert_eq!(move_kind, 2);
        assert!((-27002..=-26998).contains(&z), "{z}");
        let roach = roach_flying("POS = { -10000, -1800, -27100 }");
        roach.run("w.timers[0] = 0x5A; tick()");
        assert_eq!(roach.eval::<[u8; 4]>(ROUTINES), [1, 0, 0, 0]);
        let (hidden, taken, flags, z): (i64, i64, i64, i64) =
            roach.eval("return w.hidden, w.swarm[2], w.vars[3], w.pos[3]");
        assert_eq!((hidden, taken, flags & 3, z), (0xC, 0x20, 0, -27000));
    }

    /// Within 200 of the point it lands: attached, counted, and on the
    /// player's routines (part 2 ports the rest).
    #[test]
    fn a_roach_lands_on_its_attach_point() {
        let roach =
            roach_flying("POS = { -10000, -1000, -25200 }; PART_AT = { -10050, -900, -25150 }");
        roach.run("w.be = 0x2001; tick()");
        assert_eq!(roach.eval::<[u8; 4]>(ROUTINES), [1, 3, 0, 0]);
        let (flags, on, be): (i64, i64, i64) = roach.eval("return w.vars[3], w.swarm[1], w.be");
        assert_eq!((flags, on, be), (3, 1, 1));
    }
}
