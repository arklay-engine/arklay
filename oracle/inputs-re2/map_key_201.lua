-- arklay's start for map_key_201: map_floors_201.lua's (the maps of floors 2
-- to 4 held, floors 3 and 4 stood on, and room 100's visited and floor bits),
-- with the original's key repeat counter (oracle/re2.md, "MAP floors").
local api = ...
api:set_player_pos(-15131, 0, -23453)
api:set_player_angle(1024)
api:set_health(200)
api:set_re2_rnd(12049)
api:set_room_camera(0)
api:set_re2_flag(9, 0, true)
api:set_re2_flag(35, 0, true)
api:set_re2_flag(33, 2, true)
api:set_re2_flag(33, 3, true)
api:set_re2_flag(33, 4, true)
api:set_re2_flag(35, 3, true)
api:set_re2_flag(35, 4, true)
-- the original's repeat counter at tick 343 (the trace's prep)
api:set_re2_pad_repeat(8)
