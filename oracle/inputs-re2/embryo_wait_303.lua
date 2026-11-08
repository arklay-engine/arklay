-- arklay's start for embryo_wait_303 (oracle/re2-regress.d/em27.toml; oracle/re2.md, "The G embryos").
-- Rnd is 6114 one draw on: the original's load runs the G adult's init (slot 0,
-- one Rnd) before --no-enemies switches the slot off. The embryos' speeds are
-- what their new works' memory held, and so is the history of Cdir_y their first
-- five crawl ticks move along (vars 1 to 5 here, before the init's shift), read back
-- from the recording's steps (docs/internals/deviations/re2.md, "The G embryo").
local api = ...
api:set_player_pos(-24972, -5400, -13739)
api:set_player_angle(552)
api:set_health(200)
api:set_re2_rnd(12049)
api:set_room_camera(5)
api:set_re2_flag(4, 55)
api:set_re2_flag(4, 57)
api:set_re2_enemy_speed("303:1", 0, 0, 14156)
api:set_re2_enemy_speed("303:2", 0, 0, 4919)
api:set_re2_enemy_speed("303:3", 0, 8491, 9547)
api:set_re2_enemy_var("303:2", 1, 0)
api:set_re2_enemy_var("303:2", 2, 2065)
api:set_re2_enemy_var("303:2", 3, 2049)
api:set_re2_enemy_var("303:2", 4, 2063)
api:set_re2_enemy_var("303:2", 5, 0)
api:set_re2_enemy_var("303:3", 1, 3335)
api:set_re2_enemy_var("303:3", 2, 264)
api:set_re2_enemy_var("303:3", 3, 3337)
api:set_re2_enemy_var("303:3", 4, 3337)
api:set_re2_enemy_var("303:3", 5, 3337)
