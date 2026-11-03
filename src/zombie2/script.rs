//! The `e` of an RE2 enemy script: the fields of the original's work, and the
//! low-level calls behaviour is written with. See `docs/modding.md`.

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
            &mut self.em().z.vars
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
            e.em().z.sounds.push(slot as u8);
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
            |_, e, (x, z, floor, mask): (i32, i32, i32, i64)| {
                let room = e.em().mover.tick.room;
                Ok(room.sca.as_ref().map_or_else(Vec::new, |sca| {
                    let floor = (floor & 0x1F) as u8;
                    let list = crate::sca::list_at(sca, mask as u16, x, z, 10, floor);
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
            ))
        });
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
        methods.add_method_mut("blow_part", |_, e, (part, dir): (usize, i32)| {
            e.em().blow_part(part, dir as u16);
            Ok(())
        });
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
        methods.add_method_mut("pl_or_be_flg", |_, e, bits: i64| {
            e.em().world.pl.be_flg |= bits as u32;
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
                em.z.sounds.push((seq >> 28) as u8);
            }
            Ok(())
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

    const SCRIPTS: [&str; 4] = [
        include_str!("../../games/re2/enemy/20/enemy.lua"),
        include_str!("../../games/re2/enemy/21/enemy.lua"),
        include_str!("../../games/re2/enemy/22/enemy.lua"),
        include_str!("../../games/re2/lib/zombie.lua"),
    ];

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
            for call in SCRIPTS.iter().flat_map(|script| script.split("e:").skip(1)) {
                let name: String = call
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                // The end of a word, in a comment.
                if name.is_empty() {
                    continue;
                }
                let found: mlua::Value = method.call((&e, name.as_str()))?;
                assert!(!found.is_nil(), "e:{name} has no binding");
            }
            Ok(())
        })
        .unwrap();
    }
}
