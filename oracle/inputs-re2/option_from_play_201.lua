-- arklay's start for the option_from_play runs: map_key_201.lua's place in
-- room 201 without its maps, with the original's key repeat counter.
local api = ...
api:set_player_pos(-15131, 0, -23453)
api:set_player_angle(1024)
api:set_health(200)
api:set_re2_rnd(12049)
api:set_room_camera(0)
api:set_re2_flag(9, 0, true)
api:set_re2_flag(35, 0, true)
-- the original's repeat counter at tick 343 (the trace's prep)
api:set_re2_pad_repeat(8)
