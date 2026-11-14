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
/// `e:blow_part`'s arguments: the part, the direction, then the speed, the
/// rise, the gravity, the colour and the slowing, each optional.
type BlowPartArgs = (
    usize,
    i32,
    Option<i16>,
    Option<i16>,
    Option<i16>,
    Option<u32>,
    Option<i16>,
);

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
        // `pDamage_work`'s `+0x99` and `+0x9a` when the last hit was a bolt.
        methods.add_method_mut("bolt", |_, e, ()| {
            let z = &e.em().z;
            Ok((z.bolt_part, z.bolt_yaw))
        });
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
        // `pSuper` set to the player's part `part`'s `Workm` (nil: back to
        // the body's own `Super_matrix`).
        methods.add_method_mut("set_super_part", |_, e, part: Option<u8>| {
            e.em().work().super_part = part;
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
        // objects: its return and `DWORD_00695e74`, the lowest top that rises
        // past the point (0 when none does).
        methods.add_method_mut(
            "sca_ck_hit",
            |_, e, (x, y, z, radius, mask): (i32, i32, i32, i32, i64)| {
                let room = e.em().mover.tick.room;
                Ok(room.sca.as_ref().map_or((0, 0), |sca| {
                    let ground = crate::sca::ground_at(sca, [x, y, z], radius, mask as u16);
                    (ground.height, ground.ceiling)
                }))
            },
        );
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

        // `Em_2D`'s wait before the next wake groan, `BYTE_00689610`.
        methods.add_method_mut("groan_wait", |_, e, ()| Ok(*e.em().world.em2d_groan));
        methods.add_method_mut("set_groan_wait", |_, e, value: i64| {
            *e.em().world.em2d_groan = value as u8;
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
            // The enemies that write the life as -1 do it with `fg_system`'s
            // `0x4000000`.
            if life < 0 {
                e.em().world.pl.die_start = true;
            }
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
        methods.add_method_mut("hard_mode", |_, e, ()| Ok(e.em().world.difficulty.hard));
        methods.add_method_mut("censorship_off", |_, e, ()| {
            Ok(e.em().world.difficulty.uncensored)
        });
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
        methods.add_method_mut("burn_parts", |_, e, ()| {
            crate::zombie2::burn_parts(&mut e.em().work().part_rgb);
            Ok(())
        });
        methods.add_method_mut("char_parts", |_, e, ()| {
            crate::zombie2::char_parts(&mut e.em().work().part_rgb);
            Ok(())
        });
        methods.add_method_mut("reset_part_colours", |_, e, ()| {
            e.em().work().part_rgb = [player2::DEFAULT_PART_RGB; 32];
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
        methods.add_method_mut("set_lean", |_, e, (lean, yaw): (i64, Option<i64>)| {
            e.em().work().lean = lean as i16;
            e.em().work().lean_yaw = yaw.unwrap_or(0) as i16;
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
        // `FUN_004e0070`: every part's place blended by the weights of the
        // morph of `part`, as `morph` does for its children.
        methods.add_method_mut(
            "morph_all",
            |_, e, (part, weights): (u8, mlua::Variadic<i16>)| {
                let em = e.em();
                let Some(morph) = em.mover.tick.morphs.iter().find(|morph| morph.part == part)
                else {
                    return Ok(());
                };
                let joints = morph.all_joint_offsets(&weights);
                let offsets = &mut em.work().joint_offsets;
                offsets.retain(|(index, _)| joints.iter().all(|(child, _)| child != index));
                offsets.extend(joints);
                Ok(())
            },
        );
        // A part's morph weights as the model's data has them.
        methods.add_method_mut("morph_weights", |_, e, part: u8| {
            let em = e.em();
            let weights = em
                .mover
                .tick
                .morphs
                .iter()
                .find(|morph| morph.part == part)
                .map_or_else(Vec::new, |morph| morph.weights.clone());
            Ok(mlua::Variadic::from_iter(weights))
        });
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
            |_, e, (part, dir, speed, up, gravity, rgb, slowing): BlowPartArgs| {
                e.em().blow_part(
                    part,
                    dir as u16,
                    speed.unwrap_or(800),
                    (up.unwrap_or(0), gravity.unwrap_or(10)),
                    rgb,
                    slowing.unwrap_or(100),
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
        methods.add_method_mut("key_trg", |_, e, ()| Ok(e.em().world.key_trg));
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
        // `Guide_check` at a point: the last guide area holding it, or 0xff.
        methods.add_method_mut("guide_at", |_, e, (x, z): (i32, i32)| {
            let guides = e.em().world.guides;
            Ok(super::guide_check(guides, x as i16, z as i16).unwrap_or(0xFF))
        });
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
            let pl = &e.em().world.pl;
            Ok(pl.at_em_flg != 0 || pl.at_em_partner)
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
            |_, e, (x, y, z, angle, vars, run): (i32, i32, i32, i64, mlua::Table, Option<bool>)| {
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
                spawn.run_now = run.unwrap_or(true);
                Ok(())
            },
        );
        // `*(undefined4 *)enemy_work[slot] = 0`: take a work out of play, so
        // `Set_enemy_work` finds it dead and gives it back.
        methods.add_method_mut("free_work", |_, e, slot: usize| {
            let world = &mut *e.em().world;
            if let Some(entry) = world.works.iter_mut().find(|entry| entry.0 == slot)
                && entry.2
            {
                entry.2 = false;
                world.freed.push(slot);
            }
            Ok(())
        });
        // `ApplyMatrixSV` of a point on the parent's part `part`, as drawn.
        methods.add_method_mut(
            "parent_apply",
            |_, e, (part, x, y, z): (usize, i16, i16, i16)| {
                let matrix = parent(e)?.parts.get(part).copied().flatten();
                let Some(mut matrix) = matrix else {
                    return Ok((0, 0, 0));
                };
                matrix.t = [0; 3];
                let [x, y, z] = crate::esp2::apply_matrix(&matrix, [x, y, z]);
                Ok((x, y, z))
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
        methods.add_method_mut("l_spl", |_, e, ()| Ok(e.em().world.l_spl));
        methods.add_method_mut("dir_spl", |_, e, ()| Ok(e.em().world.dir_spl));
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
        // `Esp_call` on the player's part `part` as the last draw left it
        // (`Work::parts`) at the point (x, y, z) on it: the effect keeps the
        // part, as the original's keeps a pointer to its `Workm`. Nothing
        // without a pose of the part.
        methods.add_method_mut(
            "player_part_effect",
            |_,
             e,
             (kind, row, size, dir, part, x, y, z): (u8, u8, u16, i16, usize, i32, i32, i32)| {
                let world = &mut *e.em().world;
                let Some(matrix) = world.pl.parts.get(part).copied().flatten() else {
                    return Ok(());
                };
                let anchor = crate::esp2::Anchor {
                    body: crate::esp2::Body::Player,
                    part: part as u8,
                };
                let point = [x as i16, y as i16, z as i16];
                world
                    .effects
                    .call_on_part(kind, row, size, dir, anchor, matrix, point);
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

    const SCRIPTS: [&str; 14] = [
        include_str!("../../games/re2/enemy/20/enemy.lua"),
        include_str!("../../games/re2/enemy/21/enemy.lua"),
        include_str!("../../games/re2/enemy/23/enemy.lua"),
        include_str!("../../games/re2/enemy/25/enemy.lua"),
        include_str!("../../games/re2/enemy/26/enemy.lua"),
        include_str!("../../games/re2/enemy/27/enemy.lua"),
        include_str!("../../games/re2/enemy/28/enemy.lua"),
        include_str!("../../games/re2/enemy/29/enemy.lua"),
        include_str!("../../games/re2/enemy/2a/enemy.lua"),
        include_str!("../../games/re2/enemy/2d/enemy.lua"),
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

    /// A stand-in for the licker's work, enough for `lib/licker.lua`'s init,
    /// the scripted drop (`Routine_1` 0x10) and `listen` (0x0E); any other
    /// call does nothing. A clip ends on its `CLIP_LEN`th step, `Rnd` gives
    /// `RND`, `Direction_ck` gives `DIR`, `Super_vector` doesn't turn, and
    /// `SEES` has the player touch it. `tick()` runs one `update`.
    const LICKER_STUB: &str = "
        w = { r = { [0] = 0, 0, 0, 0 }, vars = {}, be = 0, kind = KIND,
            pos = { 32000, -5700, 32000 }, spd = { 0, 0, 0 }, angle = 1024,
            move_no = -1, move_cnt = 0, frame = 0, flags = 0, sce = 0, life = 0,
            supers = {}, init_supers = 0, spxz = 0, clips = {}, atd = 0 }
        e = setmetatable({}, { __index = function() return function() end end })
        function e:routine(i) return w.r[i] end
        function e:set_routine(i, v) w.r[i] = v end
        function e:routines(a, b, c, d) w.r = { [0] = a, b, c, d } end
        function e:go(one) w.r = { [0] = 1, one, 0, 0 } end
        function e:step(ended) if ended then w.r[2] = w.r[2] + 1 end end
        function e:var(i) return w.vars[i] or 0 end
        function e:set_var(i, v) w.vars[i] = v end
        function e:be_flg() return w.be end
        function e:set_be_flg(v) w.be = v end
        function e:kind() return w.kind end
        function e:set_kind(v) w.kind = v end
        function e:flags() return w.flags end
        function e:set_flags(v) w.flags = v end
        function e:sce_free() return w.sce end
        function e:set_sce_free(_, v) w.sce = v end
        function e:easy() return false end
        function e:common_a4() return false end
        function e:rnd() return RND end
        function e:set_life(v) w.life = v end
        function e:move_no() return w.move_no end
        function e:set_move_no(v) w.move_no = v end
        function e:set_move_cnt(v) w.move_cnt = v end
        function e:clip(n, c) w.move_no = n; w.move_cnt = c; w.frame = 0; w.clips[#w.clips + 1] = n end
        function e:joint()
            w.frame = w.frame + 1
            w.move_cnt = w.move_cnt + 1
            return w.frame >= CLIP_LEN
        end
        function e:add_speed_xz() w.spxz = w.spxz + 1 end
        function e:speed() return w.spd[1], w.spd[2], w.spd[3] end
        function e:set_speed(x, y, z) w.spd = { x, y, z } end
        function e:pos() return w.pos[1], w.pos[2], w.pos[3] end
        function e:add_pos(x, y, z) w.pos = { w.pos[1] + x, w.pos[2] + y, w.pos[3] + z } end
        function e:set_pos_y(y) w.pos[2] = y end
        function e:angle() return w.angle end
        function e:set_angle(v) w.angle = v end
        function e:super_add(x, y, z) w.supers[#w.supers + 1] = { x, y, z } end
        function e:super_rotate(_, x, y, z) return x, y, z end
        function e:direction() return DIR end
        function e:init_super() w.init_supers = w.init_supers + 1 end
        function e:set_atd() w.atd = w.atd + 1 end
        function e:weapon() return 2 end
        function e:player_life() return 200 end
        function e:touching() return SEES end
        function e:noise() return 0 end
        function e:distance() return 20000 end
        function e:pl_routine() return 0 end
        function tick() LICKER.run(e, VARIANT) end
    ";

    /// Runs `body` against [`LICKER_STUB`] with room 204's scene licker
    /// (`Type` 0x4510, `KIND`), after `setup`.
    fn run_licker(setup: &str, body: &str) {
        let (lua, budget) = crate::lua::new_sandboxed_state().unwrap();
        budget.begin();
        let licker: mlua::Table = lua
            .load(include_str!("../../games/re2/lib/licker.lua"))
            .eval()
            .unwrap();
        lua.globals().set("LICKER", licker).unwrap();
        let script = include_str!("../../games/re2/enemy/22/enemy.lua");
        lua.load(format!(
            "local function require() return {{ run = function(e, v) VARIANT = v end }} end
             {script}
             update(nil)
             KIND = 0x4510 RND = 2 CLIP_LEN = 1000 DIR = 0x350 SEES = false
             {setup}
             {LICKER_STUB}"
        ))
        .exec()
        .unwrap();
        budget.begin();
        lua.load(body).exec().unwrap();
    }

    /// The slash's hit (`switchD_004651fd::caseD_9`, 0x463db0): its blood
    /// is part 13's `Workm.t` as the last draw left it, 200 lower, with no
    /// turn of the part's (#648).
    #[test]
    fn the_slash_bleeds_200_below_part_13() {
        run_licker(
            "",
            "tick()
             w.r = { [0] = 1, 9, 2, 1 }
             function e:own_part(p) if p == 13 then return -16387, -134, -8989 end end
             function e:part_point() return 0, 0, 0 end
             function e:call_effect(...) w.fx = { ... } end
             function e:pl_damage_cnt() return 0 end
             function e:turn_to_player() return 0 end
             function e:distance() return 0x800 end
             function e:pl_life_down() return 0 end
             tick()
             assert(w.fx, 'no blood')
             local fx = w.fx
             assert(fx[1] == 0 and fx[2] == 0 and fx[3] == 0x2400 and fx[4] == 0)
             assert(fx[5] == -16387 and fx[6] == 66 and fx[7] == -8989,
                 fx[5] .. ' ' .. fx[6] .. ' ' .. fx[7])",
        );
    }

    /// `Em22_Init` (0x464B50): a scripted `Type` takes `BYTE_00527c80`, so
    /// 0x4510 starts in `Routine_1` 0x10, turned onto the ceiling (surface 5),
    /// with its life drawn from `Em22_Life`.
    #[test]
    fn a_scripted_ceiling_licker_starts_in_0x10() {
        run_licker(
            "",
            "tick()
             assert(w.r[0] == 1 and w.r[1] == 0x10 and w.r[2] == 0 and w.r[3] == 0)
             assert(#w.supers == 1 and w.supers[1][1] == 0x800 and w.supers[1][2] == 0x800
                 and w.supers[1][3] == 0)
             assert(w.life == VARIANT.life[2], w.life)",
        );
    }

    /// 0x10's `Routine_2` 0 (0x466B00): clip 3 by `Mot_play2_spxz` on the
    /// ceiling until `Sce_free0`, with the script's bit kept.
    #[test]
    fn it_waits_on_the_ceiling_until_sce_free0() {
        run_licker(
            "",
            "for _ = 1, 21 do tick() end
             assert(w.r[1] == 0x10 and w.r[2] == 0)
             assert(w.move_no == 3 and w.spxz == 20)
             assert(w.kind == 0x4510)
             assert(w.pos[2] == -5700)",
        );
    }

    /// The first `member_set(23, 1)` lets it go: `Type` loses 0x4000,
    /// `Sce_free0` is cleared and `Spd_y` starts at 0; the next tick it falls
    /// by 0x48 on clip 0x12 (0x466B45).
    #[test]
    fn the_first_word_lets_it_go() {
        run_licker(
            "",
            "tick() tick()
             w.sce = 1
             tick()
             assert(w.r[2] == 1 and w.kind == 0x0510 and w.sce == 0 and w.spd[2] == 0)
             tick()
             assert(w.spd[2] == 0x48 and w.pos[2] == -5700 + 0x48 and w.move_no == 0x12)",
        );
    }

    /// Clip 0x12's end moves it to `Routine_2` 2 (0x466B67): up by 0xA8C,
    /// turned to `(Direction_ck + 0x100) & 0xC00`, `Em_init_super`, clip
    /// 0x13, and on through state 3's fall to the landing (0x466BE3), all in
    /// one tick: from -3108 after eight falls to 240 and state 4.
    #[test]
    fn it_turns_over_and_lands_in_one_tick() {
        run_licker(
            "CLIP_LEN = 8",
            "tick() w.sce = 1 tick()
             for _ = 1, 8 do tick() end
             assert(w.r[2] == 2 and w.pos[2] == -3108, w.pos[2])
             local supers = w.init_supers
             tick()
             assert(w.pos[2] == 240, w.pos[2])
             assert(w.angle == (0x350 + 0x100) & 0xC00)
             assert(w.init_supers == supers + 1)
             assert(w.clips[#w.clips] == 0x13)
             assert(w.vars[5] == 3 and w.be & 8 == 0 and w.move_no == 0 and w.r[2] == 4)",
        );
    }

    /// State 4 (0x466C55) holds it on the floor through clip 0x13; state 5
    /// (0x466C75) stands up on clip 3 with `FLAG_UP` and gun area 4, and
    /// waits there for the second word.
    #[test]
    fn it_stands_at_0_until_clip_0x13_ends_then_waits_for_the_second_word() {
        run_licker(
            "CLIP_LEN = 8",
            "tick() w.sce = 1 tick()
             for _ = 1, 9 do tick() end
             assert(w.r[2] == 4)
             for _ = 1, 7 do tick() assert(w.pos[2] == 0 and w.r[2] == 4) end
             tick()
             assert(w.r[2] == 5)
             for _ = 1, 10 do tick() end
             assert(w.r[1] == 0x10 and w.r[2] == 5)
             assert(w.flags & 2 ~= 0 and w.vars[5] == 4)",
        );
    }

    /// The second word, in state 5, writes 0x0E01 over the routines and
    /// leaves `Sce_free0` set.
    #[test]
    fn the_second_word_sends_it_to_listen_and_keeps_sce_free0() {
        run_licker(
            "CLIP_LEN = 8",
            "tick() w.sce = 1 tick()
             for _ = 1, 18 do tick() end
             assert(w.r[2] == 5)
             w.sce = 1
             tick()
             assert(w.r[0] == 1 and w.r[1] == 0xE and w.r[2] == 0 and w.r[3] == 0)
             assert(w.sce == 1)",
        );
    }

    /// `Routine_1` 0x0E (0x464130) counts `Routine_3` every tick and, past
    /// 10, hands over to 0x0F when it spots the player; if it doesn't, the
    /// second clip's end goes to 4, and the shared count (0x4641BF) still
    /// runs after it.
    #[test]
    fn listen_hands_over_to_decide_after_eleven_ticks_when_it_spots() {
        let listen = "w.r = { [0] = 1, 0xE, 0, 0 }";
        run_licker(
            "SEES = true",
            &format!(
                "tick() {listen}
                 for i = 1, 10 do tick() assert(w.r[1] == 0xE and w.r[3] == i) end
                 tick()
                 assert(w.r[1] == 0xF and w.r[2] == 0 and w.r[3] == 0)"
            ),
        );
        run_licker(
            "CLIP_LEN = 3",
            &format!(
                "tick() {listen}
                 for _ = 1, 5 do tick() end
                 assert(w.r[1] == 0xE and w.r[2] == 2)
                 tick()
                 assert(w.r[1] == 4 and w.r[2] == 0 and w.r[3] == 1)"
            ),
        );
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

    /// `FUN_00454650`, a crawling zombie lying flat (routine 1, 2): its first
    /// tick writes `Move_no` 0x17 as a word, so `Move_cnt` and `Hokan` go to
    /// 0 with it (`leon_a_200_211`, room 20B's slot 3 on 6288).
    #[test]
    fn a_flat_crawlers_first_tick_starts_its_clip_from_0() {
        let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
        let library = include_str!("../../games/re2/lib/zombie.lua");
        lua.load(format!(
            "local w = {{ r = {{ [0] = 1, 2, 0, 0 }}, clip = {{ 0x17, 1, 3 }} }}
             local e = setmetatable({{}}, {{ __index = function() return function() return 0 end end }})
             function e:routine(i) return w.r[i] end
             function e:set_routine(i, v) w.r[i] = v end
             function e:routines(a, b, c, d) w.r = {{ [0] = a, b, c, d }} end
             function e:kind() return 0x2001 end
             function e:in_sight() return false end
             function e:clip(no, count, hokan) w.clip = {{ no, count, hokan }} end
             function e:set_move_no(no) w.clip[1] = no end
             local zombie = (function() {library} end)()
             zombie.run(e, {{}})
             CLIP = w.clip
             STEP = w.r[2]"
        ))
        .exec()
        .unwrap();
        let clip: Vec<i64> = lua.globals().get("CLIP").unwrap();
        let step: i64 = lua.globals().get("STEP").unwrap();
        assert_eq!((clip, step), (vec![0x17, 0, 0], 1));
    }

    /// `Em10Dm01`'s blood on its even ticks: on part 0, with the zombie's
    /// speed (`&Spd_x`) as its vector, after a staggered zombie's drop of 500.
    #[test]
    fn a_knocked_back_zombies_blood_is_on_its_speed() {
        let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
        let library = include_str!("../../games/re2/lib/zombie.lua");
        lua.load(format!(
            "local w = {{ r = {{ [0] = 2, 2, 1, 0 }}, vars = {{ [1] = 5, [3] = 1, [4] = 1 }},
                 timers = {{ [1] = 2, [2] = 1 }}, speed = {{ -450, 30, 7 }} }}
             EFFECTS = {{}}
             local e = setmetatable({{}}, {{ __index = function() return function() return 0 end end }})
             function e:routine(i) return w.r[i] end
             function e:set_routine(i, v) w.r[i] = v end
             function e:routines(a, b, c, d) w.r = {{ [0] = a, b, c, d }} end
             function e:var(i) return w.vars[i] or 0 end
             function e:set_var(i, v) w.vars[i] = v end
             function e:timer(i) return w.timers[i] or 0 end
             function e:set_timer(i, v) w.timers[i] = v end
             function e:life() return 100 end
             function e:speed() return w.speed[1], w.speed[2], w.speed[3] end
             function e:set_speed(x, y, z) w.speed = {{ x, y, z }} end
             function e:effect(kind, row, size, dir, part, x, y, z)
                 EFFECTS[#EFFECTS + 1] = {{ kind, size, part, x, y, z }}
             end
             local zombie = (function() {library} end)()
             zombie.run(e, {{}})"
        ))
        .exec()
        .unwrap();
        let effects: Vec<Vec<i64>> = lua.globals().get("EFFECTS").unwrap();
        assert_eq!(effects, [vec![0, 0x13E8, 0, -450, -470, 7]]);
    }

    /// `FUN_00453e40`'s step 6, a zombie on the floor starting to rise with
    /// `Timer3` at `timer3`: `(Be_flg & 0x200 after, the effects as (kind,
    /// size, part, x, y, z))`.
    fn rising_start(timer3: i64) -> (bool, Vec<Vec<i64>>) {
        let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
        let library = include_str!("../../games/re2/lib/zombie.lua");
        lua.globals().set("TIMER3", timer3).unwrap();
        lua.load(format!(
            "local w = {{ r = {{ [0] = 1, 5, 6, 0 }}, vars = {{ [1] = 5 }}, flags = 0x212,
                 timers = {{ [3] = TIMER3 }} }}
             EFFECTS = {{}}
             local e = setmetatable({{}}, {{ __index = function() return function() return 0 end end }})
             function e:routine(i) return w.r[i] end
             function e:set_routine(i, v) w.r[i] = v end
             function e:var(i) return w.vars[i] or 0 end
             function e:set_var(i, v) w.vars[i] = v end
             function e:timer(i) return w.timers[i] or 0 end
             function e:set_timer(i, v) w.timers[i] = v end
             function e:flags() return w.flags end
             function e:set_flags(v) w.flags = v end
             function e:body_box() return 0, 0, 0, 0 end
             function e:joint_bank() return false end
             function e:effect(kind, row, size, dir, part, x, y, z)
                 EFFECTS[#EFFECTS + 1] = {{ kind, size, part, x, y, z }}
             end
             local zombie = (function() {library} end)()
             zombie.run(e, {{}})
             STILL = w.flags & 0x200 ~= 0"
        ))
        .exec()
        .unwrap();
        (
            lua.globals().get("STILL").unwrap(),
            lua.globals().get("EFFECTS").unwrap(),
        )
    }

    /// It clears `+0x21b`'s 0x02 (`FLAG_NECK_STILL`), so its head follows the
    /// player again as it gets up, and with `Timer3` 1 it bleeds once more:
    /// `Esp_call(8000, Cdir_y, part 0's Workm, 0)`.
    #[test]
    fn a_rising_zombies_head_follows_again_and_a_last_blood_starts() {
        assert_eq!(rising_start(1), (false, vec![vec![0, 8000, 0, 0, 0, 0]]));
        assert_eq!(rising_start(2), (false, vec![]));
    }

    /// `FUN_00457e60`, a hit while it gets up: the first tick falls from step
    /// 0 into step 1 with the blood's `(0, 300, 0)` still in `local_48`, so
    /// its lean turns 300 about y as well; on the next it is the lean alone,
    /// `Timer3 * Timer1 * Timer0 * -8`.
    #[test]
    fn a_hit_while_rising_first_leans_by_the_bloods_vector() {
        let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
        let library = include_str!("../../games/re2/lib/zombie.lua");
        lua.load(format!(
            "local w = {{ r = {{ [0] = 2, 2, 0, 0 }}, vars = {{}}, flags = 0x10, timers = {{}} }}
             LEANS = {{}}
             local e = setmetatable({{}}, {{ __index = function() return function() return 0 end end }})
             function e:routine(i) return w.r[i] end
             function e:set_routine(i, v) w.r[i] = v end
             function e:var(i) return w.vars[i] or 0 end
             function e:set_var(i, v) w.vars[i] = v end
             function e:timer(i) return w.timers[i] or 0 end
             function e:set_timer(i, v) w.timers[i] = v end
             function e:flags() return w.flags end
             function e:set_flags(v) w.flags = v end
             function e:joint_bank() return false end
             function e:set_lean(lean, yaw) LEANS[#LEANS + 1] = {{ lean, yaw or 0 }} end
             local zombie = (function() {library} end)()
             zombie.run(e, {{}})
             zombie.run(e, {{}})"
        ))
        .exec()
        .unwrap();
        let leans: Vec<Vec<i64>> = lua.globals().get("LEANS").unwrap();
        assert_eq!(leans, [vec![0, 300], vec![-0x18 * 8, 0]]);
    }

    /// A handgun hit (`Routine_1` 2) on a standing zombie with life 100 and
    /// `At_sce_no` 1, its moan still waiting: `(At_obj_no, At_sce_no,
    /// Routine_2, the clip, the draws)` after the hit's first tick.
    fn handgun_hit(kind: u16) -> (i64, i64, i64, i64, i64) {
        weapon_hit(kind, 2, 1)
    }

    /// [`handgun_hit`] with another weapon (`Routine_1`) and `At_sce_no`.
    fn weapon_hit(kind: u16, weapon: u8, at_sce_no: u8) -> (i64, i64, i64, i64, i64) {
        let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
        let library = include_str!("../../games/re2/lib/zombie.lua");
        lua.globals().set("KIND", kind).unwrap();
        lua.globals().set("WEAPON", weapon).unwrap();
        lua.globals().set("AT_SCE", at_sce_no).unwrap();
        lua.load(format!(
            "local w = {{ r = {{ [0] = 2, WEAPON, 0, 0 }}, vars = {{ [1] = 5, [3] = 0, [4] = AT_SCE }},
                 clip = -1, drawn = 0 }}
             local e = setmetatable({{}}, {{ __index = function() return function() return 0 end end }})
             function e:routine(i) return w.r[i] end
             function e:set_routine(i, v) w.r[i] = v end
             function e:routines(a, b, c, d) w.r = {{ [0] = a, b, c, d }} end
             function e:var(i) return w.vars[i] or 0 end
             function e:set_var(i, v) w.vars[i] = v end
             function e:kind() return KIND end
             function e:life() return 100 end
             function e:move_no() return 9 end
             function e:clip(n) w.clip = n end
             function e:speed() return 0, 0, 0 end
             function e:rnd() w.drawn = w.drawn + 1 return 7 end
             local zombie = (function() {library} end)()
             zombie.run(e, {{}})
             R = {{ w.vars[3], w.vars[4], w.r[2], w.clip, w.drawn }}"
        ))
        .exec()
        .unwrap();
        let r: Vec<i64> = lua.globals().get("R").unwrap();
        (r[0], r[1], r[2], r[3], r[4])
    }

    /// `Em10Dm00` (0x455BB0): when a hit takes `At_sce_no` below 1 on a
    /// zombie whose `Type` has no 0x40 and whose life is over 0x50, it draws
    /// `At_sce_no` again, `(Rnd & 0xF) + 0x10`. Room 207's four are such
    /// zombies. With 0x40, `Em10_Damage` (0x455780) compares `At_sce_no`
    /// against 0x17 instead of 0: 1 is under it, so it marks the zombie
    /// staggered (`At_obj_no` 1) and `Em10Dm00` hands the hit to `Em10Dm01`,
    /// the knock-back (clip 4 from the front), with no draw.
    /// What a grab did, tick by tick ([`grab_run`]).
    struct Grab {
        /// The zombie's `Routine_2` after each tick.
        steps: Vec<u8>,
        /// `Timer0` after each tick.
        timers: Vec<i32>,
        /// Each `Pl_life_down`: (tick, damage, low).
        bites: Vec<(usize, i64, bool)>,
        /// The player's routines as the grab's first tick set them.
        player: Vec<i64>,
        /// The player's `Routine_2` writes: (tick, value).
        player_steps: Vec<(usize, i64)>,
        /// The zombie's routines at the end.
        routines: Vec<u8>,
    }

    /// `FUN_00453510` from its first tick (`1,3,0,0`), or `1,1,0,0` for a
    /// crawler, for `ticks` ticks. `setup` runs first: `KIND`, `V` (the id's
    /// variant), `BEHIND` (`Cdir_ck`), `PRESS(t)` (`Gacha_ch` on tick `t`,
    /// from 1), `LIFE_DOWN` (what `Pl_life_down` returns) and `STEP`/`TIMER0`/
    /// `COUNT` to start part way. The grab's clip ends on its fifth frame and
    /// the struggle's loops in 38, as the motion banks have them.
    fn grab_run(setup: &str, ticks: usize) -> Grab {
        let (lua, budget) = crate::lua::new_sandboxed_state().unwrap();
        budget.begin();
        let library = include_str!("../../games/re2/lib/zombie.lua");
        lua.load(format!(
            "KIND, V, BEHIND, LIFE_DOWN = 6, {{}}, false, 0
             STEP, TIMER0, COUNT = 0, 0, 0
             function PRESS(t) return false end
             {setup}
             local LENGTHS = {{ [0xB] = 5, [0xC] = 38, [0xD] = 8, [0xE] = 10, [0xF] = 20, [0x10] = 40 }}
             local w = {{ r = {{ [0] = 1, KIND & 1 ~= 0 and 1 or 3, STEP, 0 }}, timers = {{ [0] = TIMER0 }},
                 vars = {{ [1] = 5 }}, clip = 0xC, count = COUNT, tick = 0, pl_dc = 0 }}
             STEPS, TIMERS, BITES, PLAYER, PLAYER_STEPS = {{}}, {{}}, {{}}, {{}}, {{}}
             local e = setmetatable({{}}, {{ __index = function() return function() return 0 end end }})
             function e:routine(i) return w.r[i] end
             function e:set_routine(i, v) w.r[i] = v end
             function e:routines(a, b, c, d) w.r = {{ [0] = a, b, c, d }} end
             function e:kind() return KIND end
             function e:var(i) return w.vars[i] or 0 end
             function e:set_var(i, v) w.vars[i] = v end
             function e:timer(i) return w.timers[i] or 0 end
             function e:set_timer(i, v) w.timers[i] = v end
             function e:clip(n) w.clip = n; w.count = 0 end
             function e:move_no() return w.clip end
             function e:move_cnt() return w.count end
             function e:joint_bank()
                 w.count = w.count + 1
                 if w.count >= (LENGTHS[w.clip] or 30) then
                     w.count = 0
                     return true
                 end
                 return false
             end
             function e:step(ended) if ended then w.r[2] = w.r[2] + 1 end end
             function e:speed() return 0, 0, 0 end
             function e:cdir_player() return BEHIND end
             function e:gacha() return PRESS(w.tick) end
             function e:pl_damage_cnt() return w.pl_dc end
             function e:pl_set_damage_cnt(v) w.pl_dc = v end
             function e:pl_set_routine(a, b, c, d) PLAYER = {{ a, b, c, d }} end
             function e:pl_set_routine_at(i, v) table.insert(PLAYER_STEPS, {{ w.tick, v }}) end
             function e:pl_life_down(damage, low)
                 table.insert(BITES, {{ w.tick, damage, low }})
                 return LIFE_DOWN
             end
             function e:at_em_no() return 0xFF end
             local zombie = (function() {library} end)()
             function TICK(t)
                 w.tick = t
                 zombie.run(e, V)
                 STEPS[t] = w.r[2]
                 TIMERS[t] = w.timers[0] or 0
                 ROUTINES = {{ w.r[0], w.r[1], w.r[2], w.r[3] }}
             end"
        ))
        .exec()
        .unwrap();
        let globals = lua.globals();
        let tick: mlua::Function = globals.get("TICK").unwrap();
        for t in 1..=ticks {
            budget.begin();
            tick.call::<()>(t).unwrap();
        }
        let pairs = |name: &str| -> Vec<(usize, i64)> {
            let rows: Vec<Vec<i64>> = globals.get(name).unwrap();
            rows.iter().map(|row| (row[0] as usize, row[1])).collect()
        };
        let bites: Vec<mlua::Table> = globals.get("BITES").unwrap();
        Grab {
            steps: globals.get("STEPS").unwrap(),
            timers: globals.get("TIMERS").unwrap(),
            bites: bites
                .iter()
                .map(|row| {
                    (
                        row.get::<usize>(1).unwrap(),
                        row.get(2).unwrap(),
                        row.get(3).unwrap(),
                    )
                })
                .collect(),
            player: globals.get("PLAYER").unwrap(),
            player_steps: pairs("PLAYER_STEPS"),
            routines: globals.get("ROUTINES").unwrap(),
        }
    }

    /// The tick of the grab on which the struggle let go (`Routine_2` 4).
    fn released(grab: &Grab) -> usize {
        grab.steps.iter().position(|&step| step == 4).unwrap() + 1
    }

    /// `FUN_00453510`'s struggle: `Timer0` starts at 0x94 on the clip's
    /// first tick and loses 2 a tick, 7 on a tick `Gacha_ch` counts a fresh
    /// press, and the zombie lets go (`Routine_2` 4, the player's too) on the
    /// tick it goes below 0. The grab's clip is the first five ticks, so the
    /// struggle's first tick is the sixth: 75 struggle ticks without a press,
    /// 22 with one on every tick, 33 or 34 with one every other tick.
    #[test]
    fn the_struggle_lets_go_by_the_presses() {
        let none = grab_run("", 120);
        assert_eq!(none.steps[..6], [1, 1, 1, 1, 2, 3]);
        assert_eq!(none.timers[5], 0x94 - 2);
        assert_eq!(released(&none), 5 + 75);
        assert_eq!(none.player_steps, [(80, 4)]);
        let mashed = grab_run("function PRESS(t) return t > 5 end", 60);
        assert_eq!(released(&mashed), 5 + 22);
        // Struggle tick 21 leaves 148 - 7 * 21 = 1.
        assert_eq!(mashed.timers[5 + 20], 1);
        let odd = grab_run("function PRESS(t) return (t - 5) % 2 == 1 end", 60);
        assert_eq!(released(&odd), 5 + 33);
        let even = grab_run(
            "function PRESS(t) return t > 5 and (t - 5) % 2 == 0 end",
            60,
        );
        assert_eq!(released(&even), 5 + 34);
    }

    /// The bite's frame and damage go by `Type & 1` and the tough ids (0x11,
    /// 0x17) alone: a standing 0x13 or 0x1F bites for 0x14 on the struggle's
    /// frame 0x10 (struggle ticks 16 and 54), a tough one for 0x1E, and a
    /// crawler for 5 (10 tough) on frame 1, with `Pl_life_down`'s mode 1.
    /// `Model_type` and the skin play no part.
    #[test]
    fn the_bites_go_by_crawling_and_toughness() {
        for variant in ["{ parts0_y = 0xE66 }", "{ skins = true }"] {
            let grab = grab_run(&format!("V = {variant}"), 80);
            assert_eq!(grab.bites, [(5 + 16, 0x14, false), (5 + 54, 0x14, false)]);
        }
        let tough = grab_run("V = { tough = true }", 30);
        assert_eq!(tough.bites, [(5 + 16, 0x1E, false)]);
        // A crawler's grab clip is ten frames, and its struggle (0xF) bites
        // on frame 1.
        let low = grab_run("KIND = 1", 12);
        assert_eq!(low.bites, [(11, 5, true)]);
        let low = grab_run("KIND = 1; V = { tough = true }", 12);
        assert_eq!(low.bites, [(11, 10, true)]);
    }

    /// The counter's test comes before the bite's on the same tick
    /// (`FUN_00453510` l.88 and l.92): a struggle that runs out on the bite's
    /// frame lets go and still bites.
    #[test]
    fn the_counter_comes_before_the_bite() {
        let grab = grab_run("STEP, TIMER0, COUNT = 3, 1, 0xF", 1);
        assert_eq!(grab.steps, [4]);
        assert_eq!(grab.bites, [(1, 0x14, false)]);
        assert_eq!(grab.player_steps, [(1, 4)]);
    }

    /// `Pl_life_down`'s results: 1 (the player at nothing) lets go at once,
    /// with one more blood (the player's `Timer2`); 2 kills, and the zombie
    /// drops the body (`1,6`) as the player dies in its hands (`5` to `6`,
    /// facing as the grab did).
    #[test]
    fn a_last_bite_lets_go_and_a_lethal_one_kills() {
        let last = grab_run("LIFE_DOWN = 1", 21);
        assert_eq!(released(&last), 21);
        assert_eq!(last.player_steps, [(21, 4)]);
        let lethal = grab_run("LIFE_DOWN, BEHIND = 2, true", 21);
        assert_eq!(lethal.routines, [1, 6, 0, 0]);
        assert_eq!(lethal.player, [6, 1, 0, 0]);
    }

    /// The grab's first tick holds the player (`5`, `Cdir_ck` in
    /// `Routine_1`, plus 2 for a crawler's low hold) and raises the player's
    /// `Damage_cnt` 0x80, which keeps every other zombie's reach off.
    #[test]
    fn a_grab_holds_the_player_front_or_back() {
        assert_eq!(grab_run("", 1).player, [5, 0, 0, 0]);
        assert_eq!(grab_run("BEHIND = true", 1).player, [5, 1, 0, 0]);
        assert_eq!(grab_run("KIND = 1", 1).player, [5, 2, 0, 0]);
        assert_eq!(grab_run("KIND, BEHIND = 1, true", 1).player, [5, 3, 0, 0]);
    }

    /// `FUN_00452930`'s reach: a walking zombie within 0x4B0 that faces the
    /// player grabs (`1,3`) only while the player's `Damage_cnt` has no 0x80,
    /// so a held player is never grabbed by a second zombie.
    #[test]
    fn a_held_player_is_not_reached_for() {
        let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
        let library = include_str!("../../games/re2/lib/zombie.lua");
        let walk = |held: u8| -> Vec<u8> {
            lua.globals().set("HELD", held).unwrap();
            lua.load(format!(
                "local w = {{ r = {{ [0] = 1, 1, 1, 0 }}, vars = {{ [1] = 5, [2] = 5 }}, timers = {{ [0] = 9, 9, 9, 9 }} }}
                 local e = setmetatable({{}}, {{ __index = function() return function() return 0 end end }})
                 function e:routine(i) return w.r[i] end
                 function e:set_routine(i, v) w.r[i] = v end
                 function e:routines(a, b, c, d) w.r = {{ [0] = a, b, c, d }} end
                 function e:var(i) return w.vars[i] or 0 end
                 function e:set_var(i, v) w.vars[i] = v end
                 function e:timer(i) return w.timers[i] or 0 end
                 function e:set_timer(i, v) w.timers[i] = v end
                 function e:kind() return 6 end
                 function e:distance() return 1000 end
                 function e:turn_to_player() return 0 end
                 function e:same_floor() return true end
                 function e:player_id() return 0 end
                 function e:player_within() return true end
                 function e:player_life() return 200 end
                 function e:joint_bank() return false end
                 function e:speed() return 0, 0, 0 end
                 function e:pl_damage_cnt() return HELD end
                 local zombie = (function() {library} end)()
                 zombie.run(e, {{}})
                 R = {{ w.r[0], w.r[1] }}"
            ))
            .exec()
            .unwrap();
            lua.globals().get("R").unwrap()
        };
        assert_eq!(walk(0), [1, 3]);
        assert_ne!(walk(0x80)[1], 3);
    }

    /// `Em10Dm00` takes the Colt's 20 from `At_sce_no` (`0x5271c7 + 0xD`),
    /// where a handgun takes 15. The Beretta's rows in `Em10_Damage`'s table
    /// are past its end: they are the crawler's `FUN_00457d10`, whose clip is
    /// 6 and which leaves `At_sce_no` alone.
    #[test]
    fn the_colt_staggers_a_zombie_more_and_the_beretta_reads_the_crawlers_row() {
        assert_eq!(weapon_hit(6, 2, 0x30).0, 0, "not staggered");
        assert_eq!(weapon_hit(6, 2, 0x30).1, 0x30 - 15);
        assert_eq!(weapon_hit(6, crate::player2::COLT, 0x30).1, 0x30 - 20);
        let beretta = weapon_hit(6, crate::player2::BERETTA, 0x30);
        assert_eq!((beretta.1, beretta.3), (0x30, 6));
    }

    /// G3's hit (`FUN_00499260`) with the Beretta: `DWORD_0052a77c`'s
    /// `FUN_004995c0`, three bloods of kind 2 (kind 5, row 4, 0x1800, 300
    /// along the part), each at the part of `BYTE_0052a5dc` its `Rnd`
    /// picks, from the table's second half when `Damage_no` isn't a multiple
    /// of 3; then the hit's own two at `HIT_PART`.
    #[test]
    fn g3_sprays_blood_at_drawn_parts_by_weapon() {
        // `(effects, draws)` of one hit by `weapon`, `+0x223` at `gate`
        // before the tick counts it down.
        let hit = |weapon: u8, damage_no: u8, gate: u8| -> (Vec<Vec<i64>>, i64) {
            let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
            let script = include_str!("../../games/re2/enemy/33/enemy.lua");
            lua.globals().set("DAMAGE_NO", damage_no).unwrap();
            lua.globals().set("WEAPON", weapon).unwrap();
            lua.globals().set("GATE", gate).unwrap();
            lua.load(format!(
                "local w = {{ r = {{ [0] = 2, WEAPON, 0, 0 }}, vars = {{ [3] = GATE }}, draws = {{ 1, 6, 3 }}, drawn = 0 }}
                 E = {{}}
                 local e = setmetatable({{}}, {{ __index = function() return function() return 0 end end }})
                 function e:routine(i) return w.r[i] end
                 function e:set_routine(i, v) w.r[i] = v end
                 function e:routines(a, b, c, d) w.r = {{ [0] = a, b, c, d }} end
                 function e:var(i) return w.vars[i] or 0 end
                 function e:set_var(i, v) w.vars[i] = v end
                 function e:rnd() w.drawn = w.drawn + 1 return w.draws[w.drawn] or 0 end
                 function e:damage_no() return DAMAGE_NO end
                 function e:easy() return false end
                 function e:effect(kind, row, size, dir, part, x, y, z)
                     E[#E + 1] = {{ kind, row, size, part, x }}
                 end
                 {script}
                 update(e)
                 DRAWN = w.drawn"
            ))
            .exec()
            .unwrap();
            (
                lua.globals().get("E").unwrap(),
                lua.globals().get("DRAWN").unwrap(),
            )
        };
        let effects = |damage_no: u8| hit(0x13, damage_no, 0).0;
        // Draws 1, 6 and 3 pick parts 25, 17 and 27; with the second half
        // (indices 5, 10 and 7, masked by 7), 4, 26 and 11.
        let parts = |calls: &[Vec<i64>]| -> Vec<i64> { calls[..3].iter().map(|c| c[3]).collect() };
        let first = effects(0);
        assert_eq!(parts(&first), [25, 17, 27]);
        assert_eq!(first[0], [5, 4, 0x1800, 25, 300]);
        assert_eq!(parts(&effects(1)), [4, 26, 11]);
        // 0xa and 0x10 bleed six times; the hit's own two bloods follow.
        assert_eq!(hit(0xA, 0, 0).1, 6);
        assert_eq!(hit(0xA, 0, 0).0.len(), 6 + 2);
        assert_eq!(hit(0x10, 0, 1).0.len(), 6 + 2);
        // 0x10 with `+0x223` still counting (2, down to 1 this tick) bleeds
        // nothing and draws nothing.
        assert_eq!(hit(0x10, 0, 2).1, 0);
        assert_eq!(hit(0x10, 0, 2).0.len(), 2);
    }

    #[test]
    fn a_zombie_without_0x40_draws_its_stagger_counter_again() {
        // The walk's clip 0, its stagger, one draw: 7 & 0xF + 0x10.
        assert_eq!(handgun_hit(6), (0, 0x17, 1, 0, 1));
        assert_eq!(handgun_hit(0x46), (1, 1, 1, 4, 0));
    }

    /// `FUN_00455170` (`Routine_1` 0xD, which room 114's `main_05` sets on
    /// Claire A as it lets its zombie go) begins the zombie again as
    /// `Em10_Init` does: its third draw is `At_sce_no` (0x455235), and it
    /// forgets a stagger (`At_obj_no`, 0x455210), `+0x230` and `D_life`.
    /// There the original's draw left the zombie a stagger for its third
    /// Browning hit where `Em10_Init`'s, kept, knocked it back.
    #[test]
    fn rising_again_forgets_a_stagger_and_draws_its_counter_again() {
        let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
        let library = include_str!("../../games/re2/lib/zombie.lua");
        lua.load(format!(
            "local DRAWS = {{ 3, 1, 0x2B }}
             local w = {{ r = {{ [0] = 1, 0xD, 0, 0 }}, vars = {{ [3] = 1, [4] = -14, [11] = 2 }},
                 d_life = 0, drawn = 0 }}
             local e = setmetatable({{}}, {{ __index = function() return function() return 0 end end }})
             function e:routine(i) return w.r[i] end
             function e:set_routine(i, v) w.r[i] = v end
             function e:routines(a, b, c, d) w.r = {{ [0] = a, b, c, d }} end
             function e:var(i) return w.vars[i] or 0 end
             function e:set_var(i, v) w.vars[i] = v end
             function e:set_d_life(v) w.d_life = v end
             function e:kind() return 6 end
             function e:rnd() w.drawn = w.drawn + 1 return DRAWS[w.drawn] or 0 end
             local zombie = (function() {library} end)()
             zombie.run(e, {{}})
             R = {{ w.vars[3], w.vars[4], w.vars[11], w.d_life, w.vars[0], w.drawn }}"
        ))
        .exec()
        .unwrap();
        let r: Vec<i64> = lua.globals().get("R").unwrap();
        // 0x2B & 0xF + 0x10; the walk is clip 2 by the second draw's odd.
        assert_eq!(r, [0, 0x1B, 0, 0xD, 2, 3]);
        // What the counter decides two Browning hits (15 each) later: with 1
        // left, the third staggers (`Em10Dm00`, its walk's clip 0); with 0 or
        // under, it knocks back (`Em10Dm01`, clip 4).
        assert_eq!(weapon_hit(6, 3, 1).0, 0);
        assert_eq!(weapon_hit(6, 3, 1).3, 0);
        assert_eq!(weapon_hit(6, 3, 0).0, 1);
        assert_eq!(weapon_hit(6, 3, 0).3, 4);
    }

    /// `Em10_Init` under the new game's choice: what it leaves as
    /// `(life, flags, At_sce_no, draws)`. `Rnd` gives `draws` in turn and 0
    /// after; the room has one enemy, so the life gets its 15.
    fn zombie_init(setup: &str, draws: &[u8]) -> (i64, i64, i64, i64) {
        let lua = zombie_init_state(setup, draws);
        let r: Vec<i64> = lua.globals().get("R").unwrap();
        (r[0], r[1], r[2], r[3])
    }

    /// [`zombie_init`]'s run, kept for its other globals: `ENEMIES` (the
    /// room's count, 1 unless the setup says) and `MODEL_TYPE` go in, and
    /// `SKIN`, `PARTS0_Y` and `ROUTINES` (the four) come out beside `R`.
    fn zombie_init_state(setup: &str, draws: &[u8]) -> mlua::Lua {
        let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
        let library = include_str!("../../games/re2/lib/zombie.lua");
        lua.globals().set("DRAWS", draws.to_vec()).unwrap();
        lua.load(format!(
            "HARD, ARRANGE, ROOM, KIND, V = false, false, 0x11B, 0, {{}}
             ENEMIES, MODEL_TYPE, SKIN, PARTS0_Y, BURNS = 1, 0, 0, 0x1000, 0
             {setup}
             local w = {{ r = {{ [0] = 0, 0, 0, 0 }}, vars = {{}}, flags = 0, life = 0,
                 kind = KIND, drawn = 0 }}
             local e = setmetatable({{}}, {{ __index = function() return function() return 0 end end }})
             function e:routine(i) return w.r[i] end
             function e:set_routine(i, v) w.r[i] = v end
             function e:routines(a, b, c, d) w.r = {{ [0] = a, b, c, d }} end
             function e:rnd() w.drawn = w.drawn + 1 return DRAWS[w.drawn] or 0 end
             function e:life() return w.life end
             function e:set_life(v) w.life = v end
             function e:var(i) return w.vars[i] or 0 end
             function e:set_var(i, v) w.vars[i] = v end
             function e:flags() return w.flags end
             function e:set_flags(v) w.flags = v end
             function e:kind() return w.kind end
             function e:set_kind(v) w.kind = v end
             function e:easy() return false end
             function e:enemies() return ENEMIES end
             function e:hard_mode() return HARD end
             function e:censorship_off() return ARRANGE end
             function e:room_code() return ROOM end
             function e:model_type() return MODEL_TYPE end
             function e:set_skin(v) SKIN = v end
             function e:set_parts0_y(v) PARTS0_Y = v end
             function e:burn_parts() BURNS = BURNS + 1 end
             local zombie = (function() {library} end)()
             zombie.run(e, V)
             R = {{ w.life, w.flags, w.vars[4], w.drawn }}
             ROUTINES = {{ w.r[0], w.r[1], w.r[2], w.r[3] }}"
        ))
        .exec()
        .unwrap();
        lua
    }

    /// `FUN_00456780` darkens the parts unless the work's `Routine_1` (byte
    /// +5) is 0x10, which `Em10_Init` never leaves it at: room 10A's burning
    /// zombie, an 0x10, reads 0x404040 in the original's `zp`, as room 100's
    /// 0x1F do. An unburnt body isn't darkened.
    #[test]
    fn a_burning_0x10_darkens_its_parts_as_every_id_does() {
        let file = include_str!("../../games/re2/enemy/10/enemy.lua");
        let variant = file
            .lines()
            .find_map(|line| line.strip_prefix("local VARIANT <const> = "))
            .unwrap();
        for (kind, burns) in [(0x86, 1), (0x06, 0)] {
            let lua = zombie_init_state(&format!("V = {variant}; KIND = {kind}"), &[]);
            assert_eq!(
                lua.globals().get::<i64>("BURNS").unwrap(),
                burns,
                "Type {kind:#x}"
            );
        }
    }

    /// Room 207's three 0x1F zombies have `Model_type` 1: `Em10_Init`
    /// (0x451A80) still draws the three `Rnd` of the skin roll, since
    /// `Model_type & 0xC0` is clear, and then `& 0x3F` picks skin 1. With
    /// `Type` 6 the routines are 1,1,0,0, and with `Type` 0 they are 1,0,0,0.
    #[test]
    fn a_1f_with_model_type_1_draws_three_for_its_skin_and_wears_skin_1() {
        let file = include_str!("../../games/re2/enemy/1f/enemy.lua");
        assert!(file.contains("VARIANT <const> = { skins = true }"));
        let variant = "V = { skins = true }; MODEL_TYPE = 1; ENEMIES = 4";
        for (kind, routines) in [(6, [1, 1, 0, 0]), (0, [1, 0, 0, 0])] {
            let lua = zombie_init_state(
                &format!("{variant}; KIND = {kind}"),
                &[0, 0, 0, 0, 0, 3, 7, 9],
            );
            let globals = lua.globals();
            let r: Vec<i64> = globals.get("R").unwrap();
            assert_eq!(r[3], 8, "five draws and the skin's three");
            assert_eq!(globals.get::<i64>("SKIN").unwrap(), 1);
            assert_eq!(globals.get::<Vec<i64>>("ROUTINES").unwrap(), routines);
        }
        // `Model_type` 0x40 skips the roll's three draws. The skin isn't
        // asserted: on this path (`& 0xC0` set, `& 0x3F` zero) the original
        // takes it from the low bits of the work's pointer (0x4522A6 to
        // 0x4522F0), which arklay doesn't model.
        let lua = zombie_init_state("V = { skins = true }; MODEL_TYPE = 0x40", &[]);
        let r: Vec<i64> = lua.globals().get("R").unwrap();
        assert_eq!(r[3], 5);
    }

    /// Room 207's slot 0 is a 0x13: five draws, as every id, and `Parts0_pos_y`
    /// 0xE66, so it stands taller.
    #[test]
    fn a_13_draws_five_and_stands_taller() {
        let variant = include_str!("../../games/re2/enemy/13/enemy.lua");
        assert!(variant.contains("VARIANT <const> = { parts0_y = 0xE66 }"));
        let lua = zombie_init_state("V = { parts0_y = 0xE66 }; KIND = 6", &[]);
        let r: Vec<i64> = lua.globals().get("R").unwrap();
        assert_eq!(r[3], 5);
        assert_eq!(lua.globals().get::<i64>("PARTS0_Y").unwrap(), 0xE66);
        assert_eq!(
            lua.globals().get::<Vec<i64>>("ROUTINES").unwrap(),
            [1, 1, 0, 0]
        );
    }

    /// `Em10_Init` adds 15 to the life only while `BYTE_00989eeb`, the room's
    /// enemy count, is under 4: room 207's four get none.
    #[test]
    fn four_enemies_get_no_life_bonus() {
        assert_eq!(zombie_init("ENEMIES = 4", &[]).0, 70);
        assert_eq!(zombie_init("ENEMIES = 3", &[]).0, 85);
    }

    #[test]
    fn a_hard_zombie_has_five_times_the_life_and_the_quick_step() {
        assert_eq!(zombie_init("", &[]), (85, 0, 0x10, 5));
        assert_eq!(
            zombie_init("HARD = true", &[0, 0, 0, 0, 0, 3]),
            (425, 0x8000, 0x23, 6)
        );
        assert_eq!(zombie_init("HARD, ROOM = true, 0x610", &[]).0, 255);
        assert_eq!(zombie_init("HARD, V = true, { life = 0xFA }", &[]).0, 1250);
        // A crawler halves what HARD gave it, and kind 5 still ends dead.
        assert_eq!(zombie_init("HARD, KIND = true, 1", &[]).0, 212);
        assert_eq!(zombie_init("HARD, KIND = true, 5", &[]).0, -1);
    }

    #[test]
    fn arrange_mode_gives_one_zombie_in_three_the_quick_step() {
        // The roll's two draws, shifted 0: 1 isn't a multiple of 3.
        assert_eq!(
            zombie_init("ARRANGE = true", &[0, 0, 0, 0, 0, 0, 1]),
            (85, 0, 0x10, 7)
        );
        assert_eq!(
            zombie_init("ARRANGE = true", &[0, 0, 0, 0, 0, 0, 3, 5]),
            (85, 0x8000, 0x25, 8)
        );
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

    /// `SPIDER_STUB`'s work with what `enemy/27/enemy.lua` needs on top:
    /// `Goto00` and `Goto00_ck` as the engine's (toward `TARGET`, or toward
    /// `DEST_TARGET` for `Dest`), `Add_speedXYZ` along x, the clip's fields,
    /// and `TIMERS`, `SPEED` and `PL_DC` to start from.
    const EMBRYO_STUB: &str = "
        w.timers = TIMERS or { [0] = 0, 0, 0, 0 }
        w.spd = SPEED or w.spd
        w.move_no, w.move_cnt, w.hokan, w.xz, w.hidden, w.shadow = 0, 0, 0, 0, 0, -1
        w.rot_by = -1
        function e:clip(n, c, h)
            w.clips[#w.clips + 1] = n
            w.move_no, w.move_cnt, w.hokan, w.frame = n, c, h, 0
        end
        function e:move_no() return w.move_no end
        function e:set_move_no(v) w.move_no = v end
        function e:set_hokan(v) w.hokan = v end
        function e:set_kind(v) w.kind = v end
        function e:set_hidden_parts(v) w.hidden = v end
        function e:set_shadow(v) w.shadow = v end
        function e:dest() return 7, 7 end
        function e:goto_ck(x, z, rate)
            local to = (x == 7 and z == 7) and (DEST_TARGET or target) or target
            return turn_within(w.angle, to, rate)
        end
        function e:goto_point(x, z, rate) w.angle = goto_angle(w.angle, target, rate) end
        function e:add_speed_xz() w.xz = w.xz + 1 end
        function e:add_speed_xyz()
            w.pos = { w.pos[1] + w.spd[1], w.pos[2] + w.spd[2], w.pos[3] }
        end
        function e:rot_apply(ax, ay, az, x, y, z) w.rot_by = ay; return x, y, z end
        function e:pl_damage_cnt() return PL_DC or 0 end
        function e:effect() w.blown = w.blown + 1 end
        function e:kill() w.be = 0; w.r = { [0] = 0, 0, 0, 0 }; w.died = 1 end
        for _, name in ipairs({ 'set_damage_cnt', 'set_damage_no', 'set_atd', 'spd' }) do
            e[name] = function() end
        end
    ";

    /// What `run_embryo` leaves, by name.
    fn run_embryo(kind: u16, draws: &[u8], ticks: usize, setup: &str) -> Spider {
        let (lua, budget) = crate::lua::new_sandboxed_state().unwrap();
        lua.load(setup).exec().unwrap();
        // Each tick is a call of its own, with its own budget, as in the game.
        lua.globals()
            .set(
                "refill",
                lua.create_function(move |_, ()| {
                    budget.begin();
                    Ok(())
                })
                .unwrap(),
            )
            .unwrap();
        let script = include_str!("../../games/re2/enemy/27/enemy.lua");
        lua.globals().set("DRAWS", draws.to_vec()).unwrap();
        lua.globals().set("KIND", kind).unwrap();
        lua.globals()
            .set(
                "goto_angle",
                lua.create_function(|_, (angle, to, rate): (u16, u16, i32)| {
                    Ok(goto(angle, to, rate))
                })
                .unwrap(),
            )
            .unwrap();
        lua.globals()
            .set(
                "turn_within",
                lua.create_function(|_, (angle, to, rate): (u16, u16, i32)| {
                    Ok(turn_within(angle, to, rate))
                })
                .unwrap(),
            )
            .unwrap();
        lua.load(format!(
            "{SPIDER_STUB}
             {EMBRYO_STUB}
             {LATCH_STUB}
             {script}
             for _ = 1, {ticks} do refill(); update(e) end
             return {{ r0 = w.r[0], r1 = w.r[1], r2 = w.r[2], r3 = w.r[3], life = w.life,
                 be = w.be, kind = w.kind, angle = w.angle, spd = w.spd[1], spd_y = w.spd[2],
                 timer0 = w.timers[0], timer1 = w.timers[1], timer2 = w.timers[2],
                 timer3 = w.timers[3], drawn = drawn, h0 = w.vars[0] or 0,
                 h5 = w.vars[5] or 0, rgb = w.vars[6] or 0, pos_x = w.pos[1],
                 pos_y = w.pos[2], effects = w.bloods, squash_effects = w.blown,
                 died = w.died, move_no = w.move_no, move_cnt = w.move_cnt,
                 hokan = w.hokan, frame = w.frame, clips = #w.clips, xz = w.xz,
                 hidden = w.hidden, shadow = w.shadow, rot_by = w.rot_by }}"
        ))
        .eval::<mlua::Table>()
        .map(|table| {
            let fields = "r0 r1 r2 r3 life be kind angle spd spd_y timer0 timer1 timer2 \
                timer3 drawn h0 h5 rgb pos_x pos_y effects squash_effects died move_no \
                move_cnt hokan frame clips xz hidden shadow rot_by"
                .split_whitespace()
                .map(|name| (name.to_string(), table.get::<i64>(name).unwrap()))
                .collect();
            Spider(fields, Vec::new())
        })
        .unwrap()
    }

    /// What the latch (`FUN_00475280`) needs of the player on top of
    /// `EMBRYO_STUB`, where the stubs before it don't have it already: their
    /// routines, facing and parts, and the presses.
    const LATCH_STUB: &str = "
        e.pl_routine = e.pl_routine or function(_, i) return w.pl[i] end
        e.pl_set_routine = e.pl_set_routine or function(_, a, b, c, d) w.pl = { [0] = a, b, c, d } end
        e.pl_set_routine_at = e.pl_set_routine_at or function(_, i, v) w.pl[i] = v end
        e.pl_hold = e.pl_hold or function(_) w.held = 1 end
        e.pl_set_sub1 = e.pl_set_sub1 or function() end
        e.player_angle = e.player_angle or function(_) return 0 end
        e.player_part_point = e.player_part_point or function(_, part) return 0, PART_Y or -1000, 0 end
        e.direction_from_player = e.direction_from_player or function(_) return 0 end
        e.gacha = e.gacha or function(_) return PRESS or false end
        e.noise = e.noise or function(_) return 0 end
        e.set_pos = e.set_pos or function(_, x, y, z) w.pos = { x, y, z } end
        e.set_pos_y = e.set_pos_y or function(_, y) w.pos[2] = y end
        e.damage_cnt = e.damage_cnt or function(_) return 0 end
        e.sound = e.sound or function() end
        e.player_life = e.player_life or function(_) return 200 end
    ";

    /// `FUN_00475280`'s `Routine_2` 5: the player is caught when `Timer0`
    /// runs out, and four ticks after the latch climbs on (6).
    #[test]
    fn the_latch_climbs_four_ticks_after_the_catch() {
        let setup = "ROUTINES = { [0] = 1, 4, 5, 0 } TIMERS = { [0] = 0, 0, 0, 0 }";
        let embryo = run_embryo(0x2003, &[0; 16], 4, setup);
        assert_eq!(routines(&embryo), (1, 4, 6, 0));
        let embryo = run_embryo(0x2003, &[0; 16], 3, setup);
        assert_eq!(routines(&embryo), (1, 4, 5, 0));
    }

    fn routines(embryo: &Spider) -> (i64, i64, i64, i64) {
        (
            embryo.get("r0"),
            embryo.get("r1"),
            embryo.get("r2"),
            embryo.get("r3"),
        )
    }

    /// `FUN_004742e0`: a free embryo (`Type` 3) draws its life twice and
    /// crawls; the room's template (`Type` 0) draws once, hides and does
    /// nothing after.
    #[test]
    fn an_embryos_init() {
        let embryo = run_embryo(3, &[1, 0], 1, "");
        assert_eq!(routines(&embryo), (1, 1, 0, 0));
        assert_eq!((embryo.get("life"), embryo.get("kind")), (2, 0x2003));
        assert_eq!(embryo.get("be") & 0x440_0002, 0x440_0002);
        assert_eq!((embryo.get("spd"), embryo.get("drawn")), (0x50, 2));
        assert_eq!((embryo.get("shadow"), embryo.get("rgb")), (400, 0x80_8080));
        let template = run_embryo(0, &[1], 1, "");
        assert_eq!(routines(&template), (1, 0, 0, 0));
        assert_eq!((template.get("life"), template.get("kind")), (2, 0x6000));
        assert_eq!(template.get("be") & 0x400_0000, 0);
        assert_eq!((template.get("hidden"), template.get("shadow")), (0x3FF, 0));
        assert_eq!(template.get("drawn"), 1);
        // The next tick does nothing, not even `Joint_move`.
        let template = run_embryo(0, &[1], 2, "");
        assert_eq!((template.get("drawn"), template.get("frame")), (1, 1));
        assert_eq!(routines(&template), (1, 0, 0, 0));
    }

    /// The crawl's first tick: `Timer0`, `Timer2`'s refill and `Rnd & 3`,
    /// the speed's steps, the turn by `Timer1`, and a step along the facing
    /// of six ticks before, not the body's.
    #[test]
    fn the_crawl_starts_and_steps() {
        let embryo = run_embryo(3, &[0, 0, 5, 3, 1], 2, "VARS = { [1] = 0x400 }");
        assert_eq!(embryo.get("drawn"), 5);
        assert_eq!(embryo.get("timer0"), 0x19 - 1);
        assert_eq!((embryo.get("timer2"), embryo.get("timer3")), (13, 6));
        // 0x50 - 10, then 13 * 3 as `Timer3` < `Timer2`.
        assert_eq!(embryo.get("spd"), 0x50 - 10 + 39);
        assert_eq!(embryo.get("rot_by"), 0x400);
        assert_eq!(embryo.get("pos_x"), 0x50 - 10 + 39);
        // `Dest` is behind it (0x800), on the left: `Timer1` -4 and -8 more.
        assert_eq!(embryo.get("timer1"), 0xFFF4);
        assert_eq!(embryo.get("angle"), 0xFFF4);
    }

    /// `Timer1` grows by 4 a tick toward `Dest`, and by 12 the other way
    /// while above -0x40 (the original's comparisons, both against -0x40).
    #[test]
    fn the_crawl_turns_by_timer1() {
        let embryo = run_embryo(3, &[1; 16], 4, "DEST_TARGET = 0x400");
        assert_eq!(
            (embryo.get("timer1"), embryo.get("angle")),
            (12, 4 + 8 + 12)
        );
        let embryo = run_embryo(3, &[1; 16], 3, "DEST_TARGET = 0xC00");
        assert_eq!(embryo.get("timer1"), 0x1_0000 - 24);
        assert_eq!(embryo.get("angle"), 0x1_0000 - 36);
    }

    /// After each tick the newest of the history is `Cdir_y` and the oldest
    /// is the facing five ticks before.
    #[test]
    fn the_history_shifts() {
        // `Timer1` -12, -24, -36, -48, -60, -64: the angles after ticks 2 to 7
        // are -12, -36, -72, -120, -180 and -244.
        let embryo = run_embryo(3, &[1; 32], 7, "DEST_TARGET = 0xC00");
        assert_eq!(embryo.get("angle"), 0x1_0000 - 244);
        assert_eq!(embryo.get("h5"), embryo.get("angle"));
        assert_eq!(embryo.get("h0"), 0x1_0000 - 12);
        // Tick 7 stepped along tick 1's facing.
        assert_eq!(embryo.get("rot_by"), 0);
    }

    /// As `Timer0` runs out, an odd draw slides (5), and another odd one
    /// rests (7) instead; an even one keeps crawling. `Timer0` refills.
    #[test]
    fn the_crawl_leaves_for_5_or_7() {
        let setup = "ROUTINES = { [0] = 1, 1, 1, 0 }; TIMERS = { [0] = 1, 0, 5, 2 }";
        let embryo = run_embryo(0x2003, &[1, 1, 1, 5], 1, setup);
        assert_eq!(routines(&embryo), (1, 7, 0, 0));
        assert_eq!((embryo.get("timer0"), embryo.get("drawn")), (0x19, 4));
        let embryo = run_embryo(0x2003, &[1, 1, 0, 5], 1, setup);
        assert_eq!(routines(&embryo), (1, 5, 0, 0));
        let embryo = run_embryo(0x2003, &[1, 0, 5], 1, setup);
        assert_eq!(routines(&embryo), (1, 1, 1, 0));
        assert_eq!((embryo.get("timer0"), embryo.get("drawn")), (0x19, 3));
    }

    /// `FUN_00474690`: within 3000 and facing the player within 0x80, the
    /// jump starts on the same tick.
    #[test]
    fn the_pre_jumps_when_faced_and_near() {
        let embryo = run_embryo(3, &[0; 8], 2, "DIST = 2999; TARGET = 0");
        assert_eq!(routines(&embryo), (1, 2, 1, 0));
        assert_eq!((embryo.get("spd"), embryo.get("spd_y")), (0x50 + 200, -180));
        assert_eq!((embryo.get("move_no"), embryo.get("hokan")), (2, 3));
        assert_eq!(embryo.get("drawn"), 2);
        let embryo = run_embryo(3, &[0; 8], 2, "DIST = 3000; TARGET = 0");
        assert_eq!(embryo.get("r1"), 1);
        let embryo = run_embryo(3, &[0; 8], 2, "DIST = 2999; TARGET = 0x100");
        assert_eq!(embryo.get("r1"), 1);
    }

    /// The jump rises at -180 less 15 a tick more each tick, takes clip 3
    /// once falling, and lands on `Ground` into the crawl or, on an odd
    /// draw, the rest; within 700 of a player who isn't being hurt it
    /// latches on (4).
    #[test]
    fn the_jump_lands_or_latches() {
        let setup = "ROUTINES = { [0] = 1, 2, 0, 0 }";
        let embryo = run_embryo(0x2003, &[0], 5, setup);
        assert_eq!(embryo.get("move_no"), 2);
        let embryo = run_embryo(0x2003, &[0], 6, setup);
        assert_eq!((embryo.get("move_no"), embryo.get("hokan")), (3, 7));
        let embryo = run_embryo(0x2003, &[0], 10, setup);
        assert_eq!((embryo.get("r1"), embryo.get("pos_y")), (2, 0));
        let embryo = run_embryo(0x2003, &[0], 11, setup);
        assert_eq!(routines(&embryo), (1, 1, 0, 0));
        assert_eq!((embryo.get("pos_y"), embryo.get("spd")), (0, 200 - 0x96));
        let embryo = run_embryo(0x2003, &[1], 11, setup);
        assert_eq!(routines(&embryo), (1, 7, 0, 0));
        let embryo = run_embryo(0x2003, &[0], 1, &format!("{setup}; DIST = 699"));
        assert_eq!(routines(&embryo), (1, 4, 0, 0));
        let embryo = run_embryo(0x2003, &[0], 1, &format!("{setup}; DIST = 699; PL_DC = 1"));
        assert_eq!(routines(&embryo), (1, 2, 1, 0));
    }

    /// A player stepping within 400 squashes a crawling, sliding or resting
    /// embryo: three effects and clip 9 from four draws.
    #[test]
    fn a_tread_in_crawl_slide_and_rest() {
        for state in [1, 5, 7] {
            let setup = format!(
                "ROUTINES = {{ [0] = 1, {state}, 1, 0 }}; SPEED = {{ 50, 0, 0 }}; \
                 DIST = 399; NOISE = 2"
            );
            let embryo = run_embryo(0x2003, &[1, 2, 3, 0x0D], 1, &setup);
            assert_eq!(routines(&embryo), (3, 1, 1, 0), "state {state}");
            assert_eq!((embryo.get("spd"), embryo.get("effects")), (0, 3));
            assert_eq!((embryo.get("move_no"), embryo.get("move_cnt")), (9, 5));
            assert_eq!(embryo.get("drawn"), 4);
        }
        for quiet in ["DIST = 399; NOISE = 1", "DIST = 400; NOISE = 2"] {
            let setup = format!("ROUTINES = {{ [0] = 1, 7, 1, 0 }}; {quiet}");
            let embryo = run_embryo(0x2003, &[0; 8], 1, &setup);
            assert_eq!(embryo.get("r0"), 1, "{quiet}");
        }
    }

    /// The slide (5) slows by `2 * Timer0` to a stop, waits, then either
    /// crawls again or turns away speeding up to 100; the rest (7) slows by
    /// `5 * Timer0`, waits, and a near player starts clip 0xD at once.
    #[test]
    fn the_slide_and_the_rest() {
        let setup = "ROUTINES = { [0] = 1, 5, 0, 0 }; SPEED = { 10, 0, 0 }";
        let embryo = run_embryo(0x2003, &[0, 1], 4, setup);
        assert_eq!((embryo.get("r2"), embryo.get("timer0")), (2, 0x1E));
        assert_eq!((embryo.get("spd"), embryo.get("xz")), (-2, 4));
        let embryo = run_embryo(0x2003, &[0, 1], 4 + 31, setup);
        assert_eq!(routines(&embryo), (1, 1, 0, 0));
        let embryo = run_embryo(0x2003, &[0, 0, 0x10], 4 + 31, setup);
        assert_eq!(routines(&embryo), (1, 5, 3, 0));
        let embryo = run_embryo(0x2003, &[0, 0, 0x10], 4 + 31 + 10, setup);
        assert_eq!((embryo.get("r1"), embryo.get("spd")), (5, 98));
        assert_eq!(embryo.get("timer1"), 0x10 + 0x1E);
        let embryo = run_embryo(0x2003, &[0, 0, 0x10], 4 + 31 + 11, setup);
        assert_eq!(routines(&embryo), (1, 1, 0, 0));
        assert_eq!(embryo.get("spd"), 100);

        let setup = "ROUTINES = { [0] = 1, 7, 0, 0 }; SPEED = { 10, 0, 0 }";
        let embryo = run_embryo(0x2003, &[0], 3, setup);
        assert_eq!((embryo.get("r2"), embryo.get("timer0")), (2, 0x3C));
        let embryo = run_embryo(0x2003, &[0], 3 + 60, setup);
        assert_eq!(embryo.get("r1"), 7);
        let embryo = run_embryo(0x2003, &[0], 3 + 61, setup);
        assert_eq!(routines(&embryo), (1, 1, 0, 0));
        assert_eq!(embryo.get("spd"), 0x50);
        let near = format!("{setup}; DIST = 2499; CLIP_LEN = 3");
        let embryo = run_embryo(0x2003, &[0], 4, &near);
        assert_eq!((embryo.get("r2"), embryo.get("move_no")), (3, 0xD));
        let embryo = run_embryo(0x2003, &[0], 6, &near);
        assert_eq!(embryo.get("r2"), 3);
        let embryo = run_embryo(0x2003, &[0], 7, &near);
        assert_eq!(routines(&embryo), (1, 1, 0, 0));
        assert_eq!(embryo.get("spd"), 100);
    }

    /// A trodden embryo: two ticks to a stop, 103 of the squash, the fade's
    /// 111 with the parts at `0x808080`, then `Kill_enemy_work`.
    #[test]
    fn a_trodden_embryo_squashes_and_goes() {
        let setup = "ROUTINES = { [0] = 3, 1, 1, 0 }; TIMERS = { [0] = 0, 0, 0, 5 }; \
            VARS = { [6] = 0x808080 }";
        let embryo = run_embryo(0x2003, &[0], 1, setup);
        assert_eq!((embryo.get("r2"), embryo.get("spd")), (1, 0));
        let embryo = run_embryo(0x2003, &[0], 2, setup);
        assert_eq!((embryo.get("r2"), embryo.get("drawn")), (2, 1));
        let embryo = run_embryo(0x2003, &[0], 104, setup);
        assert_eq!(embryo.get("r2"), 2);
        let embryo = run_embryo(0x2003, &[0], 105, setup);
        assert_eq!(embryo.get("r2"), 3);
        assert_eq!(embryo.get("rgb"), 0x20_8020);
        // `Timer0` 0x1E and 0x3C step the clip on.
        assert_eq!((embryo.get("move_no"), embryo.get("hokan")), (2, 7));
        let embryo = run_embryo(0x2003, &[0], 106, setup);
        assert_eq!(routines(&embryo), (7, 0, 0, 0));
        let embryo = run_embryo(0x2003, &[0], 216, setup);
        assert_eq!((embryo.get("r1"), embryo.get("died")), (2, 0));
        assert_eq!(embryo.get("life"), -1);
        let embryo = run_embryo(0x2003, &[0], 217, setup);
        assert_eq!((embryo.get("died"), embryo.get("be")), (1, 0));
        // With `Timer3` 0 (a body that never crawled) the squash draws three
        // more every 32 ticks for an effect.
        let embryo = run_embryo(
            0x2003,
            &[0; 16],
            105,
            "ROUTINES = { [0] = 3, 1, 1, 0 }; VARS = { [6] = 0x808080 }",
        );
        assert_eq!((embryo.get("squash_effects"), embryo.get("drawn")), (3, 10));
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

    /// `Em_2D`'s world: `DIST` is `L_pl`, the player stands at `PLAYER`
    /// (x, z) in the way `TARGET` from the body, `PARTS` gives the parts'
    /// places (x, z), a clip lasts `CLIP_LEN` frames, and `ROUTINES`, `VARS`,
    /// `ANGLE`, `TIMER`, `FRAME`, `GROAN`, `ROOM_ENEMY` and `PL_DC` start it
    /// past its init.
    const EM2D_STUB: &str = "
        local draws, drawn = DRAWS, 0
        local clip_len = CLIP_LEN or 1000
        local parts = PARTS or {}
        local player = PLAYER or { 0, 20000 }
        w = { r = ROUTINES or { [0] = 0, 0, 0, 0 }, vars = VARS or {}, timer = TIMER or 0,
            be = 1, kind = KIND, angle = ANGLE or 0, spd = { 0, 0, 0 }, pos = POS or { 0, 0, 0 },
            clips = {}, move_no = -1, frame = FRAME or 0, hokan = -1, hidden = -1, dc = 0,
            life = 0, sounds = {}, groan = GROAN or 0, room_enemy = ROOM_ENEMY or 0,
            pl_dc = PL_DC or 0, box = 0, joints = 0, pl_xz = { 0, 0 },
            pl_r = PL_R or { [0] = 1, 0, 0, 0 }, held = 0, sub1 = 0 }
        e = {}
        function e:routine(i) return w.r[i] end
        function e:set_routine(i, v) w.r[i] = v end
        function e:routines(a, b, c, d) w.r = { [0] = a, b, c, d } end
        function e:var(i) return w.vars[i] or 0 end
        function e:set_var(i, v) w.vars[i] = v end
        function e:timer(i) return w.timer end
        function e:set_timer(i, v) w.timer = v & 0xFFFF end
        function e:rnd() drawn = drawn + 1; return draws[drawn] end
        function e:be_flg() return w.be end
        function e:set_be_flg(v) w.be = v end
        function e:kind() return w.kind end
        function e:set_life(v) w.life = v end
        function e:set_damage_cnt(v) w.dc = v end
        function e:damage_cnt() return w.dc end
        function e:clip(n, c, h)
            w.clips[#w.clips + 1] = n; w.move_no = n; w.frame = c; w.hokan = h
        end
        function e:joint()
            w.joints = w.joints + 1; w.frame = w.frame + 1; return w.frame >= clip_len
        end
        function e:move_cnt() return w.frame end
        function e:step(ended) if ended then w.r[2] = w.r[2] + 1 end end
        function e:angle() return w.angle end
        function e:set_angle(v) w.angle = v end
        function e:set_speed(x, y, z) w.spd = { x, y, z } end
        function e:add_speed_xz_at(dir)
            local dx, dz = rotate((w.angle + dir) & 0xFFFF, w.spd[1], w.spd[3])
            w.pos = { w.pos[1] + dx, w.pos[2], w.pos[3] + dz }
        end
        function e:pos() return w.pos[1], w.pos[2], w.pos[3] end
        function e:set_pos(x, y, z) w.pos = { x, y, z } end
        function e:distance() return DIST or 20000 end
        function e:player_pos() return player[1], 0, player[2] end
        function e:goto_ck(x, z, rate)
            local error = ((TARGET or 0) - w.angle + rate) & 0xFFF
            if error < rate * 2 then return 0 end
            return error > 0x800 and -rate or rate
        end
        function e:angle_to_player() return ((TARGET or 0) - w.angle) & 0xFFF end
        function e:set_player_xz(x, z) w.pl_xz = { x, z } end
        function e:pl_set_routine(a, b, c, d) w.pl_r = { [0] = a, b, c, d } end
        function e:pl_set_routine_at(i, v) w.pl_r[i] = v end
        function e:pl_hold() w.held = 1 end
        function e:pl_set_sub1() w.sub1 = 1 end
        function e:gacha() return GACHA ~= nil and GACHA[w.timer] == true end
        function e:own_part(p)
            local at = parts[p] or { 0, 0 }
            return at[1], -2000, at[2]
        end
        function e:hypot(x, z) return math_hypot(x, z) end
        function e:set_hidden_parts(v) w.hidden = v end
        function e:sound(n) w.sounds[#w.sounds + 1] = n end
        function e:groan_wait() return w.groan end
        function e:set_groan_wait(v) w.groan = v & 0xFF end
        function e:room_enemy() return w.room_enemy end
        function e:set_room_enemy(v) w.room_enemy = v end
        function e:pl_damage_cnt() return w.pl_dc end
        function e:pl_set_damage_cnt(v) w.pl_dc = v end
        function e:set_body_box(y, at_w, at_h, wide) w.box = at_w end
        for _, name in ipairs({ 'set_d_life', 'set_damage_flg', 'set_sce_flg', 'clear_neck',
            'set_body_ofs_x' }) do
            e[name] = function() end
        end
    ";

    /// What `run_em2d` leaves: its fields by name, its clips and its sounds.
    struct Em2d(std::collections::HashMap<String, i64>, Vec<i64>, Vec<i64>);

    impl Em2d {
        fn get(&self, name: &str) -> i64 {
            self.0[name]
        }
    }

    /// `Em_2D`'s script on the stub for `ticks` ticks, with `setup`'s globals.
    fn run_em2d(kind: u16, draws: &[u8], ticks: usize, setup: &str) -> Em2d {
        let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
        lua.load(setup).exec().unwrap();
        let script = include_str!("../../games/re2/enemy/2d/enemy.lua");
        lua.globals().set("DRAWS", draws.to_vec()).unwrap();
        lua.globals().set("KIND", kind).unwrap();
        lua.globals()
            .set(
                "math_hypot",
                lua.create_function(|_, (x, z): (i32, i32)| Ok(square_root0(x * x + z * z)))
                    .unwrap(),
            )
            .unwrap();
        lua.globals()
            .set(
                "rotate",
                lua.create_function(|_, (angle, x, z): (i64, i32, i32)| {
                    Ok(player2::rotate(angle as u16, x, z))
                })
                .unwrap(),
            )
            .unwrap();
        lua.load(format!(
            "{EM2D_STUB}
             {script}
             for _ = 1, {ticks} do update(e) end
             return {{ r0 = w.r[0], r1 = w.r[1], r2 = w.r[2], r3 = w.r[3], life = w.life,
                 be = w.be, angle = w.angle, x = w.pos[1], z = w.pos[3], timer = w.timer,
                 hidden = w.hidden, groan = w.groan, room_enemy = w.room_enemy,
                 move_no = w.move_no, frame = w.frame, hokan = w.hokan, dc = w.dc,
                 pl_dc = w.pl_dc, drawn = drawn, box = w.box, joints = w.joints,
                 home_angle = w.vars[0] or 0, home_x = w.vars[1] or 0,
                 home_z = w.vars[2] or 0, seen = w.vars[3] or 0, clips = w.clips,
                 sounds = w.sounds, pl_x = w.pl_xz[1], pl_z = w.pl_xz[2], pl_r0 = w.pl_r[0],
                 pl_r2 = w.pl_r[2], held = w.held, sub1 = w.sub1 }}"
        ))
        .eval::<mlua::Table>()
        .map(|table| {
            let list = |name: &str| {
                table
                    .get::<mlua::Table>(name)
                    .unwrap()
                    .sequence_values()
                    .map(Result::unwrap)
                    .collect()
            };
            let fields = "r0 r1 r2 r3 life be angle x z timer hidden groan room_enemy move_no \
                frame hokan dc pl_dc drawn box joints home_angle home_x home_z seen pl_x pl_z pl_r0 \
                pl_r2 held sub1"
                .split_whitespace()
                .map(|name| (name.to_string(), table.get::<i64>(name).unwrap()))
                .collect();
            Em2d(fields, list("clips"), list("sounds"))
        })
        .unwrap()
    }

    /// Past the init and armed (`Routine_2` 2), pushed back into the wall
    /// from its home, then `setup`.
    fn em2d_armed(setup: &str) -> String {
        format!(
            "ROUTINES = {{ [0] = 1, 0, 2, 0 }}; ANGLE = 1024; POS = {{ -12100, -2480, -6600 }};
             VARS = {{ [0] = 1024, -12100, -7000, 0 }}; TARGET = 1024; {setup}"
        )
    }

    /// `FUN_00483eb0`: one draw for `Timer0`, the home, life 250, both bodies
    /// hidden and `Be_flg` `0x0C001407`.
    #[test]
    fn an_em2d_init_hides_both_bodies_and_draws_its_wait() {
        for kind in [0x10, 1] {
            let setup = "ANGLE = 1024; POS = { -12100, -2480, -7000 }; GROAN = 9; ROOM_ENEMY = 9";
            let em = run_em2d(kind, &[0x2B], 1, setup);
            let routines = [em.get("r0"), em.get("r1"), em.get("r2"), em.get("r3")];
            assert_eq!(routines, [1, 0, 0, 0]);
            assert_eq!((em.get("timer"), em.get("drawn")), (0xB, 1));
            assert_eq!((em.get("life"), em.get("be")), (250, 0x0C00_1407));
            assert_eq!(em.get("hidden"), 0x7FFE);
            let home = (em.get("home_angle"), em.get("home_x"), em.get("home_z"));
            assert_eq!(home, (1024, -12100, -7000));
            assert_eq!(em.get("seen"), 0);
            // A leader counts down before the init clears them.
            assert_eq!((em.get("groan"), em.get("room_enemy")), (0, 0));
            assert_eq!(em.1, vec![0]);
            assert_eq!(
                (em.get("hokan"), em.get("joints"), em.get("box")),
                (0, 1, 800)
            );
        }
    }

    /// `FUN_004842b0`'s step 0: home, then 400 back into the wall, and the
    /// timer counts down to the arming.
    #[test]
    fn an_em2d_backs_into_the_wall_then_arms() {
        let wall = "ANGLE = 1024; POS = { -12100, -2480, -7000 }";
        let em = run_em2d(0x10, &[0x2B], 2, wall);
        assert_eq!((em.get("x"), em.get("z")), (-12100, -6600));
        assert_eq!(
            (em.get("r2"), em.get("dc"), em.get("timer")),
            (1, 0x80, 0xA)
        );
        assert_eq!((em.get("move_no"), em.get("hokan")), (0, 0xF));
        let west = "ANGLE = 0; POS = { -27150, -2580, -12350 }";
        let em = run_em2d(0x10, &[0x2B], 2, west);
        assert_eq!((em.get("x"), em.get("z")), (-27550, -12350));
        // Armed on the tick its count passes 0: `Timer0 + 1` ticks of the wait.
        let em = run_em2d(0x10, &[0x2B], 1 + 0xB, wall);
        assert_eq!(em.get("r2"), 1);
        let em = run_em2d(0x10, &[0x2B], 1 + 0xB + 1, wall);
        assert_eq!((em.get("r2"), em.get("timer")), (2, 0xFFFF));
    }

    /// `FUN_004841c0`'s second branch: under 3000 and in front, it comes out
    /// and walks 50 a tick on clip 0, from the frame the wait left.
    #[test]
    fn an_armed_em2d_emerges_within_3000_in_front() {
        for (kind, shown) in [(0, 0x7F00), (1, 0x00FE)] {
            let em = run_em2d(kind, &[], 1, &em2d_armed("DIST = 2800; FRAME = 1"));
            assert_eq!((em.get("r1"), em.get("r2")), (0, 3));
            assert_eq!((em.get("hidden"), em.get("be")), (shown, 1));
            assert_eq!((em.get("seen"), em.get("dc")), (1, 0));
            assert!(em.2.is_empty() && em.1.is_empty());
            assert_eq!((em.get("frame"), em.get("z")), (2, -6650));
        }
        // From frame 11 on, it no longer moves.
        let em = run_em2d(0, &[], 1, &em2d_armed("DIST = 2800; FRAME = 10"));
        assert_eq!(em.get("z"), -6600);
        // At 3000, or off its facing by 0x2f8 or more, it stays in.
        let em = run_em2d(0, &[], 1, &em2d_armed("DIST = 3000"));
        assert_eq!(em.get("r2"), 2);
        let em = run_em2d(0, &[], 1, &em2d_armed("DIST = 2800; TARGET = 1024 + 0x2F8"));
        assert_eq!(em.get("r2"), 2);
        let em = run_em2d(0, &[], 1, &em2d_armed("DIST = 2800; TARGET = 1024 + 0x2F7"));
        assert_eq!(em.get("r2"), 3);
        // Not yet armed.
        let setup = em2d_armed("DIST = 2800; ROUTINES = { [0] = 1, 0, 1, 0 }; TIMER = 5");
        assert_eq!(run_em2d(0, &[], 1, &setup).get("r2"), 1);
    }

    /// `FUN_004841c0`'s first branch, and the head turn's three or four draws
    /// on the same tick. A second wake groans unless the room's wait runs.
    #[test]
    fn an_em2d_turns_its_head_within_2500() {
        let setup = em2d_armed("DIST = 2400; ROUTINES = { [0] = 1, 0, 3, 0 }");
        let em = run_em2d(0, &[1, 0x05, 3, 1], 1, &setup);
        let routines = [em.get("r0"), em.get("r1"), em.get("r2"), em.get("r3")];
        assert_eq!(routines, [1, 1, 1, 0]);
        assert_eq!((em.get("x"), em.get("z")), (-12100, -7000));
        assert_eq!((em.get("move_no"), em.get("hokan")), (2, 0xF));
        assert_eq!(em.get("timer"), 0x3C + 5 - 1);
        assert_eq!((em.get("drawn"), em.2.clone()), (4, vec![0]));
        assert_eq!(em.get("seen"), 1);
        // An even third draw plays nothing; an even fourth plays sound 1.
        let em = run_em2d(0, &[0, 0, 2, 0], 1, &setup);
        assert_eq!((em.get("move_no"), em.get("drawn"), em.2.len()), (1, 3, 0));
        let em = run_em2d(0, &[0, 0, 1, 2], 1, &setup);
        assert_eq!(em.2, vec![1]);
        // Woken before: the groan, and the room's wait before the next.
        let seen = em2d_armed("DIST = 2400; VARS = { [0] = 1024, -12100, -7000, 1 }");
        let em = run_em2d(0, &[0, 0, 0], 1, &seen);
        assert_eq!((em.2.clone(), em.get("groan")), (vec![4], 0x3C));
        let em = run_em2d(0, &[0, 0, 0], 1, &format!("{seen}; GROAN = 5"));
        assert_eq!((em.2.len(), em.get("groan")), (0, 5));
        // Off its facing by 0x400, no.
        let em = run_em2d(0, &[0, 0, 0], 1, &format!("{setup}; TARGET = 1024 + 0x400"));
        assert_eq!(em.get("r1"), 0);
    }

    /// `FUN_00484430`'s turn: 0x20 a tick, and never more than 0x180 from
    /// home, compared signed on the facing unmasked.
    #[test]
    fn an_em2d_head_turn_is_clamped_around_home() {
        let turning = |home: i64, target: i64, ticks: usize| {
            let setup = format!(
                "ROUTINES = {{ [0] = 1, 1, 1, 0 }}; ANGLE = {home}; TIMER = 500;
                 VARS = {{ [0] = {home}, 0, 0, 1 }}; TARGET = {target}; PLAYER = {{ 5000, 0 }}"
            );
            run_em2d(0, &[], ticks, &setup).get("angle")
        };
        assert_eq!(turning(0, 0xC00, 1), 0xFFE0);
        assert_eq!(turning(0, 0xC00, 12), 0xFE80);
        assert_eq!(turning(0, 0xC00, 13), 0xFE80);
        assert_eq!(turning(0, 0xC00, 40), 0xFE80);
        assert_eq!(turning(1024, 0x800, 40), 1024 + 0x180);
        assert_eq!(turning(1024, 0, 40), 1024 - 0x180);
        // A facing past the other side of home, as the reach's unclamped turn
        // can leave it, is clamped to that side.
        let setup = "ROUTINES = { [0] = 1, 1, 1, 0 }; ANGLE = 0xF80; TIMER = 500;
            VARS = { [0] = 0, 0, 0, 1 }; TARGET = 0xF80; PLAYER = { 5000, 0 }";
        assert_eq!(run_em2d(0, &[], 1, setup).get("angle"), 0x180);
        // The timer's end goes to the retreat.
        let setup = "ROUTINES = { [0] = 1, 1, 1, 0 }; TIMER = 0";
        assert_eq!(run_em2d(0, &[], 1, setup).get("r1"), 5);
    }

    /// `FUN_004843d0`: the player within 900 of the arm, part 3 or 10.
    #[test]
    fn an_em2d_reaches_when_the_player_is_within_900_of_its_arm() {
        for (kind, part) in [(0, 3), (1, 10)] {
            let at = |distance: i64| {
                let setup = format!(
                    "ROUTINES = {{ [0] = 1, 1, 1, 0 }}; TIMER = 50; PLAYER = {{ 0, 0 }};
                     PARTS = {{ [{part}] = {{ {distance}, 0 }} }}"
                );
                run_em2d(kind, &[], 1, &setup)
            };
            // `SquareRoot0` comes from a table, so the edges are approximate.
            let em = at(880);
            assert_eq!((em.get("r1"), em.get("r2")), (3, 1));
            assert_eq!(
                (em.get("move_no"), em.get("hokan"), em.get("frame")),
                (3, 0xF, 1)
            );
            assert_eq!(at(920).get("r1"), 1);
        }
    }

    /// `FUN_00484780`: sound 6, clip 4 and 30 a tick back, then hidden at
    /// `Life` -1 for good.
    #[test]
    fn an_em2d_retreats_and_ends() {
        let setup = "ROUTINES = { [0] = 1, 5, 0, 0 }; ANGLE = 1024; POS = { 0, 0, -7000 };
            CLIP_LEN = 3";
        let em = run_em2d(0, &[0x07], 1, setup);
        assert_eq!(
            (em.2.clone(), em.get("move_no"), em.get("hokan")),
            (vec![6], 4, 0xF)
        );
        assert_eq!((em.get("z"), em.get("dc"), em.get("r1")), (-6970, 0x80, 5));
        let em = run_em2d(0, &[0x07], 3, setup);
        assert_eq!(em.get("z"), -6910);
        assert_eq!(
            (em.get("timer"), em.get("hidden"), em.get("be")),
            (0x07 + 0x1E, 0x7FFE, 3)
        );
        let routines = [em.get("r0"), em.get("r1"), em.get("r2"), em.get("r3")];
        assert_eq!((routines, em.get("life")), ([1, 7, 0, 0], -1));
        let later = run_em2d(0, &[0x07], 10, setup);
        assert_eq!((later.get("z"), later.get("joints")), (-6910, 3));
    }

    /// `Em_2D`: only a body with `Type`'s 0x10 counts the room's two
    /// globals down.
    #[test]
    fn an_em2d_leader_counts_the_room_globals_down() {
        let setup = "ROUTINES = { [0] = 1, 7, 0, 0 }; ROOM_ENEMY = 5; GROAN = 3";
        let em = run_em2d(0x10, &[], 1, setup);
        assert_eq!((em.get("room_enemy"), em.get("groan")), (4, 2));
        let em = run_em2d(0x10, &[], 5, setup);
        assert_eq!((em.get("room_enemy"), em.get("groan")), (0, 0));
        let em = run_em2d(0x01, &[], 1, setup);
        assert_eq!((em.get("room_enemy"), em.get("groan")), (5, 3));
    }

    /// `FUN_00484530`: within 600 of the arm, past frame 4 and with no
    /// room wait, it takes the player, or waits while another body has them.
    #[test]
    fn an_em2d_grab_check_needs_600_a_late_frame_and_no_cooldown() {
        let at = |extra: &str| {
            let setup = format!(
                "ROUTINES = {{ [0] = 1, 3, 1, 0 }}; PLAYER = {{ 0, 0 }};
                 PARTS = {{ [3] = {{ 580, 0 }} }}; {extra}"
            );
            run_em2d(0, &[], 1, &setup)
        };
        let em = at("FRAME = 5");
        assert_eq!((em.get("r1"), em.get("pl_dc")), (4, 0x80));
        let em = at("FRAME = 5; PL_DC = 0x80");
        assert_eq!((em.get("r1"), em.get("r2"), em.get("move_no")), (6, 1, 5));
        assert_eq!(at("FRAME = 4").get("r1"), 3);
        assert_eq!(at("FRAME = 5; ROOM_ENEMY = 2").get("r1"), 3);
        assert_eq!(at("FRAME = 5; PARTS = { [3] = { 620, 0 } }").get("r1"), 3);
    }

    /// `FUN_00484850`: each step ends the tick, and the head turn follows
    /// once the player is free.
    #[test]
    fn an_em2d_holds_while_another_body_grabs() {
        let setup = "ROUTINES = { [0] = 1, 6, 0, 0 }; PL_DC = 0x80";
        let em = run_em2d(0, &[], 1, setup);
        assert_eq!(
            (em.get("r2"), em.get("move_no"), em.get("joints")),
            (1, 5, 0)
        );
        let em = run_em2d(0, &[], 3, setup);
        assert_eq!((em.get("r2"), em.get("joints")), (1, 2));
        let setup = "ROUTINES = { [0] = 1, 6, 1, 0 }";
        let em = run_em2d(0, &[], 1, setup);
        assert_eq!((em.get("r2"), em.get("joints")), (2, 1));
        let em = run_em2d(0, &[], 2, setup);
        assert_eq!((em.get("r1"), em.get("r2")), (1, 0));
    }

    /// `FUN_00484640`: the grab puts the player at its arm in routine 5, on
    /// its clips, with `Timer1` 150, and plays clip 5 from the next tick,
    /// with a cry on its first frame.
    #[test]
    fn an_em2d_grab_takes_the_player_to_its_arm() {
        let setup = "ROUTINES = { [0] = 1, 4, 0, 0 }; PARTS = { [3] = { -12300, -8100 } }";
        let em = run_em2d(0, &[], 1, setup);
        assert_eq!(
            (em.get("r2"), em.get("move_no"), em.get("joints")),
            (1, 5, 0)
        );
        assert_eq!((em.get("pl_x"), em.get("pl_z")), (-12300, -8100));
        assert_eq!((em.get("pl_r0"), em.get("held"), em.get("sub1")), (5, 1, 1));
        assert_eq!(em.get("timer"), 0x96);
        let em = run_em2d(0, &[1], 2, setup);
        assert_eq!(
            (em.get("joints"), em.get("timer"), em.get("drawn")),
            (1, 0x95, 1)
        );
        assert_eq!(em.2, vec![5]);
        let em = run_em2d(0, &[2], 2, setup);
        assert_eq!(em.2, vec![7]);
    }

    /// `FUN_00484640`'s step 1: `Timer1` counts down by one, and two more for
    /// a press; below 0 it sets the player's `Routine_2` to 2, and the next
    /// tick starts `fg_room_enemy`'s 120 and the retreat.
    #[test]
    fn an_em2d_grab_lets_go_when_its_timer_runs_out() {
        let setup =
            "ROUTINES = { [0] = 1, 4, 1, 0 }; FRAME = 1; TIMER = 1; PL_R = { [0] = 5, 0, 1, 0 }";
        let em = run_em2d(0, &[], 1, setup);
        assert_eq!((em.get("r2"), em.get("timer"), em.get("pl_r2")), (1, 0, 1));
        let em = run_em2d(0, &[], 2, setup);
        assert_eq!(
            (em.get("r2"), em.get("timer"), em.get("pl_r2")),
            (2, 0xFFFF, 2)
        );
        let em = run_em2d(0, &[], 3, setup);
        assert_eq!(
            (em.get("r1"), em.get("r2"), em.get("room_enemy")),
            (5, 0, 0x78)
        );
        // A press at `Timer1` 3 takes it to 0.
        let setup = format!("{setup}; TIMER = 3; GACHA = {{ [3] = true }}");
        let em = run_em2d(0, &[], 1, &setup);
        assert_eq!((em.get("r2"), em.get("timer")), (1, 0));
    }

    /// `FUN_004845d0`'s `Goto00` steps the facing without masking it to 12
    /// bits.
    #[test]
    fn an_em2d_reach_turn_is_left_unmasked() {
        let setup = "ROUTINES = { [0] = 1, 3, 1, 0 }; ANGLE = 0x10; TARGET = 0xF00";
        let em = run_em2d(0, &[], 1, setup);
        assert_eq!(em.get("angle"), 0xFFE0);
        let setup = "ROUTINES = { [0] = 1, 3, 1, 0 }; ANGLE = 0xFFE0; TARGET = 0xFF0";
        assert_eq!(run_em2d(0, &[], 1, setup).get("angle"), 0xFF0);
    }

    /// A cockroach for `enemy/29/enemy.lua`: `DRAWS` feeds `Rnd`, a clip
    /// ends on its `CLIP_LEN`th frame, the player stands at `PLAYER` with
    /// `Damage_cnt` `PL_DC` and the keys `KEYS` new this tick, an attach
    /// point is at `PART_AT` (or nowhere), and `Sca_ck_hit` finds `GROUND`
    /// under a ceiling at `CEILING`. The player's routines, hold and life
    /// go to `w.pl`. The turns, the trigonometry and `SquareRoot0` are the
    /// engine's own.
    const ROACH_STUB: &str = "
        local draws, drawn = DRAWS or {}, 0
        local clip_len = CLIP_LEN or 1000
        w = { r = ROUTINES or { [0] = 0, 0, 0, 0 }, vars = VARS or {},
            timers = { [0] = 0, 0, 0 }, be = 1, kind = 0, life = 0, angle = ANGLE or 0,
            cdir = { 0, 0 }, spd = { 0, 0, 0 }, pos = POS or { -15372, -3600, -25133 },
            clip = -1, frame = 0, hidden = 0, sounds = {}, faced = {},
            swarm = SWARM or { [0] = 0, 0, 0, 0 } }
        local player = PLAYER or { -7824, 0, -25140 }
        w.pl = { r = { [0] = 1, 0, 1, 0 }, dc = PL_DC or 0, life = 200 }
        local e = {}
        function e:pl_damage_cnt() return w.pl.dc end
        function e:pl_set_damage_cnt(v) w.pl.dc = v end
        function e:pl_set_routine(a, b, c, d) w.pl.r = { [0] = a, b, c, d } end
        function e:pl_hold() w.pl.held = true end
        function e:pl_set_sub1() w.pl.sub1 = true end
        function e:set_player_life(v) w.pl.life = v end
        function e:key_trg() return KEYS or 0 end
        function e:set_super_part(part) w.super_part = part end
        function e:sca_ck_hit(x, y, z, radius, mask)
            w.probe = { x, y, z, radius, mask }
            return GROUND or 0, CEILING or 0
        end
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

    const PLAYER_ROUTINES: &str = "return { w.pl.r[0], w.pl.r[1], w.pl.r[2], w.pl.r[3] }";

    /// Within 200 of the point it lands: attached, counted, and the
    /// player's struggle started from its beginning, held by this roach on
    /// its model's second bank.
    #[test]
    fn a_roach_lands_on_its_attach_point() {
        let landing = "POS = { -10000, -1000, -25200 }; PART_AT = { -10050, -900, -25150 }";
        let roach = roach_flying(landing);
        roach.run("w.be = 0x2001; tick()");
        assert_eq!(roach.eval::<[u8; 4]>(ROUTINES), [1, 3, 0, 0]);
        let (flags, on, be): (i64, i64, i64) = roach.eval("return w.vars[3], w.swarm[1], w.be");
        assert_eq!((flags, on, be), (3, 1, 1));
        assert_eq!(roach.eval::<[u8; 4]>(PLAYER_ROUTINES), [5, 0, 0, 0]);
        assert_eq!(
            roach.eval::<(bool, bool, i64)>("return w.pl.held, w.pl.sub1, w.pl.life"),
            (true, true, 200)
        );
        // A player whose `Damage_cnt` is up only gets the count.
        let roach = roach_flying(&format!("{landing}; PL_DC = 0x10"));
        roach.run("tick()");
        assert_eq!(roach.eval::<[u8; 4]>(ROUTINES), [1, 3, 0, 0]);
        assert_eq!(roach.eval::<[u8; 4]>(PLAYER_ROUTINES), [1, 0, 1, 0]);
        assert_eq!(roach.eval::<i64>("return w.swarm[1]"), 1);
        assert!(roach.eval::<Option<bool>>("return w.pl.held").is_none());
    }

    /// A landing once three or more have been on the player for more than
    /// 0xE1 ticks kills: routines 3, 0, 0, 0, life -1, `Damage_cnt` 0x80
    /// and the swarm's kill, which sends every roach without a point jumping.
    #[test]
    fn the_swarm_kills_the_player() {
        let roach = roach_flying(
            "POS = { -10000, -1000, -25200 }; PART_AT = { -10050, -900, -25150 };
             SWARM = { [0] = 0xE2, 3, 0x30, 0 }",
        );
        roach.run("tick()");
        assert_eq!(roach.eval::<[u8; 4]>(PLAYER_ROUTINES), [3, 0, 0, 0]);
        let (life, dc, killed): (i64, i64, i64) =
            roach.eval("return w.pl.life, w.pl.dc, w.swarm[3]");
        assert_eq!((life, dc, killed), (-1, 0x80, 1));
        // This one holds a point, so it stays on the player.
        assert_eq!(roach.eval::<[u8; 4]>(ROUTINES), [1, 3, 0, 0]);
        // One that holds none jumps.
        let free = spawn_roach(
            &[],
            "ROUTINES = { [0] = 1, 0, 0, 0 }; SWARM = { [0] = 0, 0, 0, 1 }",
        );
        free.run("tick()");
        assert_eq!(free.eval::<[u8; 4]>(ROUTINES), [1, 2, 0, 0]);
    }

    /// A roach on attach point 5 (part 1), routines 1, 3, 0, with `Timer0`
    /// drawn from `draw`.
    fn roach_on(draw: u8, setup: &str) -> Roach {
        spawn_roach(
            &[draw],
            &format!(
                "ROUTINES = {{ [0] = 1, 3, 0, 0 }}; VARS = {{ [3] = 3, [7] = 5 }};
                 SWARM = {{ [0] = 0, 1, 0x20, 0 }}; {setup}"
            ),
        )
    }

    /// `0x47c9b0`: the point's turn and part, `Timer0` from `(Rnd >> 4) +
    /// 0x5A`, and in the same tick the point's offset as `Pos`, clip 2 and
    /// the first step of `Timer0`.
    #[test]
    fn a_roach_on_the_player_takes_its_points_place() {
        let roach = roach_on(0x47, "");
        roach.run("w.be = 1; tick()");
        assert_eq!(roach.eval::<[u8; 4]>(ROUTINES), [1, 3, 1, 0]);
        let (be, part, timer): (i64, i64, i64) =
            roach.eval("return w.be, w.super_part, w.timers[0]");
        assert_eq!((be, part, timer), (0x300_0001, 1, (0x47 >> 4) + 0x5A - 1));
        let (cdir, angle, pos): (Vec<i64>, i64, Vec<i64>) =
            roach.eval("return w.cdir, w.angle, w.pos");
        assert_eq!(
            (cdir, angle, pos),
            (vec![3546, 2846], 2160, vec![134, -168, -194])
        );
        assert_eq!(
            roach.eval::<(i64, Vec<i64>)>("return w.clip, w.sounds"),
            (2, vec![3])
        );
        // It stays, back on its offset each tick.
        roach.run("w.pos = { 0, 0, 0 }; tick()");
        assert_eq!(roach.eval::<Vec<i64>>("return w.pos"), [134, -168, -194]);
        assert_eq!(
            roach.eval::<i64>("return w.timers[0]"),
            (0x47 >> 4) + 0x5A - 2
        );
        assert_eq!(roach.eval::<usize>("return drawn_count()"), 1);
    }

    /// A new key press takes 8 off `Timer0`, not 1. When it runs out the
    /// roach falls; the last one off frees the player.
    #[test]
    fn mashing_shakes_the_roaches_off() {
        let roach = roach_on(0, "KEYS = 0x80");
        roach.run("tick()");
        assert_eq!(roach.eval::<i64>("return w.timers[0]"), 0x5A - 8);
        roach.run("tick(10)");
        assert_eq!(roach.eval::<i64>("return w.timers[0]"), 0x5A - 8 * 11);
        assert_eq!(roach.eval::<[u8; 4]>(ROUTINES), [1, 3, 1, 0]);
        roach.run("tick()");
        assert_eq!(roach.eval::<[u8; 4]>(ROUTINES), [1, 4, 0, 0]);
        assert_eq!(roach.eval::<i64>("return w.swarm[1]"), 0);
        assert_eq!(roach.eval::<[u8; 4]>(PLAYER_ROUTINES), [1, 0, 0, 0]);
        // With another still on, the player stays held.
        let roach = roach_on(0, "KEYS = 0x80; SWARM = { [0] = 0, 2, 0x20, 0 }");
        roach.run("tick(12)");
        assert_eq!(roach.eval::<[u8; 4]>(ROUTINES), [1, 4, 0, 0]);
        assert_eq!(roach.eval::<i64>("return w.swarm[1]"), 1);
        assert_eq!(roach.eval::<[u8; 4]>(PLAYER_ROUTINES), [1, 0, 1, 0]);
        // After the swarm's kill none falls.
        let roach = roach_on(0, "KEYS = 0x80; SWARM = { [0] = 0, 1, 0x20, 1 }");
        roach.run("tick(20)");
        assert_eq!(roach.eval::<[u8; 4]>(ROUTINES), [1, 3, 1, 0]);
    }

    /// `0x689590` counts the ticks with more than two on the player; with
    /// fewer it goes back to 0.
    #[test]
    fn the_swarm_counts_the_ticks_three_are_on() {
        let roach = roach_on(0x80, "SWARM = { [0] = 7, 3, 0x20, 0 }");
        roach.run("tick(3)");
        assert_eq!(roach.eval::<i64>("return w.swarm[0]"), 10);
        roach.run("w.swarm[1] = 2; tick()");
        assert_eq!(roach.eval::<i64>("return w.swarm[0]"), 0);
    }

    /// `0x47caf0`: off the point's place, away from the player and up, then
    /// falling faster each tick with its surface turning away from 0xC00,
    /// down to the ground and back onto the pipe, its point freed.
    #[test]
    fn a_roach_falls_off_the_player() {
        let roach = spawn_roach(
            &[],
            "ROUTINES = { [0] = 1, 4, 0, 0 }; VARS = { [3] = 3, [7] = 5 };
             SWARM = { [0] = 0, 0, 0x20, 0 }; PART_AT = { -7900, -1900, -25140 }; GROUND = 0",
        );
        roach.run("w.be = 0x3000001; w.super_part = 1; w.hidden = 0; tick()");
        assert_eq!(roach.eval::<[u8; 4]>(ROUTINES), [1, 4, 1, 0]);
        let (be, part, angle): (i64, Option<i64>, i64) =
            roach.eval("return w.be, w.super_part, w.angle");
        assert_eq!((be, part, angle), (0x200_0001, None, 0));
        // 0x32 away from the player at (-7824, -25140), straight along -x.
        let away = player2::rotate(direction_ck(-7824, -25140, -7900, -25140), 0x32, 0);
        let (x, y, z): (i32, i32, i32) = roach.eval("return w.pos[1], w.pos[2], w.pos[3]");
        assert_eq!((x, z), (-7900 + away.0, -25140 + away.1));
        assert_eq!(y, -1900 + 0x14 + 2);
        assert_eq!(
            roach.eval::<(i64, Vec<i64>)>("return w.timers[0], w.super"),
            (2, vec![0, 0, -0x10])
        );
        roach.run("tick()");
        assert_eq!(roach.eval::<i32>("return w.pos[2]"), -1900 + 0x16 + 0x1A);
        assert_eq!(roach.eval::<Vec<i64>>("return w.super"), [0, 0, -0x20]);
        roach.run("tick(40)");
        assert_eq!(roach.eval::<[u8; 4]>(ROUTINES), [1, 0, 0, 0]);
        let (hidden, taken, flags, sounds): (i64, i64, i64, Vec<i64>) =
            roach.eval("return w.hidden, w.swarm[2], w.vars[3], w.sounds");
        assert_eq!((hidden, taken, flags & 1, sounds), (0xC, 0, 0, vec![1]));
        assert_eq!(
            roach.eval::<(i64, i64)>("return w.probe[4], w.probe[5]"),
            (1, 0x400)
        );
    }

    /// The sub-machine gun's (0xF) and the flamethrower's (0x10) hits on the zombies, lib/zombie.lua.
    mod smg_flame_zombie {
        // SMG_FLAME_ZOMBIE_TESTS

        /// A body for lib/zombie.lua: `w.rnds` are `Rnd`'s draws (0 after),
        /// `EFFECTS` its `Esp_call`s (kind, row, size, dir, part, x, y, z),
        /// `FLAMES` `FUN_00456780`'s fires (size, part, x, y, z). It is hit
        /// by the sub-machine gun on the head (`Routine_1` 0xF, `Damage_no`
        /// 1), faces 0x100 with the player at 0x300 and 1000 away, and its
        /// moan is still waiting.
        pub(super) const STUB: &str = "
            local w = { r = { [0] = 2, 0xF, 0, 0 }, vars = { [1] = 5, [4] = 0x20 }, timers = {},
                kind = 0, flags = 0, no = 1, hidden = 0, angle = 0x100, dir = 0x300,
                dist = 1000, turn = 0, rnds = {}, drawn = 0, burns = 0, push = -1, toward = -1,
                clip = { -1, -1, -1 }, spd = { 0, 7, 0 } }
            EFFECTS, FLAMES = {}, {}
            local e = setmetatable({}, { __index = function() return function() return 0 end end })
            function e:routine(i) return w.r[i] end
            function e:set_routine(i, v) w.r[i] = v end
            function e:routines(a, b, c, d) w.r = { [0] = a, b, c, d } end
            function e:var(i) return w.vars[i] or 0 end
            function e:set_var(i, v) w.vars[i] = v end
            function e:timer(i) return w.timers[i] or 0 end
            function e:set_timer(i, v) w.timers[i] = v end
            function e:kind() return w.kind end
            function e:set_kind(v) w.kind = v end
            function e:flags() return w.flags end
            function e:set_flags(v) w.flags = v end
            function e:damage_no() return w.no end
            function e:hidden_parts() return w.hidden end
            function e:angle() return w.angle end
            function e:set_angle(v) w.angle = v end
            function e:direction_to_player() return w.dir end
            function e:distance() return w.dist end
            function e:turn_to_player() return w.turn end
            function e:partner() return false end
            function e:joint_bank() return false end
            function e:move_no() return 9 end
            function e:clip(n, c, h) w.clip = { n, c, h } end
            function e:speed() return w.spd[1], w.spd[2], w.spd[3] end
            function e:set_speed(x, y, z) w.spd = { x, y, z } end
            function e:add_speed_xz_at(d) w.push = d end
            function e:goto_player(r) w.toward = r end
            function e:part_offset() return 0, 0, 0 end
            function e:burn_parts() w.burns = w.burns + 1 end
            function e:rnd()
                w.drawn = w.drawn + 1
                return w.rnds[w.drawn] or 0
            end
            function e:effect(kind, row, size, dir, part, x, y, z)
                EFFECTS[#EFFECTS + 1] = { kind, row, size, dir, part, x, y, z }
            end
            function e:light_flame(size, part, x, y, z)
                FLAMES[#FLAMES + 1] = { size, part, x, y, z }
            end
        ";

        /// One tick of the script on [`STUB`] after `setup`, then `result`'s
        /// values.
        pub(super) fn tick(setup: &str, result: &str) -> Vec<i64> {
            let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
            let library = include_str!("../../games/re2/lib/zombie.lua");
            lua.load(format!(
                "{STUB}\n{setup}\nlocal zombie = (function() {library} end)()
                 zombie.run(e, {{}})
                 R = {{ {result} }}"
            ))
            .exec()
            .unwrap();
            lua.globals().get("R").unwrap()
        }

        /// The effects of [`tick`]: `EFFECTS` and `FLAMES`.
        pub(super) fn effects(setup: &str) -> (Vec<Vec<i64>>, Vec<Vec<i64>>) {
            let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
            let library = include_str!("../../games/re2/lib/zombie.lua");
            lua.load(format!(
                "{STUB}\n{setup}\nlocal zombie = (function() {library} end)()
                 zombie.run(e, {{}})"
            ))
            .exec()
            .unwrap();
            let globals = lua.globals();
            (
                globals.get("EFFECTS").unwrap(),
                globals.get("FLAMES").unwrap(),
            )
        }

        /// `0x5270f0`, the sub-machine gun's row of `Em10_Damage`'s table, is
        /// `Em10Dm08`: clip `0x458260`'s `[0x1d, 0x1c, 0x1e, 0x1c, 0x1c, 0x1d,
        /// 0x1c, 0x1e][Rnd & 7]` (hokan 7), a head shot's blood at `(0x100 -
        /// 2r, 0x138 - 4r, 0x100 - 2r)` on part 0, size `r * 8 + 7000` and row
        /// `Rnd & 1`, toward the player; then `Spd_x = (Rnd & 0x3f) + 0x14`,
        /// `Spd_z = 0x40 - (Rnd & 0x7f)` pushed along the player's way plus
        /// 0x800, `Cdir_y += 0x40 - (Rnd & 0x7f)` and `Goto00` at 0x10. No
        /// stagger: `At_sce_no` stays.
        #[test]
        fn the_smg_jerks_a_zombie_back_by_em10dm08() {
            let setup = "w.rnds = { 5, 10, 20, 30, 40, 1, 0x50, 0x60, 0x70 }";
            let r = tick(
                setup,
                "w.clip[1], w.clip[2], w.clip[3], w.spd[1], w.spd[2], w.spd[3], w.push, w.angle, \
                 w.toward, w.r[2], w.timers[0], w.vars[4], w.drawn",
            );
            assert_eq!(
                r,
                [
                    0x1D, 0, 7, 0x24, 7, -32, 0xA00, 0xD0, 0x10, 1, 0x300, 0x20, 9
                ]
            );
            let (blood, _) = effects(setup);
            assert_eq!(blood, [vec![0, 1, 7320, 0x300, 0, 236, 232, 196]]);
        }

        /// `Em10Dm08` on the body (`Damage_no` 0): `(0x7f - r, 0x39f - r,
        /// 0x7f - r)` on part 1, size `r * 8 + 0x17d0`.
        #[test]
        fn the_smg_bleeds_a_body_shot_from_part_1() {
            let (blood, _) = effects("w.no = 0; w.rnds = { 0, 10, 20, 30, 40, 0 }");
            assert_eq!(blood, [vec![0, 0, 6416, 0x300, 1, 117, 907, 97]]);
        }

        /// `Em10Dm08`'s step 2: it walks (`1,1`) or, on `Rnd & 1`, wanders
        /// (`1,2`), and facing the player within 0x200, under 5000 away and
        /// free, one in eight grabs (`1,0xB`).
        #[test]
        fn after_the_smg_it_walks_on_or_grabs() {
            let step2 = "w.r = { [0] = 2, 0xF, 2, 0 }; w.rnds = { 1, 9 }";
            assert_eq!(tick(step2, "w.r[0], w.r[1], w.drawn"), [1, 2, 2]);
            let grab = "w.r = { [0] = 2, 0xF, 2, 0 }; w.rnds = { 0, 0 }";
            assert_eq!(tick(grab, "w.r[0], w.r[1], w.drawn"), [1, 0xB, 2]);
            let far = "w.r = { [0] = 2, 0xF, 2, 0 }; w.dist = 5000";
            assert_eq!(tick(far, "w.r[0], w.r[1], w.drawn"), [1, 1, 1]);
        }

        /// `Em10Dm00` with `Routine_1` 0x10: no blood, `Timer3` 2 in place of
        /// 0x18, and one more at `+0x23a`; `At_sce_no` loses 0
        /// (`0x5271c7 + 0x10`).
        #[test]
        fn a_flame_hit_draws_no_blood_and_leans_less() {
            let setup = "w.r[1] = 0x10; w.vars[12] = 3";
            let r = tick(
                setup,
                "w.timers[3], w.vars[12], w.vars[4], w.kind, w.flags, w.burns",
            );
            assert_eq!(r, [2, 4, 0x20, 0, 0, 0]);
            assert_eq!(effects(setup), (vec![], vec![]));
        }

        /// Once `+0x23a` is over 8, `Em10Dm00`'s next flame hit calls
        /// `FUN_00456780`: `Type |= 0x80`, the fires on parts 0, 3 and 6 (or
        /// 0, 8 and 3), each drawn part's only, and no darker parts for
        /// 0x10; and `+0x21a |= 0x800`, so it burns as it walks.
        #[test]
        fn the_tenth_flame_hit_sets_a_zombie_alight() {
            let lit = "w.r[1] = 0x10; w.vars[12] = 9";
            let r = tick(lit, "w.kind, w.flags, w.burns, w.vars[12]");
            assert_eq!(r, [0x80, 0x800, 0, 10]);
            let (_, flames) = effects(lit);
            assert_eq!(
                flames,
                [
                    vec![10000, 0, 0, 0, 0],
                    vec![5000, 3, 0, 200, 0],
                    vec![5096, 6, 0, -200, 0],
                ]
            );
            let (_, flames) =
                effects("w.r[1] = 0x10; w.vars[12] = 9; w.rnds = { 1 }; w.hidden = 1 << 8");
            assert_eq!(
                flames,
                [vec![10000, 0, 0, 200, 0], vec![5096, 3, 0, 200, 0]]
            );
            let ninth = tick(
                "w.r[1] = 0x10; w.vars[12] = 8",
                "w.kind, w.flags, w.vars[12]",
            );
            assert_eq!(ninth, [0, 0, 9]);
            let burnt = tick("w.r[1] = 0x10; w.vars[12] = 9; w.kind = 0x80", "w.flags");
            assert_eq!(burnt, [0]);
        }

        /// `FUN_00457d10`, a hit while crawling, counts and lights as
        /// `Em10Dm00` does.
        #[test]
        fn a_crawler_is_set_alight_too() {
            let r = tick(
                "w.kind = 1; w.r[1] = 0x10; w.vars[12] = 9",
                "w.kind, w.flags, w.vars[12], w.clip[1]",
            );
            assert_eq!(r, [0x81, 0x800, 10, 6]);
        }

        /// `FUN_00457e60`, a hit while getting up: `Timer3` (`+0x16b`) is 2
        /// for 0x10, 0xF and 0x12, where it's 0x18, and 0x10 counts and
        /// lights.
        #[test]
        fn a_rising_zombie_leans_less_from_the_smg_and_the_flamethrower() {
            let rising = |weapon: &str| {
                tick(
                    &format!("w.flags = 0x10; w.r[1] = {weapon}; w.vars[12] = 9"),
                    "w.timers[3], w.vars[12], w.kind",
                )
            };
            assert_eq!(rising("0xF"), [2, 9, 0]);
            assert_eq!(rising("0x10"), [2, 10, 0x80]);
            assert_eq!(rising("2"), [0x18, 9, 0]);
        }

        /// `0x5273fc`, the flamethrower's row of `funcs_458795`, is
        /// `FUN_004587b0`, which with `Routine_1` 0x10 calls `FUN_00456780`
        /// on a body not yet burnt; the sub-machine gun's (`0x5273d8`) is the
        /// same death with no fire.
        #[test]
        fn a_flame_kills_a_zombie_alight() {
            let die = |setup: &str| {
                let (blood, flames) = effects(&format!("w.r = {{ [0] = 3, 0x10, 0, 0 }}; {setup}"));
                (blood.len(), flames.len())
            };
            assert_eq!(die(""), (1, 3));
            assert_eq!(die("w.kind = 0x80"), (1, 0));
            assert_eq!(die("w.r[1] = 0xF"), (1, 0));
            let r = tick(
                "w.r = { [0] = 3, 0x10, 0, 0 }",
                "w.kind, w.flags & 0x800, w.burns",
            );
            assert_eq!(r, [0x80, 0, 0]);
        }
    }

    /// The spark shot's (0xE) hits on the zombies, lib/zombie.lua.
    mod spark_zombie {
        use super::smg_flame_zombie::{effects, tick};

        /// `FUN_00456b00` on all-zero draws, facing 0x100: kind 6 at `(0x80,
        /// 0x80, 0x80)` on part 0 four times, kind 4 row 0xC on part 0, kind 6
        /// on parts 9 and 3.
        fn sparks() -> Vec<Vec<i64>> {
            let at = |kind, row, size, part, offset| {
                vec![kind, row, size, 0x100, part, offset, offset, offset]
            };
            vec![
                at(6, 0, 0x13E8, 0, 0x80),
                at(6, 0, 0x1000, 0, 0x80),
                at(6, 0, 0x1000, 0, 0x80),
                at(6, 0, 0xC18, 0, 0x80),
                at(4, 0xC, 0x1000, 0, 0),
                at(6, 0, 0xC18, 9, 0x80),
                at(6, 0, 0xC18, 3, 0x80),
            ]
        }

        /// `0x5270cc`, the spark shot's row of `Em10_Damage`'s table, is
        /// `Em10Dm00`, whose first tick calls `FUN_00456b00` after its blood
        /// and `Goto00`: for each of four sparks `0x80 - Rnd` on x, y and z,
        /// then `Rnd * 8` over 0x13e8, 0x1000, 0x1000 and 0xc18 (`| 0x6000000`)
        /// on part 0; `Rnd * 8 + 0x1000 | 0x40c0000` on part 0 with no vector;
        /// and `Rnd * 8 + 0xc18 | 0x6000000` on parts 9 and 3 at the last
        /// vector. No stagger (`0x5271c7 + 0xe` is 0).
        #[test]
        fn a_spark_hit_sparks_on_a_zombie_by_em10dm00() {
            let setup = "w.r[1] = 0xE; w.rnds = { 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, \
                         15, 16, 17, 18, 19 }";
            assert_eq!(
                tick(setup, "w.r[2], w.vars[4], w.timers[3], w.drawn"),
                [1, 0x20, 0x18, 19]
            );
            let (fx, flames) = effects(setup);
            assert!(flames.is_empty());
            assert_eq!(
                fx,
                [
                    vec![0, 0, 0x17D0, 0x100, 0, -100, 300, 0],
                    vec![6, 0, 0x13E8 + 32, 0x100, 0, 127, 126, 125],
                    vec![6, 0, 0x1000 + 64, 0x100, 0, 123, 122, 121],
                    vec![6, 0, 0x1000 + 96, 0x100, 0, 119, 118, 117],
                    vec![6, 0, 0xC18 + 128, 0x100, 0, 115, 114, 113],
                    vec![4, 0xC, 0x1000 + 136, 0x100, 0, 0, 0, 0],
                    vec![6, 0, 0xC18 + 144, 0x100, 9, 115, 114, 113],
                    vec![6, 0, 0xC18 + 152, 0x100, 3, 115, 114, 113],
                ]
            );
        }

        /// `Em10_Die`'s row for the rocket launcher (0x11) on `Damage_no` 1
        /// is `FUN_00458d40`, which blows the head off behind the body
        /// (`Be_flg |= 0x4a`: at 400 and up at 100, `Parts_bomb` slowing it
        /// by 50 a tick) rather than hiding it, before the blood on it.
        #[test]
        fn a_torn_zombies_head_flies_off_behind_it() {
            let setup = "w.r = { [0] = 3, 0x11, 0, 0 }; w.no = 1; BLOWN = {}; \
                         function e:blow_part(...) BLOWN = { ... } end; \
                         function e:hide_part(part) HIDDEN = part end";
            assert_eq!(
                tick(
                    setup,
                    "BLOWN[1], BLOWN[2], BLOWN[3], BLOWN[4], BLOWN[5], BLOWN[6], BLOWN[7], \
                     HIDDEN or -1"
                ),
                [8, 0x900, 400, -100, 10, 0x101030, 50, -1]
            );
            let (fx, _) = effects(setup);
            let blood = fx
                .iter()
                .find(|call| call[..3] == [0, 0, 3000])
                .expect("the blood");
            assert_eq!(blood[4..], [8, 0, 0, 0], "on the head");
        }

        /// `Em10Dm00` sparks again as its step 1 ends (step 2, `Timer0`
        /// 0x10), and at the top of step 3, before its choices' draws.
        #[test]
        fn em10dm00_sparks_again_at_steps_2_and_3() {
            let step1 = "w.r = { [0] = 2, 0xE, 1, 0 }; w.timers[0] = 3";
            assert_eq!(tick(step1, "w.r[2], w.timers[0], w.drawn"), [2, 0x10, 19]);
            assert_eq!(effects(step1).0, sparks());
            let early = "w.r = { [0] = 2, 0xE, 1, 0 }; w.timers[0] = 2";
            assert_eq!(tick(early, "w.r[2], w.drawn"), [1, 0]);
            // Then `Rnd & 0xf` (1: no grab) and `Rnd & 1` (1: wander).
            let mut draws = vec!["0"; 19];
            draws.extend(["1", "1"]);
            let step3 = format!(
                "w.r = {{ [0] = 2, 0xE, 3, 0 }}; w.rnds = {{ {} }}",
                draws.join(", ")
            );
            assert_eq!(
                tick(&step3, "w.r[0], w.r[1], w.r[2], w.drawn"),
                [1, 2, 0, 21]
            );
            assert_eq!(effects(&step3).0, sparks());
        }

        /// `FUN_00457d10` (crawling) and `FUN_00457e60` (getting up) call
        /// `FUN_00456b00` at the end of their first tick, after the blood.
        #[test]
        fn a_crawling_or_rising_zombie_sparks_too() {
            for (setup, drawn) in [("w.kind = 1", 20), ("w.flags = 0x10", 19)] {
                let setup = format!("w.r[1] = 0xE; {setup}");
                assert_eq!(tick(&setup, "w.drawn"), [drawn], "{setup}");
                let (fx, _) = effects(&setup);
                assert_eq!(fx.len(), 8, "{setup}");
                assert_eq!(fx[0][0], 0, "{setup}");
                assert_eq!(fx[1..], sparks(), "{setup}");
            }
        }

        /// `funcs_458795`'s row for 0xE is `FUN_004587b0`, which sparks after
        /// its blood and last moan; so do `FUN_00458bf0` (crawling),
        /// `FUN_00459b60` (getting up) and `Em10_Die`'s lying body, which then
        /// goes to `7,9`.
        #[test]
        fn a_spark_kill_sparks_on_every_death() {
            for (setup, drawn) in [
                ("", 21),
                ("w.kind = 1", 20),
                ("w.flags = 0x10", 20),
                ("w.flags = 2", 20),
            ] {
                let setup = format!("w.r = {{ [0] = 3, 0xE, 0, 0 }}; {setup}");
                assert_eq!(tick(&setup, "w.drawn"), [drawn], "{setup}");
                let (fx, _) = effects(&setup);
                assert_eq!(fx.len(), 8, "{setup}");
                assert_eq!(fx[1..], sparks(), "{setup}");
            }
            assert_eq!(
                tick(
                    "w.r = { [0] = 3, 0xE, 0, 0 }; w.flags = 2",
                    "w.r[0], w.r[1]"
                ),
                [7, 9]
            );
        }
    }

    /// The sub-machine gun's (0xF) and the flamethrower's (0x10) hits on the lickers and the dog.
    mod smg_flame_licker_dog {
        // SMG_FLAME_LICKER_DOG_TESTS

        /// The first tick of `Em22_Damage` (or `Em_24`'s `FUN_0046dd40`) on
        /// a licker on the floor, hit by `weapon`: `(Damage_no, the draws,
        /// the effects' kind, row and size, +0x233 and +0x23c's callback)`.
        fn licker_hit(script: &str, weapon: u8) -> (i64, i64, Vec<Vec<i64>>, i64, i64) {
            let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
            let licker: mlua::Table = lua
                .load(include_str!("../../games/re2/lib/licker.lua"))
                .eval()
                .unwrap();
            lua.globals().set("LICKER", licker).unwrap();
            lua.globals().set("WEAPON", weapon).unwrap();
            lua.load(format!(
                "local w = {{ r = {{ [0] = 2, WEAPON, 0, 0 }}, vars = {{}}, be = 0, flags = 0,
                     damage_no = 0xFF, drawn = 0 }}
                 E = {{}}
                 local e = setmetatable({{}}, {{ __index = function() return function() return 0 end end }})
                 function e:routine(i) return w.r[i] end
                 function e:set_routine(i, v) w.r[i] = v end
                 function e:routines(a, b, c, d) w.r = {{ [0] = a, b, c, d }} end
                 function e:var(i) return w.vars[i] or 0 end
                 function e:set_var(i, v) w.vars[i] = v end
                 function e:be_flg() return w.be end
                 function e:set_be_flg(v) w.be = v end
                 function e:flags() return w.flags end
                 function e:set_flags(v) w.flags = v end
                 function e:damage_no() return w.damage_no end
                 function e:set_damage_no(v) w.damage_no = v end
                 function e:rnd() w.drawn = w.drawn + 1 return 5 end
                 function e:effect(kind, row, size) E[#E + 1] = {{ kind, row, size }} end
                 function e:pos() return 0, 0, 0 end
                 function e:speed() return 0, 0, 0 end
                 function e:rotate() return 0, 0 end
                 function e:joint() return false end
                 function e:cdir_player() return false end
                 function e:tongue_owner() return -1 end
                 local function require() return LICKER end
                 {script}
                 update(e)
                 R = {{ w.damage_no, w.drawn, w.vars[6] or 0, w.vars[7] or 0 }}"
            ))
            .exec()
            .unwrap();
            let r: Vec<i64> = lua.globals().get("R").unwrap();
            (r[0], r[1], lua.globals().get("E").unwrap(), r[2], r[3])
        }

        /// `FUN_00467040` (and `Em_24`'s `FUN_0046ddc0`) by the bytes of its
        /// jump table (`0x467154`, `0x46de7c`): the sub-machine gun knocks it
        /// back (`Damage_no` 1) with no draw, the flames barely hurt it (0).
        /// `DAT_00527c90`'s (and `DAT_00528360`'s) rows then bleed once for
        /// 0xF (kind 0, row 6, 0x2000) and six times for 0x10 (kind 5, row
        /// 4, 0x1800), one draw each.
        #[test]
        fn the_smg_knocks_a_licker_back_and_the_flames_barely_hurt_it() {
            for script in [
                include_str!("../../games/re2/enemy/22/enemy.lua"),
                include_str!("../../games/re2/enemy/24/enemy.lua"),
            ] {
                let (damage_no, drawn, effects, ..) = licker_hit(script, crate::player2::SMG);
                assert_eq!((damage_no, drawn), (1, 1));
                assert_eq!(effects, [vec![0, 6, 0x2000]]);
                let (damage_no, drawn, effects, ..) =
                    licker_hit(script, crate::player2::FLAMETHROWER);
                assert_eq!((damage_no, drawn), (0, 6));
                assert_eq!(effects, vec![vec![5, 4, 0x1800]; 6]);
            }
        }

        /// The spark shot's hit (`FUN_004671f0`'s 0xE) calls `FUN_00467370`
        /// itself, which sets `+0x233` going and itself as `+0x23c`'s
        /// callback, and `Em_22` (and `Em_24`) runs that after the routine
        /// the same tick: `DAT_00527ba8`'s record 0, which draws nothing, is
        /// the hit tick's, so `+0x233` is 2.
        #[test]
        fn a_spark_hit_sets_a_lickers_sparks_going_at_once() {
            for script in [
                include_str!("../../games/re2/enemy/22/enemy.lua"),
                include_str!("../../games/re2/enemy/24/enemy.lua"),
            ] {
                let (_, drawn, effects, count, after) = licker_hit(script, 0xE);
                assert_eq!((count, after), (2, 2));
                let sparks = effects.iter().filter(|effect| effect[0] == 6).count();
                assert_eq!(sparks, 0, "record 0 has none");
                assert_eq!(
                    drawn,
                    effects.len() as i64,
                    "a draw a blood, none for record 0"
                );
            }
        }

        /// One tick of the dog with `Routine_0` `zero`, `Routine_1`
        /// `weapon`, `Routine_2` `two`, `Damage_no` `damage_no`, standing
        /// tall (`Be_flg` 0x8000000) on the floor, after `setup`: `(Routine_1,
        /// Routine_2, Spd_x, Spd_y, +0x219, +0x223, Damage_cnt, the draws,
        /// the effects)`.
        fn dog_hit(zero: u8, weapon: u8, two: u8, damage_no: u8, setup: &str) -> Vec<i64> {
            let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
            let script = include_str!("../../games/re2/enemy/20/enemy.lua");
            lua.load(format!(
                "local w = {{ r = {{ [0] = {zero}, {weapon}, {two}, 0 }}, vars = {{}}, timers = {{}},
                     be = 0x8000000, spd = {{ 0, 0, 0 }}, cnt = 0, drawn = 0, effects = 0 }}
                 local e = setmetatable({{}}, {{ __index = function() return function() return 0 end end }})
                 function e:routine(i) return w.r[i] end
                 function e:set_routine(i, v) w.r[i] = v end
                 function e:routines(a, b, c, d) w.r = {{ [0] = a, b, c, d }} end
                 function e:var(i) return w.vars[i] or 0 end
                 function e:set_var(i, v) w.vars[i] = v end
                 function e:timer(i) return w.timers[i] or 0 end
                 function e:set_timer(i, v) w.timers[i] = v end
                 function e:be_flg() return w.be end
                 function e:set_be_flg(v) w.be = v end
                 function e:speed() return w.spd[1], w.spd[2], w.spd[3] end
                 function e:set_speed(x, y, z) w.spd = {{ x, y, z }} end
                 function e:speed_x() return w.spd[1] end
                 function e:set_speed_x(v) w.spd[1] = v end
                 function e:damage_no() return {damage_no} end
                 function e:damage_cnt() return w.cnt end
                 function e:set_damage_cnt(v) w.cnt = v end
                 function e:rnd() w.drawn = w.drawn + 1 return 1 end
                 function e:effect() w.effects = w.effects + 1 end
                 function e:body_box() return 0, 0, 0, 0 end
                 function e:joint() return false end
                 function e:pos() return 0, 0, 0 end
                 {setup}
                 {script}
                 update(e)
                 R = {{ w.r[1], w.r[2], w.spd[1], w.spd[2], w.vars[4] or 0, w.vars[10] or 0, w.cnt,
                     w.drawn, w.effects }}"
            ))
            .exec()
            .unwrap();
            lua.globals().get("R").unwrap()
        }

        /// `Em20_Hurt`'s table (`0x52766c`) sends 0xF to the blasts' case
        /// (0x45ED50): thrown up (`Spd_y` -140, `Spd_x` 0xF0, which
        /// `hurt_start` halves as it's in the air) into `Routine_2` 1, with
        /// one blood.
        #[test]
        fn the_smg_throws_a_dog_into_the_air() {
            let r = dog_hit(2, crate::player2::SMG, 0, 1, "");
            assert_eq!(r, [0xF, 1, 0x78, -140, 1, 1, 0, 2, 1]);
        }

        /// 0x10 is 0xA's case (0x45EBD0): `4 - Damage_no / 3` bloods, though
        /// each takes one off the count it's checked against (`0x45f9a0`), so
        /// two; `Damage_cnt`'s 0x80 held, and on as `Routine_1` 1 to
        /// `hurt_start`'s own blood.
        #[test]
        fn the_flames_bleed_a_dog_and_hold_the_hit() {
            let r = dog_hit(2, crate::player2::FLAMETHROWER, 0, 2, "");
            assert_eq!((r[0], r[1], r[6]), (1, 2, 0x80));
            assert_eq!((r[7], r[8]), (6, 3));
        }

        /// Hit again (`+0x223` negative) in `Routine_2` 4, `0x5276e0`'s
        /// fifth cell is past its four: `0x5276f0`'s first, 0x45ED70, the
        /// blast's start, which throws it up and runs `hurt_start`; that
        /// counts the SMG's three more bloods (`FUN_0045e900`) and, in the
        /// air, goes to `Routine_2` 1. The three bloods of `0x45e8a0` follow.
        #[test]
        fn a_dog_hit_again_in_routine_2_4_starts_as_a_blast() {
            let r = dog_hit(2, crate::player2::SMG, 4, 1, "w.vars[10] = 0x81");
            assert_eq!(r, [0xF, 1, 0xF0, -140, 1, 0x81, 0, 8, 3]);
        }

        /// `FUN_0045ee20`'s table for `Routine_2` 0 (`0x527754`): 0xF is the
        /// plain death (0x45EE70, `hurt_start`'s one blood), 0x10 is 0xA's
        /// `FUN_0045f2a0`, whose six bloods 0x10 has whatever `Damage_no`.
        #[test]
        fn a_dog_killed_by_the_flames_bleeds_six_times() {
            let effects = |weapon: u8| dog_hit(3, weapon, 0, 4, "")[8];
            assert_eq!(effects(crate::player2::SMG), 1);
            assert_eq!(effects(crate::player2::FLAMETHROWER), 7);
            assert_eq!(effects(0xA), 1);
        }
    }

    /// The sub-machine gun's (0xF) and the flamethrower's (0x10) hits on the crow, the alligator and the spiders, embryos and cockroaches.
    mod smg_flame_small {
        // SMG_FLAME_SMALL_TESTS
        use super::*;
        use crate::player2::{FLAMETHROWER, SMG};

        /// What one tick left: each `Esp_call` as `{ kind, row, size, part }`
        /// (part -1 for `call_effect`'s, at a point), the draws, the routines
        /// and the clips.
        pub(super) struct Tick {
            pub(super) effects: Vec<Vec<i64>>,
            pub(super) drawn: i64,
            pub(super) routines: Vec<i64>,
            pub(super) clips: Vec<i64>,
        }

        /// Records the effects and the clips over whatever stub ran before.
        const RECORD: &str = "
            CALLED, CLIPPED = {}, {}
            function e:effect(kind, row, size, dir, part)
                CALLED[#CALLED + 1] = { kind, row, size, part }
            end
            function e:call_effect(kind, row, size)
                CALLED[#CALLED + 1] = { kind, row, size, -1 }
            end
            local clip = e.clip
            function e:clip(n, c, h)
                CLIPPED[#CLIPPED + 1] = n
                if clip then clip(self, n, c, h) end
            end
        ";

        fn list<T: mlua::FromLua>(table: &mlua::Table, name: &str) -> Vec<T> {
            table
                .get::<mlua::Table>(name)
                .unwrap()
                .sequence_values::<T>()
                .map(Result::unwrap)
                .collect()
        }

        /// One `update` of `script` on `stub` (which defines `e`, and the
        /// local `drawn` when `counted`), with `setup`'s globals and `DRAWS`
        /// (zeros after).
        pub(super) fn tick(script: &str, stub: &str, setup: &str, draws: &[u8]) -> Tick {
            let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
            lua.load(setup).exec().unwrap();
            let mut draws = draws.to_vec();
            draws.resize(256, 0);
            lua.globals().set("DRAWS", draws).unwrap();
            lua.globals()
                .set(
                    "math_hypot",
                    lua.create_function(|_, (x, z): (i32, i32)| Ok(square_root0(x * x + z * z)))
                        .unwrap(),
                )
                .unwrap();
            let table = lua
                .load(format!(
                    "{stub}
                     {RECORD}
                     {script}
                     update(e)
                     local r = w.r
                     return {{ effects = CALLED, clips = CLIPPED, drawn = drawn,
                         routines = {{ r[0], r[1], r[2], r[3] }} }}"
                ))
                .eval::<mlua::Table>()
                .unwrap();
            Tick {
                effects: list(&table, "effects"),
                drawn: table.get("drawn").unwrap(),
                routines: list(&table, "routines"),
                clips: list(&table, "clips"),
            }
        }

        /// A work whose every other method returns zeros, for the crow and
        /// the alligator: `ROUTINES`, `VARS`, and `Rnd` from `DRAWS`.
        pub(super) const ZEROS: &str = "
            local w = { r = ROUTINES, vars = VARS or {} }
            local drawn = 0
            local e = setmetatable({}, { __index = function()
                return function() return 0, 0, 0, 0 end
            end })
            function e:routine(i) return w.r[i] end
            function e:set_routine(i, v) w.r[i] = v end
            function e:routines(a, b, c, d) w.r = { [0] = a, b, c, d } end
            function e:var(i) return w.vars[i] or 0 end
            function e:set_var(i, v) w.vars[i] = v end
            function e:rnd() drawn = drawn + 1; return DRAWS[drawn] end
        ";

        const CROW: &str = include_str!("../../games/re2/enemy/21/enemy.lua");

        /// The crow's hit by `weapon`: its clips and its effects.
        fn crow_hit(weapon: u8) -> (Vec<i64>, Vec<Vec<i64>>) {
            let setup = format!("ROUTINES = {{ [0] = 2, {weapon}, 0, 0 }}");
            let hit = tick(CROW, ZEROS, &setup, &[]);
            (hit.clips, hit.effects)
        }

        /// `DAT_00527978[0xF]` and `[0x10]` are `FUN_00461a00`, the
        /// handguns' fall to the floor (clip 9), not `FUN_00461d00`'s burst.
        #[test]
        fn the_smg_and_the_flamethrower_knock_a_crow_down() {
            let handgun = crow_hit(2);
            assert_eq!(handgun.0[0], 9);
            assert_eq!(crow_hit(SMG), handgun);
            assert_eq!(crow_hit(FLAMETHROWER), handgun);
            assert_ne!(crow_hit(9).0.first(), Some(&9), "weapon 9 bursts it");
        }

        pub(super) const ALLIGATOR: &str = include_str!("../../games/re2/enemy/23/enemy.lua");

        /// `FUN_0046af30`'s jump table (`0x46b02c`): 0xF one blood of row 6
        /// (`FUN_0046b080`, two draws); 0x10 the blasts' four of effect 5,
        /// row 4 about the head (`FUN_0046b540`, eight draws).
        #[test]
        fn the_alligator_bleeds_once_for_the_smg_and_burns_for_the_flamethrower() {
            // `+0x22e` still counting: no flinch, and no draw for one.
            let hit = |weapon: u8| {
                let setup =
                    format!("ROUTINES = {{ [0] = 2, {weapon}, 0, 0 }}; VARS = {{ [12] = 9 }}");
                tick(ALLIGATOR, ZEROS, &setup, &[])
            };
            let smg = hit(SMG);
            assert_eq!(smg.effects, [[0, 6, 0x2800, -1]]);
            assert_eq!(smg.drawn, 2);
            let flame = hit(FLAMETHROWER);
            assert_eq!(flame.effects.len(), 4);
            assert_eq!(flame.effects[0], [5, 4, 0x2000, -1]);
            assert_eq!(flame.effects[3], [5, 4, 0x2800, -1]);
            assert_eq!(flame.drawn, 8);
        }

        const SPIDER: &str = include_str!("../../games/re2/enemy/25/enemy.lua");

        /// The giant spider on the floor, `ROUTINES` and `VARS` from `setup`,
        /// the legs' parts shown unless `HIDDEN` has their bits.
        pub(super) fn spider(setup: &str, draws: &[u8]) -> Tick {
            let stub = format!(
                "{SPIDER_STUB}
                 function e:hidden_parts() return HIDDEN or 0 end"
            );
            tick(SPIDER, &stub, &format!("KIND = 0; {setup}"), draws)
        }

        /// `DWORD_00528694[0xF]` is `FUN_00471840`, weapon 4's bleeding
        /// flinch: two bloods of row 3, then the flinch's clip 1.
        #[test]
        fn the_smg_makes_a_spider_bleed_as_weapon_4() {
            let hit = |weapon: u8| {
                spider(
                    &format!("ROUTINES = {{ [0] = 2, {weapon}, 0, 0 }}"),
                    &[1, 2],
                )
            };
            let smg = hit(SMG);
            // The flinch's own blood (row 0) follows.
            let row_3 = [0, 9, 0x1000, -1];
            assert_eq!(smg.effects, [row_3, row_3, [0, 0x19, 0x2000, -1]]);
            assert_eq!((smg.drawn, smg.clips.as_slice()), (2, [1].as_slice()));
            let four = hit(4);
            assert_eq!(
                (four.effects, four.drawn, four.clips),
                (smg.effects, 2, smg.clips)
            );
        }

        /// `FUN_00471b10`: with `+0x23d` at 0, five draws of a leg's part,
        /// each still on the body burning (row 5: effect 5, row 4 on its
        /// matrix), and `+0x23d` at 30; then it carries on with `+0x233`'s
        /// routines in the move, `Routine_0` 1, without a clip of the hit.
        #[test]
        fn the_flamethrower_burns_a_spiders_legs_and_doesnt_stop_it() {
            // The rest (`Routine_1` 0) as it was; leg 1 (parts 5 and 6) is
            // off, and part 8 isn't drawn.
            let setup = "ROUTINES = { [0] = 2, 0x10, 0, 0 }; VARS = { [12] = 2 }; HIDDEN = 1 << 8";
            // Parts 3, 8, 18, 5 and 4.
            let hit = spider(setup, &[0x00, 0x05, 0x1F, 0x02, 0x01]);
            let burns: Vec<i64> = hit
                .effects
                .iter()
                .filter(|call| call[..3] == [5, 4, 0x1000])
                .map(|call| call[3])
                .collect();
            assert_eq!(burns, [3, 18, 4]);
            assert_eq!(hit.routines[0], 1);
            assert!(!hit.clips.contains(&1), "no flinch");
            // Still counting down: no burn.
            let setup = "ROUTINES = { [0] = 2, 0x10, 0, 0 }; VARS = { [11] = 5 }";
            let hit = spider(setup, &[]);
            assert!(hit.effects.iter().all(|call| call[..2] != [5, 4]));
            assert_eq!(hit.routines[0], 1);
        }

        /// `DWORD_00528724[0xF]` is the handguns' `FUN_00471c10`; `[0x10]`
        /// is `FUN_00472580`: row 4 (effect 5, row 3) on the body, then four
        /// draws of a leg's part taking row 4 and row 5, before the handguns'
        /// death and its blood (row 0). In water (`Water`) row 5 takes 4's
        /// place; a body shot again doesn't burn.
        #[test]
        fn the_flamethrower_kills_a_spider_burning() {
            let death = |weapon: u8, vars: &str| {
                let setup =
                    format!("ROUTINES = {{ [0] = 3, {weapon}, 0, 0 }}; VARS = {{ {vars} }}");
                spider(&setup, &[0x00, 0x13, 0x0E, 0x01])
            };
            let smg = death(SMG, "");
            assert_eq!(smg.effects, [[0, 0x19, 0x2000, -1]]);
            assert_eq!((smg.drawn, smg.clips.as_slice()), (0, [0xC].as_slice()));
            let flame = death(FLAMETHROWER, "");
            let mut expected = vec![vec![5, 3, 0x1400, 0]];
            for part in [3, 6, 17, 4] {
                expected.push(vec![5, 3, 0x1400, part]);
                expected.push(vec![5, 4, 0x1000, part]);
            }
            expected.push(vec![0, 0x19, 0x2000, -1]);
            assert_eq!(flame.effects, expected);
            assert_eq!((flame.drawn, flame.clips), (4, smg.clips));
            let in_water = death(FLAMETHROWER, "[14] = -180");
            assert_eq!(in_water.effects[0], [5, 4, 0x1000, 0]);
            assert_eq!(in_water.effects[1], [5, 4, 0x1000, 3]);
            let again = death(FLAMETHROWER, "[16] = 1");
            assert!(again.effects.iter().all(|call| call[..2] != [5, 3]));
        }

        /// `FUN_00471c10`'s end in water: weapons 9, 10 and 0x10 (not the
        /// SMG) put their flames out there, row 7 (effect 4, row 0xF) on
        /// part 0, before the body floats.
        #[test]
        fn a_burning_spider_that_dies_in_water_puts_out_its_flames() {
            let death = |weapon: u8| {
                let setup = format!(
                    "ROUTINES = {{ [0] = 3, {weapon}, 0, 0 }}; VARS = {{ [14] = -180 }};
                     CLIP_LEN = 1; SEQ_END = 0x80000"
                );
                spider(&setup, &[])
            };
            let out = [4, 0xF, 0x1400, 0];
            let flame = death(FLAMETHROWER);
            assert!(flame.effects.iter().any(|call| call == &out));
            assert_eq!(flame.routines[0], 7);
            assert!(!death(SMG).effects.iter().any(|call| call == &out));
        }

        /// `0x528893 + 0x10` is 3: a young spider the flamethrower kills
        /// burns (effect 5, row 4) as weapon 0xA's does; the SMG's is row 0.
        #[test]
        fn a_young_spider_burns_to_the_flamethrower() {
            let script = include_str!("../../games/re2/enemy/26/enemy.lua");
            let stub = format!("{SPIDER_STUB}\n{YOUNG_STUB}");
            let death = |weapon: u8| {
                let setup = format!("KIND = 0x2002; ROUTINES = {{ [0] = 3, {weapon}, 0, 0 }}");
                tick(script, &stub, &setup, &[]).effects
            };
            assert_eq!(death(FLAMETHROWER), [[5, 4, 0x1000, -1]]);
            assert_eq!(death(0xA), death(FLAMETHROWER));
            assert_eq!(death(SMG), [[0, 0x19, 0x1000, -1]]);
        }

        /// `0x4767c0`'s `Routine_2` 0, the shot's death: clip 9 at the
        /// frame `Rnd & 7` picks (`Hokan_flg` 0xF), blood (effect 0, row 8)
        /// on part 0, a life of -1 and sound 3, then the slide this tick.
        /// The flamethrower (as 9, 10 and 0x11) also burns it (effect 5,
        /// row 4, `Rnd * 4 + 0xC18`) and sets `Timer3`; the SMG doesn't.
        #[test]
        fn a_shot_embryo_dies_and_the_flamethrower_burns_it() {
            let script = include_str!("../../games/re2/enemy/27/enemy.lua");
            let stub = format!(
                "{SPIDER_STUB}
                  {EMBRYO_STUB}"
            );
            let death = |weapon: u8| {
                let setup = format!(
                    "KIND = 3; ROUTINES = {{ [0] = 3, {weapon}, 0, 0 }}; SPEED = {{ 0x50, 0, 0 }}"
                );
                tick(script, &stub, &setup, &[0x0B, 0x10])
            };
            let smg = death(SMG);
            assert_eq!(smg.effects, [[0, 8, 0x17D0, 0]]);
            assert_eq!((smg.drawn, smg.clips.as_slice()), (1, [9].as_slice()));
            assert_eq!(smg.routines, [3, i64::from(SMG), 1, 0]);
            let flame = death(FLAMETHROWER);
            assert_eq!(
                flame.effects,
                [vec![0, 8, 0x17D0, 0], vec![5, 4, 0x10 * 4 + 0xC18, -1]]
            );
            assert_eq!(flame.drawn, 2);
        }
    }

    /// The sub-machine gun's (0xF) and the flamethrower's (0x10) hits on Mr. X, room 205's bodies, the ivy and its vines.
    mod smg_flame_tyrant_ivy {
        // SMG_FLAME_TYRANT_IVY_TESTS

        /// A stub `e` on the fields `w`: its routines, vars, `Timer0` to
        /// `Timer3`, `Damage_no`, `Be_flg`, the room's enemy flags and `Rnd`'s
        /// draws (`w.draws`, then 0). Every other method returns 0 and logs
        /// its call in `LOG` as `name(args)`; a part's place is its number.
        pub(super) const STUB: &str = "
            LOG = {}
            w = { r = { [0] = 0, 0, 0, 0 }, vars = {}, timers = { [0] = 0, 0, 0, 0 },
                draws = {}, drawn = 0, damage_no = 1, be = 0, room_enemy = 0xFF, life = 100 }
            local function log(name, ...)
                local args = {}
                for i = 1, select('#', ...) do args[i] = tostring((select(i, ...))) end
                LOG[#LOG + 1] = name .. '(' .. table.concat(args, ',') .. ')'
            end
            e = setmetatable({}, { __index = function(_, name)
                return function(_, ...) log(name, ...) return 0 end
            end })
            function e:routine(i) return w.r[i] end
            function e:set_routine(i, v) w.r[i] = v end
            function e:routines(a, b, c, d) w.r = { [0] = a, b, c, d } end
            function e:var(i) return w.vars[i] or 0 end
            function e:set_var(i, v) w.vars[i] = v end
            function e:timer(i) return w.timers[i] end
            function e:set_timer(i, v) w.timers[i] = v end
            function e:rnd() w.drawn = w.drawn + 1 return w.draws[w.drawn] or 0 end
            function e:damage_no() return w.damage_no end
            function e:be_flg() return w.be end
            function e:set_be_flg(v) w.be = v end
            function e:room_enemy() return w.room_enemy end
            function e:set_room_enemy(v) w.room_enemy = v end
            function e:life() return w.life end
            function e:set_life(v) w.life = v end
            function e:own_part(p) return p, 0, 0 end
        ";

        /// `script` (which defines `update`) on [`STUB`] after `setup`, for
        /// `ticks` ticks: the state, to read with `eval`, and the calls whose
        /// names start with one of `names`.
        pub(super) fn run(
            script: &str,
            setup: &str,
            ticks: usize,
            names: &[&str],
        ) -> (mlua::Lua, Vec<String>) {
            let (lua, budget) = crate::lua::new_sandboxed_state().unwrap();
            lua.load(STUB).exec().unwrap();
            lua.load(setup).exec().unwrap();
            lua.load(script).exec().unwrap();
            for _ in 0..ticks {
                budget.begin();
                lua.load("update(e)").exec().unwrap();
            }
            let log: Vec<String> = lua.globals().get("LOG").unwrap();
            let log = log
                .into_iter()
                .filter(|call| names.iter().any(|name| call.starts_with(name)))
                .collect();
            (lua, log)
        }

        pub(super) fn eval<T: mlua::FromLuaMulti>(lua: &mlua::Lua, code: &str) -> T {
            lua.load(code).eval().unwrap()
        }

        /// The ivy (`lib/ivy.lua`) as `update`, for [`run`].
        pub(super) fn ivy() -> String {
            format!(
                "IVY = (function() {} end)()
                 function update(e) IVY.update(e) end",
                include_str!("../../games/re2/lib/ivy.lua")
            )
        }

        /// One tick of the ivy's `Routine_0` `zero` for `weapon`: the effects,
        /// sounds and routine sets, and `Rnd`'s draws.
        fn ivy_tick(zero: u8, weapon: u8, setup: &str) -> (Vec<String>, i64) {
            let setup = format!("w.r = {{ [0] = {zero}, {weapon}, 0, 0 }} {setup}");
            let (lua, log) = run(&ivy(), &setup, 1, &["call_effect", "sound"]);
            (log, eval(&lua, "return w.drawn"))
        }

        /// `funcs_487037`'s 0xF cell (`0x529f00`) is `caseD_5`, weapon 4's:
        /// row 0's blood at one of `0x529f64`'s parts, sound 2, and back to
        /// the routines it was in.
        #[test]
        fn the_ivy_takes_the_smgs_hit_as_weapon_4s() {
            let setup = "w.draws = { 3 } w.vars[26] = 1 w.vars[27] = 4";
            let smg = ivy_tick(2, 0xF, setup);
            assert_eq!(smg, ivy_tick(2, 4, setup));
            // `0x529f64`[3] is part 20; row 0 is (0, 0x11, 0x1800).
            assert_eq!(smg.0[0], "call_effect(0,17,6144,0,20,0,0)");
            assert!(smg.0.contains(&"sound(2)".to_string()), "{:?}", smg.0);
        }

        /// `caseD_11` (`0x487580`): the flame sprays six of row 2's blood
        /// from `0x529e08`'s draw (`FUN_00487450`), sets `+0x23f` to 0x3c, and
        /// then hits as `caseD_2` does, without its sound. While `+0x23f`
        /// counts, only `caseD_2`'s blood comes, and one draw fewer.
        #[test]
        fn the_flame_sprays_the_ivy_once_in_0x3c_ticks() {
            let (log, drawn) = ivy_tick(2, 0x10, "w.damage_no = 0 w.draws = { 3, 1 }");
            // `0x529e08` from 3: parts 14, 5, 10, 20, 17 and 2, row 2 (5, 4,
            // 0x1000); then part 7 (`Rnd` 1, the head) in row 1.
            let parts: Vec<&str> = log.iter().map(|call| &call[..]).collect();
            assert_eq!(
                parts,
                [
                    "call_effect(5,4,4096,0,14,0,0)",
                    "call_effect(5,4,4096,0,5,0,0)",
                    "call_effect(5,4,4096,0,10,0,0)",
                    "call_effect(5,4,4096,0,20,0,0)",
                    "call_effect(5,4,4096,0,17,0,0)",
                    "call_effect(5,4,4096,0,2,0,0)",
                    "call_effect(0,17,4096,0,7,0,0)",
                ]
            );
            let (lua, _) = run(
                &ivy(),
                "w.r = { [0] = 2, 0x10, 0, 0 } w.damage_no = 0",
                1,
                &[],
            );
            assert_eq!(eval::<i64>(&lua, "return w.vars[23]"), 0x3C);
            // `Em_2E` counts `+0x23f` down before the hit: 1 lets it spray.
            let (waiting, waiting_drawn) = ivy_tick(2, 0x10, "w.damage_no = 0 w.vars[23] = 2");
            assert_eq!(waiting, ["call_effect(0,17,4096,0,3,0,0)"]);
            assert_eq!(waiting_drawn, drawn - 1);
            let (ready, _) = ivy_tick(2, 0x10, "w.damage_no = 0 w.vars[23] = 1");
            assert_eq!(ready.len(), 7);
        }

        /// `0x529f6c`'s 0xF cell (`0x529fa8`) is `FUN_00487600`, the
        /// handguns' death: row 1's blood at part 10.
        #[test]
        fn the_ivy_dies_of_the_smg_as_of_the_handgun() {
            let smg = ivy_tick(3, 0xF, "");
            assert_eq!(smg, ivy_tick(3, 2, ""));
            assert_eq!(smg.0, ["call_effect(0,17,4096,0,10,0,0)"]);
        }

        /// `FUN_00487980`, the flame's death: two sprays of row 3's blood,
        /// the fall, the vines' command 7 to both (`+0x225`, `+0x227`) and
        /// `+0x238` 1.
        #[test]
        fn the_flame_burns_the_ivy_down() {
            let setup = "w.r = { [0] = 3, 0x10, 0, 0 } w.draws = { 0, 5 }";
            let (lua, log) = run(&ivy(), setup, 1, &["call_effect"]);
            let rows: Vec<&str> = log.iter().map(|call| &call[..17]).collect();
            assert_eq!(rows, ["call_effect(5,1,4"; 12]);
            // `0x529e08` from 0 and from 5.
            let parts: Vec<i64> = log
                .iter()
                .map(|call| call.split(',').nth(4).unwrap().parse().unwrap())
                .collect();
            assert_eq!(parts, [0, 1, 9, 14, 5, 10, 10, 20, 17, 2, 21, 10]);
            let state: (u8, u8, i64, i64, i64, i64, i64) = eval(
                &lua,
                "return w.r[0], w.r[2], w.vars[8], w.vars[9], w.vars[16], w.be, w.room_enemy",
            );
            assert_eq!(state, (3, 1, 7, 3, 1, 4, 0xBF));
        }

        /// With `+0x23d` the flame's death stands: `Routine_2` 1, `Timer0`
        /// 0x78 and the sprays, then `Routine_0` 7 when `Timer0` runs out.
        #[test]
        fn a_burning_ivy_with_0x23d_stands_0x78_ticks() {
            let setup = "w.r = { [0] = 3, 0x10, 0, 0 } w.vars[21] = 1";
            let (lua, log) = run(&ivy(), setup, 1, &["call_effect"]);
            assert_eq!(log.len(), 12);
            let state: (u8, u8, i64, i64) =
                eval(&lua, "return w.r[0], w.r[2], w.timers[0], w.vars[8]");
            assert_eq!(state, (3, 1, 0x78, 7));
            let (lua, log) = run(&ivy(), setup, 0x78, &["call_effect"]);
            assert_eq!(log.len(), 12);
            assert_eq!(
                eval::<(u8, i64)>(&lua, "return w.r[0], w.timers[0]"),
                (3, 1)
            );
            let (lua, _) = run(&ivy(), setup, 0x79, &[]);
            assert_eq!(eval::<u8>(&lua, "return w.r[0]"), 7);
        }

        /// Mr. X after `ticks` ticks from a hit by `weapon` (`setup` first):
        /// his `e:effect` calls' kind, row and size, `+0x220` (the SMG's
        /// callback, 0xF) and `+0x228`.
        fn mr_x(weapon: u8, setup: &str, ticks: usize) -> (Vec<String>, i64, i64) {
            let setup = format!("w.r = {{ [0] = 2, {weapon}, 0, 0 }} {setup}");
            let script = include_str!("../../games/re2/enemy/2a/enemy.lua");
            let (lua, log) = run(script, &setup, ticks, &["effect("]);
            let log = log
                .iter()
                .map(|call| {
                    call["effect(".len()..]
                        .splitn(4, ',')
                        .take(3)
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .collect();
            let (bleeding, count) = eval(&lua, "return w.vars[9] or 0, w.vars[10] or 0");
            (log, bleeding, count)
        }

        /// `FUN_0047fea0`'s 0xF case, `FUN_00480130`: a drop of the
        /// handguns' blood, and `+0x220` keeps it running each tick, the hit's
        /// too, until `+0x228` reaches 10. Its drops on 4, 7 and 10 need fire
        /// held, which the script can't read, so they don't come.
        #[test]
        fn the_smg_keeps_mr_x_bleeding_for_ten_ticks() {
            let (log, bleeding, count) = mr_x(0xF, "", 1);
            assert_eq!(log, ["0,6,4096"]);
            assert_eq!((bleeding, count), (0xF, 2));
            assert_eq!(mr_x(0xF, "", 9).1, 0xF);
            let (log, bleeding, count) = mr_x(0xF, "", 10);
            assert_eq!((log.len(), bleeding, count), (1, 0, 11));
        }

        /// A hit while the SMG's callback runs sets `+0x228` back to 0, and
        /// the callback then draws its first drop again.
        #[test]
        fn a_hit_during_the_smgs_bleed_starts_it_again() {
            let (log, bleeding, count) = mr_x(2, "w.vars[9] = 0xF w.vars[10] = 5", 1);
            assert_eq!(log.len(), 2);
            assert_eq!((bleeding, count), (0xF, 1));
        }

        /// The flame's case: five of kind 5's row 4 at 0x1600, and no
        /// callback.
        #[test]
        fn the_flame_draws_mr_xs_kind_5_blood() {
            let (log, bleeding, count) = mr_x(0x10, "", 1);
            assert_eq!(log, ["5,4,5632"; 5]);
            assert_eq!((bleeding, count), (0, 0));
        }

        /// Room 205's body after a tick of `Routine_0` `zero` by `weapon`
        /// with `Damage_no` `damage_no` and `Rnd` `draw`: its routines,
        /// `Damage_cnt`'s 0x80, its life, and its effects and sounds.
        fn wall_body(
            zero: u8,
            weapon: u8,
            damage_no: u8,
            kind: u16,
            draw: u8,
        ) -> (Vec<u8>, i64, i64, Vec<String>) {
            let setup = format!(
                "w.r = {{ [0] = {zero}, {weapon}, 0, 0 }} w.damage_no = {damage_no}
                 w.draws = {{ {draw} }} w.life = -5
                 function e:kind() return {kind} end
                 function e:damage_cnt() return w.dc or 0 end
                 function e:set_damage_cnt(v) w.dc = v end"
            );
            let script = include_str!("../../games/re2/enemy/2d/enemy.lua");
            let (lua, log) = run(script, &setup, 1, &["effect", "sound"]);
            let (r0, r1, r2, r3, dc, life) = eval(
                &lua,
                "return w.r[0], w.r[1], w.r[2], w.r[3], w.dc or 0, w.life",
            );
            (vec![r0, r1, r2, r3], dc, life, log)
        }

        /// `FUN_004848b0` and `FUN_00484910`: every weapon's hit and death
        /// send the body back into the wall (`0x501`), the death with blood
        /// at its kind's first part and its life back to 250; `Rnd`'s low bit
        /// picks sound 3 or 2. `Damage_no` 2's cells are 0.
        #[test]
        fn room_205s_bodies_take_the_smg_and_the_flame_as_any_hit() {
            for weapon in [2, 0xF, 0x10] {
                let hurt = wall_body(2, weapon, 0, 0, 0);
                assert_eq!(
                    hurt,
                    (vec![1, 5, 0, 0], 0x80, -5, vec!["sound(3)".to_string()])
                );
                let died = wall_body(3, weapon, 1, 1, 1);
                assert_eq!(
                    died,
                    (
                        vec![1, 5, 0, 0],
                        0x80,
                        250,
                        vec![
                            "effect(0,2,4096,0,8,0,0,0)".to_string(),
                            "sound(2)".to_string()
                        ]
                    )
                );
            }
            assert_eq!(wall_body(2, 0x10, 2, 0, 0).0, [2, 0x10, 0, 0]);
            assert_eq!(wall_body(3, 0x10, 5, 0, 0).2, -5);
        }
    }

    /// The sub-machine gun's (0xF) and the flamethrower's (0x10) hits on G1, G2 and G3.
    mod smg_flame_birkin {
        // SMG_FLAME_BIRKIN_TESTS

        /// One tick of `script` with `Routine_0` `zero` and `Routine_1`
        /// `weapon`, `Damage_no` 1 and the vars `vars` (a Lua table's
        /// fields): `(e:effect calls as kind, row, size, part, x;
        /// e:call_effect calls as kind, row, size, x; Rnd draws)`.
        pub(super) fn birkin_tick(
            script: &str,
            zero: u8,
            weapon: u8,
            damage_no: u8,
            vars: &str,
        ) -> (Vec<Vec<i64>>, Vec<Vec<i64>>, i64) {
            let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
            lua.globals().set("ZERO", zero).unwrap();
            lua.globals().set("WEAPON", weapon).unwrap();
            lua.globals().set("DAMAGE_NO", damage_no).unwrap();
            lua.load(format!(
                "local w = {{ r = {{ [0] = ZERO, WEAPON, 0, 0 }}, vars = {{ {vars} }}, drawn = 0 }}
                 E = {{}}
                 C = {{}}
                 local e = setmetatable({{}}, {{ __index = function() return function() return 0 end end }})
                 function e:routine(i) return w.r[i] end
                 function e:set_routine(i, v) w.r[i] = v end
                 function e:routines(a, b, c, d) w.r = {{ [0] = a, b, c, d }} end
                 function e:var(i) return w.vars[i] or 0 end
                 function e:set_var(i, v) w.vars[i] = v end
                 function e:rnd() w.drawn = w.drawn + 1 return 1 end
                 function e:damage_no() return DAMAGE_NO end
                 function e:easy() return false end
                 function e:angle() return 0x123 end
                 function e:pos() return 0, 0, 0 end
                 function e:rot_apply() return 0, 0, 0 end
                 function e:speed() return 0, 0, 0 end
                 function e:drawn_part_point(part) return part * 10, 0, 0 end
                 function e:effect(kind, row, size, dir, part, x, y, z)
                     E[#E + 1] = {{ kind, row, size, part, x }}
                 end
                 function e:call_effect(kind, row, size, dir, x, y, z)
                     C[#C + 1] = {{ kind, row, size, x }}
                 end
                 {script}
                 update(e)
                 DRAWN = w.drawn"
            ))
            .exec()
            .unwrap();
            let g = lua.globals();
            (
                g.get("E").unwrap(),
                g.get("C").unwrap(),
                g.get("DRAWN").unwrap(),
            )
        }

        /// G1's hit (`FUN_0048dc70`) with the flamethrower: no drops of its
        /// own, but `FUN_0048e060`'s four bloods of kind 5, row 4 at parts
        /// 2, 0, 5 and 15, each `Rnd * 4` over its size, once in 0x2d ticks
        /// (`+0x22c`, var 5). Its death (`FUN_0048f050`, shot again once
        /// dead) bleeds so whatever `+0x22c` holds.
        #[test]
        fn g1_bleeds_at_four_parts_from_a_flame() {
            let script = include_str!("../../games/re2/enemy/30/enemy.lua");
            // `FLAGS` 0x80 (var 3): mid flinch, so the hit goes on with it.
            let (effects, drops, drawn) = birkin_tick(script, 2, 0x10, 1, "[3] = 0x80, [5] = 1");
            assert_eq!(
                effects,
                [
                    vec![5, 4, 0x17D0 + 4, 2, 0],
                    vec![5, 4, 0x1FA0 + 4, 0, 0],
                    vec![5, 4, 0x1000 + 4, 5, 0],
                    vec![5, 4, 0x1000 + 4, 15, 0],
                ]
            );
            assert!(drops.is_empty());
            // `+0x22c` 2 counts down to 1 this tick: no blood, four fewer draws.
            let (held, _, held_drawn) = birkin_tick(script, 2, 0x10, 1, "[3] = 0x80, [5] = 2");
            assert!(held.is_empty());
            assert_eq!(drawn - held_drawn, 4);
            // The SMG adds no flame blood: the hit's own two drops.
            let (smg, smg_drops, _) = birkin_tick(script, 2, crate::player2::SMG, 1, "[3] = 0x80");
            assert!(smg.is_empty());
            assert_eq!(smg_drops.len(), 2);
            // Dead (`FLAGS` 0x10) and shot again, `+0x22c` still counting.
            let (dead, dead_drops, _) = birkin_tick(script, 3, 0x10, 1, "[3] = 0x10, [5] = 9");
            assert_eq!(dead.len(), 4);
            assert!(dead_drops.is_empty());
        }

        /// G2's hit (`FUN_00494440`) with the flamethrower: no drops of its
        /// own, and `FUN_00494940`'s seven bloods once in 0x2d ticks
        /// (`+0x22e`, var 11). Its death (`FUN_00495bc0`) bleeds so at
        /// every flame, even while `FLAGS` lacks 0x100 and it only goes on
        /// with 10 life.
        #[test]
        fn g2_bleeds_at_seven_parts_from_a_flame() {
            let script = include_str!("../../games/re2/enemy/31/enemy.lua");
            // `FLAGS` 0x200 (var 4): mid flinch, so the hit goes on with it.
            let (effects, drops, drawn) = birkin_tick(script, 2, 0x10, 1, "[4] = 0x200, [11] = 1");
            assert_eq!(
                effects,
                [
                    vec![5, 4, 0x3AF8 + 4, 2, 0],
                    vec![5, 4, 0x2770 + 4, 0, 0],
                    vec![5, 4, 0x1000 + 4, 5, 0],
                    vec![5, 4, 0x1FA0 + 4, 15, 0],
                    vec![5, 4, 0x17D0 + 4, 13, 0],
                    vec![5, 4, 0x1000 + 4, 10, 0],
                    vec![4, 0xF, 0x17D0 + 4, 3, 0],
                ]
            );
            assert!(drops.is_empty());
            let (held, _, held_drawn) = birkin_tick(script, 2, 0x10, 1, "[4] = 0x200, [11] = 2");
            assert!(held.is_empty());
            assert_eq!(drawn - held_drawn, 7);
            let (smg, smg_drops, _) = birkin_tick(script, 2, crate::player2::SMG, 1, "[4] = 0x200");
            assert!(smg.is_empty());
            assert_eq!(smg_drops.len(), 4);
            let (dead, dead_drops, _) = birkin_tick(script, 3, 0x10, 1, "[11] = 9");
            assert_eq!(dead.len(), 7);
            assert!(dead_drops.is_empty());
        }

        /// G3's hit (`FUN_00499260`) with the SMG: `DWORD_0052a77c`'s
        /// `FUN_00499570`, one blood of `BYTE_0052a5c3`'s kind 1 (kind 0,
        /// row 1, 0x2000 at the part's point) at `BYTE_0052a5dc[Damage_no %
        /// 3 * 4 + (Rnd & 3)]`, then the hit's own two at `HIT_PART`.
        #[test]
        fn g3_bleeds_at_a_spot_from_the_smg() {
            let script = include_str!("../../games/re2/enemy/33/enemy.lua");
            let smg = |damage_no: u8| birkin_tick(script, 2, crate::player2::SMG, damage_no, "");
            // `Rnd` 1 picks index 1, 5 and 9 (past the spray's eight):
            // parts 25, 4 and 3.
            for (damage_no, part) in [(0, 25), (1, 4), (2, 3)] {
                let (effects, drops, drawn) = smg(damage_no);
                assert!(effects.is_empty());
                assert_eq!(drops.len(), 3);
                assert_eq!(drops[0], [0, 1, 0x2000, part * 10]);
                assert_eq!(drawn, 1);
            }
        }
    }

    /// The spark shot's (0xE) hits on the alligator, the giant spider, Mr. X,
    /// the ivy and G1 to G3.
    mod spark_others {
        use super::smg_flame_birkin::birkin_tick;
        use super::smg_flame_small::{ALLIGATOR, ZEROS, spider, tick};
        use super::smg_flame_tyrant_ivy::{eval, ivy, run};

        /// `FUN_0046af30`'s 0xE case calls `FUN_0046b750`, whose first call
        /// only sets `+0x234` (var 22, 4) and `+0x231` (var 17, 1); `Em_23`
        /// then runs it the same tick: `DAT_00527fc0`'s first record, a
        /// spark of kind 6 sized `Rnd * 2 + 0x1800`.
        #[test]
        fn a_spark_hit_sets_the_alligator_sparking() {
            let hit = tick(
                ALLIGATOR,
                ZEROS,
                "ROUTINES = { [0] = 2, 0xE, 0, 0 }; VARS = { [12] = 9 }",
                &[3],
            );
            assert_eq!(hit.effects, [[6, 0, 0x1800 + 6, -1]]);
            assert_eq!(hit.drawn, 1);
        }

        /// The alligator's sparks over the ticks after: a record a tick,
        /// those with `+10`'s bit 2 skipped without a draw, the records
        /// again after the 21st, and the last at `+0x231` 0x18, which then
        /// stops (0x19, `+0x234` 0).
        #[test]
        fn the_alligators_sparks_follow_dat_00527fc0() {
            let sparks = |count: u8| {
                let setup = format!(
                    "w.r = {{ [0] = 3, 0, 0, 0 }} w.vars[0] = 2 w.vars[22] = 4 w.vars[17] = {count}
                     function e:drawn_part_point(_, x, y, z) return x, y, z end"
                );
                let script = include_str!("../../games/re2/enemy/23/enemy.lua");
                let (lua, log) = run(script, &setup, 1, &["call_effect"]);
                let state: (i64, i64, i64) =
                    eval(&lua, "return w.vars[22] or 0, w.vars[17] or 0, w.drawn");
                (log, state)
            };
            assert_eq!(
                sparks(1),
                (
                    vec!["call_effect(6,0,6144,0,3100,-800,0)".to_string()],
                    (4, 2, 1)
                )
            );
            assert_eq!(sparks(2), (vec![], (4, 3, 0)));
            assert_eq!(sparks(10).0, ["call_effect(6,0,6144,0,900,600,800)"]);
            // 22 is the first record again.
            assert_eq!(sparks(22).0, ["call_effect(6,0,6144,0,3100,-800,0)"]);
            assert_eq!(
                sparks(0x18),
                (
                    vec!["call_effect(6,0,6144,0,2500,-100,-200)".to_string()],
                    (0, 0x19, 1)
                )
            );
        }

        /// `DWORD_00528694[0xE]`, `FUN_00471ae0`: `+0x23b` (var 31) at 0x14,
        /// `FUN_00472df0`'s row 10 (kind 4, row 0xC on part 0's matrix), and
        /// the hard flinch as `Routine_1` 7. Then `Em_25`'s `FUN_00473530`
        /// counts it down, sparking (row 9) on every odd count left.
        #[test]
        fn a_spark_hit_flinches_the_spider_hard_and_sets_it_sparking() {
            let hit = spider("ROUTINES = { [0] = 2, 0xE, 0, 0 }", &[]);
            assert_eq!(hit.effects[0], [4, 0xC, 0x1000, 0]);
            let row_9: Vec<_> = hit
                .effects
                .iter()
                .filter(|call| call[..3] == [6, 0, 0x1000])
                .collect();
            // 0x13 left: three draws of `BYTE_00528848`, all 0: part 0.
            assert_eq!(row_9, [&vec![6, 0, 0x1000, 0]; 3]);
            let hard = spider("ROUTINES = { [0] = 2, 7, 0, 0 }", &[]);
            assert_eq!(hit.clips, hard.clips);
        }

        /// `FUN_00473530` alone: on an odd count left, three of
        /// `BYTE_00528848`'s parts by `Rnd & 7`, each still drawn and, for a
        /// leg's, not lost.
        #[test]
        fn the_spiders_sparks_skip_parts_that_are_off() {
            let sparks = |setup: &str, draws: &[u8]| {
                let hit = spider(
                    &format!("ROUTINES = {{ [0] = 4, 0, 0, 0 }}; {setup}"),
                    draws,
                );
                let parts: Vec<i64> = hit.effects.iter().map(|call| call[3]).collect();
                (parts, hit.drawn)
            };
            assert_eq!(
                sparks("VARS = { [31] = 2 }", &[1, 7, 5]),
                (vec![3, 0x13, 0], 3)
            );
            assert_eq!(sparks("VARS = { [31] = 3 }", &[1, 7, 5]), (vec![], 0));
            assert_eq!(sparks("VARS = { [31] = 0 }", &[1, 7, 5]), (vec![], 0));
            // Part 19 hidden, leg 0 (parts 3 and 4) lost.
            let off = "VARS = { [31] = 2, [12] = 1 }; HIDDEN = 1 << 0x13";
            assert_eq!(sparks(off, &[1, 7, 5]), (vec![0], 3));
        }

        /// `DWORD_00528724[0xE]`, `FUN_004726d0`: row 10 on parts 19 and 0,
        /// `+0x23b` at 0x1e, then the handguns' death (`Routine_1` 2) with its
        /// blood; and the first of `FUN_00473530`'s sparks.
        #[test]
        fn a_spark_kills_the_spider_sparking() {
            let death = spider("ROUTINES = { [0] = 3, 0xE, 0, 0 }", &[]);
            assert_eq!(
                death.effects[..3],
                [
                    vec![4, 0xC, 0x1000, 0x13],
                    vec![4, 0xC, 0x1000, 0],
                    vec![0, 0x19, 0x2000, -1]
                ]
            );
            assert_eq!(death.effects[3..], vec![vec![6, 0, 0x1000, 0]; 3]);
            assert_eq!(death.routines[1], 2);
            // A body that died once takes no sparks.
            let again = spider(
                "ROUTINES = { [0] = 3, 0xE, 0, 0 }; VARS = { [16] = 1 }",
                &[],
            );
            assert!(
                again
                    .effects
                    .iter()
                    .all(|call| call[0] != 4 && call[0] != 6)
            );
        }

        /// Mr. X's `FUN_0047fea0` 0xE case, `FUN_00480280`: its first call
        /// sets `+0x220` (var 9) and `+0x228` (var 10) going, and `Em_2A` runs
        /// it the same tick; then `DAT_00529478`'s records, a spark of kind 6
        /// (`Rnd * 2 + 0x1000`) each tick but for the skipped, until the 24th.
        #[test]
        fn a_spark_hit_keeps_mr_x_sparking_for_24_ticks() {
            let mr_x = |ticks: usize| {
                let script = include_str!("../../games/re2/enemy/2a/enemy.lua");
                let (lua, log) = run(script, "w.r = { [0] = 2, 0xE, 0, 0 }", ticks, &["effect("]);
                let state: (i64, i64) = eval(&lua, "return w.vars[9] or 0, w.vars[10] or 0");
                (log, state)
            };
            assert_eq!(mr_x(1), (vec![], (0xE, 2)));
            assert_eq!(mr_x(3).0, ["effect(6,0,4096,0,0,-500,0,0)"]);
            let (log, state) = mr_x(23);
            assert_eq!(log.len(), 8);
            assert_eq!(log[7], "effect(6,0,4096,0,4,350,500,100)");
            assert_eq!(state, (0xE, 0x18));
            assert_eq!(mr_x(24).1, (0, 0x19));
        }

        /// The ivy's `funcs_487037[0xE]`, `caseD_f`: two of row 10 (kind 4,
        /// row 0xC, 0x1800) at `0x529e08`'s parts, `+0x23c` (var 20) at 0xF,
        /// then `caseD_2`'s blood and sound; and `Em_2E`'s first of row 9's.
        #[test]
        fn a_spark_hit_sets_the_ivy_sparking() {
            let setup = "w.r = { [0] = 2, 0xE, 0, 0 } w.draws = { 3, 5, 7 }";
            let (lua, log) = run(&ivy(), setup, 1, &["call_effect", "sound"]);
            assert_eq!(
                log,
                [
                    "call_effect(4,12,6144,0,14,0,0)",
                    "call_effect(4,12,6144,0,10,0,0)",
                    "call_effect(0,17,4096,0,10,0,0)",
                    "sound(2)",
                    "call_effect(6,0,4096,0,17,0,0)",
                ]
            );
            assert_eq!(eval::<i64>(&lua, "return w.vars[20]"), 0xE);
        }

        /// `0x529f6c[0xE]`, `FUN_00487ac0`: the fall, three of row 10, `+0x23c`
        /// at 0x2d, the vines' command 7 and `+0x238`; `Rot_neck_em` turns
        /// `Neck_no` 0.
        #[test]
        fn a_spark_kills_the_ivy_sparking() {
            let setup = "w.r = { [0] = 3, 0xE, 0, 0 } w.draws = { 1, 2, 4 }";
            let (lua, log) = run(&ivy(), setup, 1, &["call_effect", "rot_neck_joint"]);
            assert_eq!(
                log[..3],
                [
                    "call_effect(4,12,6144,0,1,0,0)",
                    "call_effect(4,12,6144,0,9,0,0)",
                    "call_effect(4,12,6144,0,5,0,0)",
                ]
            );
            assert!(log[3].starts_with("rot_neck_joint(0,"), "{log:?}");
            assert_eq!(log[4..], ["call_effect(6,0,4096,0,0,0,0)"]);
            let state: (i64, i64, i64, i64) =
                eval(&lua, "return w.vars[20], w.vars[8], w.vars[16], w.r[2]");
            assert_eq!(state, (0x2C, 7, 1, 1));
            let (_, plain) = run(&ivy(), "w.r = { [0] = 3, 2, 0, 0 }", 1, &["rot_neck_joint"]);
            assert!(plain[0].starts_with("rot_neck_joint(9,"), "{plain:?}");
        }

        /// The kind 6 sparks of G1's `FUN_0048e5e0` and G2's `FUN_00494f70`
        /// (`Rnd` 1: sizes 8 over their bases, x 0x7f), then their three of
        /// kind 4, row 0xF (4 over 0x1000) with no offset.
        fn sparks(parts: &[(i64, i64)], tails: [i64; 3]) -> Vec<Vec<i64>> {
            let mut calls: Vec<Vec<i64>> = parts
                .iter()
                .map(|&(part, base)| vec![6, 0, base + 8, part, 0x7F])
                .collect();
            calls.extend(tails.iter().map(|&part| vec![4, 0xF, 0x1004, part, 0]));
            calls
        }

        /// G1's hit (`FUN_0048dc70`), its death (`FUN_0048f2b0`) and a dead
        /// G1 shot again (`FUN_0048f050`) call `FUN_0048e5e0` for 0xE.
        #[test]
        fn g1_sparks_from_the_spark_shot() {
            let script = include_str!("../../games/re2/enemy/30/enemy.lua");
            let expected = sparks(
                &[
                    (2, 0x17D0),
                    (2, 0x17D0),
                    (0, 0x1BB8),
                    (1, 0x1BB8),
                    (13, 0x13E8),
                    (7, 0x13E8),
                    (4, 0x13E8),
                ],
                [1, 10, 4],
            );
            let (effects, drops, drawn) = birkin_tick(script, 2, 0xE, 1, "[3] = 0x80");
            assert_eq!(effects, expected);
            assert_eq!(drops.len(), 2);
            let (smg, _, smg_drawn) = birkin_tick(script, 2, crate::player2::SMG, 1, "[3] = 0x80");
            assert!(smg.is_empty());
            assert_eq!(drawn - smg_drawn, 31);
            let (death, _, _) = birkin_tick(script, 3, 0xE, 1, "");
            assert_eq!(death, expected);
            let (again, again_drops, _) = birkin_tick(script, 3, 0xE, 1, "[3] = 0x10");
            assert_eq!(again[..10], expected[..]);
            assert_eq!(again_drops.len(), 2);
        }

        /// G2's hit (`FUN_00494440`) and death (`FUN_00495bc0`) call
        /// `FUN_00494f70` for 0xE, and so does its flinch (`FUN_00495ad0`) at
        /// frame 15 while `Routine_1` is still 0xE.
        #[test]
        fn g2_sparks_from_the_spark_shot() {
            let script = include_str!("../../games/re2/enemy/31/enemy.lua");
            let expected = sparks(
                &[
                    (2, 0x17D0),
                    (2, 0x17D0),
                    (0, 0x1BB8),
                    (1, 0x1BB8),
                    (12, 0x13E8),
                    (13, 0x13E8),
                    (10, 0x13E8),
                    (4, 0x13E8),
                    (16, 0x13E8),
                ],
                [1, 12, 4],
            );
            let (effects, drops, _) = birkin_tick(script, 2, 0xE, 1, "[4] = 0x200");
            assert_eq!(effects, expected);
            assert_eq!(drops.len(), 4);
            let (death, _, _) = birkin_tick(script, 3, 0xE, 1, "");
            assert_eq!(death, expected);
            let flinch = |weapon: u8| {
                let setup = format!(
                    "w.r = {{ [0] = 2, {weapon}, 1, 0 }}
                     function e:joint() return false end
                     function e:move_cnt() return 0xF end"
                );
                let (lua, log) = run(script, &setup, 1, &["effect("]);
                (log.len(), eval::<i64>(&lua, "return w.r[1]"))
            };
            assert_eq!(flinch(0xE), (12, 1));
            assert_eq!(flinch(2), (0, 1));
        }

        /// G3's `DWORD_0052a77c[0xE]`, `FUN_004996c0`: in a hit, `+0x219`
        /// (var 0) at 0xF and two of kind 4 at `BYTE_0052a5e4`'s parts; in
        /// the death 0x2d, two of kind 4 and one of kind 8. `Em_33` then
        /// counts `+0x219` down with two of kind 7 at `BYTE_0052a5de`'s.
        #[test]
        fn g3_sparks_from_the_spark_shot() {
            let script = include_str!("../../games/re2/enemy/33/enemy.lua");
            // `Rnd` 1: `BYTE_0052a5e4[1]` is part 3, `BYTE_0052a5de[1]` 27;
            // the matrix's kinds sit 300 along x.
            let four = vec![4, 0xF, 0x1000, 3, 300];
            let seven = vec![6, 0, 0x1000, 27, 300];
            let (hit, drops, _) = birkin_tick(script, 2, 0xE, 1, "");
            assert_eq!(
                hit,
                [four.clone(), four.clone(), seven.clone(), seven.clone()]
            );
            assert_eq!(drops.len(), 2);
            let (death, _, _) = birkin_tick(script, 3, 0xE, 1, "");
            let sparks: Vec<_> = death
                .iter()
                .filter(|call| call[0] == 4 || call[..2] == [6, 0])
                .collect();
            assert_eq!(
                sparks,
                [&four, &four, &vec![4, 0xC, 0x1800, 3, 300], &seven, &seven]
            );
            let (after, _, drawn) = birkin_tick(script, 2, 2, 1, "[0] = 2");
            assert_eq!(after.iter().filter(|call| **call == seven).count(), 2);
            let (_, _, plain_drawn) = birkin_tick(script, 2, 2, 1, "");
            assert_eq!(drawn - plain_drawn, 2);
        }
    }

    /// The gatling gun's (0x12) hits and deaths, against the original's tables.
    mod gatling {
        use super::smg_flame_small::spider;
        use super::smg_flame_tyrant_ivy::{eval, ivy, run};
        use super::smg_flame_zombie::{effects, tick};
        use crate::player2::{GATLING, SMG};

        /// `0x52715c`, the gatling gun's row of `Em10_Damage`'s table, is the
        /// SMG's `Em10Dm08` (`0x458260`): the same clip, blood, push and turn
        /// from the same draws, where the handguns' `Em10Dm00` staggers.
        #[test]
        fn the_gatling_gun_jerks_a_zombie_back_by_em10dm08() {
            let draws = "w.rnds = { 5, 10, 20, 30, 40, 1, 0x50, 0x60, 0x70 }";
            let gatling = format!("w.r[1] = {GATLING} {draws}");
            let result = "w.clip[1], w.clip[2], w.clip[3], w.spd[1], w.spd[2], w.spd[3], \
                          w.push, w.angle, w.toward, w.r[2], w.timers[0], w.vars[4], w.drawn";
            let hit = tick(&gatling, result);
            assert_eq!(hit, tick(draws, result));
            assert_eq!(hit[0], 0x1D);
            assert_eq!(effects(&gatling), effects(draws));
        }

        /// The ivy's hit (`Routine_0` 2) or death (3) by `weapon` on `Rnd`'s
        /// `draws`: its effects and sounds, and the draws taken.
        fn ivy_tick(zero: u8, weapon: u8, draws: &str) -> (Vec<String>, i64) {
            let setup = format!(
                "w.r = {{ [0] = {zero}, {weapon}, 0, 0 }} w.draws = {{ {draws} }}
                 w.vars[26] = 1 w.vars[27] = 4"
            );
            let (lua, log) = run(&ivy(), &setup, 1, &["call_effect", "sound"]);
            (log, eval(&lua, "return w.drawn"))
        }

        /// `funcs_487037`'s 0x12 cell (`0x529f0c`) is `caseD_5`, weapon 4's,
        /// which for 0x12 alone adds a second drop of row 1 (0, 0x11, 0x1000)
        /// at another of `0x529f64`'s parts, from a second `Rnd`.
        #[test]
        fn the_gatling_gun_bleeds_the_ivy_twice() {
            let four = ivy_tick(2, 4, "3, 6");
            assert_eq!(four.1, 1);
            let gatling = ivy_tick(2, GATLING, "3, 6");
            // `0x529f64`[3] is part 20, [6] part 5.
            assert_eq!(
                gatling,
                (
                    vec![
                        "call_effect(0,17,6144,0,20,0,0)".to_string(),
                        "call_effect(0,17,4096,0,5,0,0)".to_string(),
                        "sound(2)".to_string(),
                    ],
                    2
                )
            );
            assert_eq!(gatling.0[0], four.0[0]);
        }

        /// `0x529f6c`'s 0x12 cell (`0x529fb4`) is `FUN_00487600`, the
        /// handguns' death: row 1's blood at part 10.
        #[test]
        fn the_ivy_dies_of_the_gatling_gun_as_of_the_handgun() {
            let gatling = ivy_tick(3, GATLING, "");
            assert_eq!(gatling, ivy_tick(3, 2, ""));
            assert_eq!(gatling.0, ["call_effect(0,17,4096,0,10,0,0)"]);
        }

        /// The rocket launcher's entries (`0x529f08`, `0x529fb0`) are the
        /// Beretta's: `FUN_00487160`'s flinch and `FUN_00487bf0`'s tearing.
        #[test]
        fn the_rocket_launcher_hurts_and_kills_the_ivy_as_the_beretta() {
            for zero in [2, 3] {
                let rocket = ivy_tick(zero, crate::player2::ROCKET, "");
                assert_eq!(rocket, ivy_tick(zero, crate::player2::BERETTA, ""));
                assert_ne!(rocket, ivy_tick(zero, 0x14, ""), "{zero}");
            }
        }

        /// `DWORD_00528694[0x12]` is `FUN_00471840`, the SMG's and weapon
        /// 4's bleeding flinch, and `DWORD_00528724[0x12]` the handguns'
        /// `FUN_00471c10`, as the SMG's.
        #[test]
        fn the_gatling_gun_hurts_and_kills_a_spider_as_the_smg() {
            let tick = |zero: u8, weapon: u8| {
                let hit = spider(
                    &format!("ROUTINES = {{ [0] = {zero}, {weapon}, 0, 0 }}"),
                    &[1, 2],
                );
                (hit.effects, hit.drawn, hit.clips)
            };
            let hit = tick(2, GATLING);
            assert_eq!(hit, tick(2, SMG));
            let row_3 = vec![0, 9, 0x1000, -1];
            assert_eq!(hit.0[..2], [row_3.clone(), row_3]);
            assert_eq!(hit.2, [1]);
            let death = tick(3, GATLING);
            assert_eq!(death, tick(3, SMG));
            assert_eq!(death.0, [[0, 0x19, 0x2000, -1]]);
            assert_eq!(death.2, [0xC]);
        }
    }
}
