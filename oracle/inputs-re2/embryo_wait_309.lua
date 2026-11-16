-- arklay's start for embryo_wait_309 (oracle/re2-regress.d/em27.toml; oracle/re2.md, "The G embryos").
-- The G adult (slot 0) is in the room: its init draws one Rnd in the load, and
-- --no-enemies (here set_re2_no_enemies) switches the slot off after it. The embryos' speeds are
-- what their new works' memory held, and so is the history of Cdir_y their first
-- five crawl ticks move along (vars 1 to 5 here, before the init's shift), read back
-- from the recording's steps (docs/internals/deviations/re2.md, "The G embryo").
local api = ...
api:set_player_pos(-17273, -5400, 1678)
api:set_player_angle(488)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_re2_no_enemies({"309:1", "309:2", "309:3"})
api:set_room_camera(3)
api:set_re2_flag(4, 57)
api:set_re2_enemy_speed("309:1", 0, 0, 14156)
api:set_re2_enemy_speed("309:2", 0, 0, 4919)
api:set_re2_enemy_speed("309:3", 0, 6376, 6344)
api:set_re2_enemy_var("309:2", 1, 0)
api:set_re2_enemy_var("309:2", 2, 2065)
api:set_re2_enemy_var("309:2", 3, 2049)
api:set_re2_enemy_var("309:2", 4, 2063)
api:set_re2_enemy_var("309:2", 5, 0)
api:set_re2_enemy_var("309:3", 1, 3335)
api:set_re2_enemy_var("309:3", 2, 3336)
api:set_re2_enemy_var("309:3", 3, 265)
api:set_re2_enemy_var("309:3", 4, 3337)
api:set_re2_enemy_var("309:3", 5, 3337)
