-- arklay's start for status_repeat_file: status_file_reader.lua's, with file 4
-- in binder 1's first slot (--file 0:1 --file 1:3 --file 8:4; oracle/re2.md,
-- "Key repeat").
local api = ...
api:set_player_pos(-11378, 0, -4587)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_re2_flag(7, 0, true)
api:set_re2_file(0, 1)
api:set_re2_file(1, 3)
api:set_re2_file(8, 4)
-- the original's repeat counter at tick 343 (the trace's prep)
api:set_re2_pad_repeat(8)
