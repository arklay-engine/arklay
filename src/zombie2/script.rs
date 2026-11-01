//! The `e` of an RE2 enemy script: the fields of the original's work, and the
//! low-level calls behaviour is written with. See `docs/modding.md`.

use mlua::{UserData, UserDataMethods};

use super::licker::{Licker, VARS};
use super::{Atd, Em, PLAYER_BODY, direction_ck, goto, square_root0, turn_within};
use crate::player2::{self, Bank};

/// One script call's view of its enemy, valid only for that call: the host's
/// `Lua::scope` seals the userdata so the script can't keep it.
pub(super) struct Api {
    em: *mut Em<'static, 'static, 'static>,
    l: *mut Licker,
}

impl Api {
    pub(super) fn new(em: &mut Em<'_, '_, '_>, l: &mut Licker) -> Self {
        let em: *mut Em<'_, '_, '_> = em;
        Self { em: em.cast(), l }
    }

    fn em(&mut self) -> &mut Em<'static, 'static, 'static> {
        // SAFETY: `new` took these from live borrows that outlast the script
        // call this `Api` exists for, and `&mut self` makes this the only path
        // to them.
        unsafe { &mut *self.em }
    }

    fn l(&mut self) -> &mut Licker {
        // SAFETY: as in `em`; the `Licker` is out of the `Em` for the call.
        unsafe { &mut *self.l }
    }
}

fn var_index(index: i64) -> mlua::Result<usize> {
    usize::try_from(index)
        .ok()
        .filter(|index| *index < VARS)
        .ok_or_else(|| mlua::Error::RuntimeError(format!("no var {index}")))
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
        methods.add_method_mut("var", |_, e, index: i64| Ok(e.l().vars[var_index(index)?]));
        methods.add_method_mut("set_var", |_, e, (index, value): (i64, i32)| {
            e.l().vars[var_index(index)?] = value;
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
        methods.add_method_mut("flags", |_, e, ()| Ok(e.l().flags));
        methods.add_method_mut("set_flags", |_, e, flags: i64| {
            e.l().flags = flags as u16;
            Ok(())
        });
        methods.add_method_mut("reach", |_, e, ()| Ok(e.l().reach));
        methods.add_method_mut("set_reach", |_, e, reach: i16| {
            e.l().reach = reach;
            Ok(())
        });
        methods.add_method_mut("timer1", |_, e, ()| Ok(e.l().timer1));
        methods.add_method_mut("set_timer1", |_, e, timer: i16| {
            e.l().timer1 = timer;
            Ok(())
        });
        methods.add_method_mut("set_fading", |_, e, fading: i64| {
            e.l().fading = fading as u8;
            Ok(())
        });
        methods.add_method_mut("tongue", |_, e, ()| Ok(e.l().tongue));
        methods.add_method_mut("tongue_out", |_, e, ()| {
            e.l().tongue = 1;
            Ok(())
        });
        methods.add_method_mut(
            "set_atd",
            |_, e, (which, ofs_y, at_w, at_h, w): (i64, i16, u16, u16, u16)| {
                let atd = Atd {
                    ofs_y,
                    at_w,
                    at_h,
                    w,
                    at: [0; 3],
                };
                if which == 0 {
                    e.em().z.atd = atd;
                } else {
                    e.l().head = atd;
                }
                Ok(())
            },
        );
        methods.add_method_mut("init_parts", |_, e, ()| {
            let pos = e.em().mover.player.pos;
            let l = e.l();
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
            e.em().world.pl.held_kill = true;
            Ok(())
        });
        methods.add_method_mut("pl_life_down", |_, e, down: i32| {
            Ok(e.em().pl_life_down(down, false))
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

    const LICKER: &str = include_str!("../../games/re2/enemy/22/enemy.lua");

    #[test]
    fn the_licker_calls_only_what_the_sandbox_has() {
        for library in ["math.", "os.", "io.", "utf8.", "debug.", "coroutine."] {
            assert!(!LICKER.contains(library), "{library} isn't in the sandbox");
        }
        let (lua, _budget) = crate::lua::new_sandboxed_state().unwrap();
        lua.load(LICKER).exec().unwrap();
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
            for call in LICKER.split("e:").skip(1) {
                let name: String = call
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                let found: mlua::Value = method.call((&e, name.as_str()))?;
                assert!(!found.is_nil(), "e:{name} has no binding");
            }
            Ok(())
        })
        .unwrap();
    }
}
