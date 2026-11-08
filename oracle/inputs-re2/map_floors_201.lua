-- arklay's start for map_floors_201: where --warp 201:-14471:0:-24100:1024
-- --life 200 leaves the player on the original's tick 344, with the original's
-- --flag 33:2 --flag 33:3 --flag 33:4 --flag 35:3 --flag 35:4 (the maps of
-- floors 2 to 4 held, and floors 3 and 4 stood on), and room 100's visited
-- and floor bits (fg_map, bank 9, bit 0, and bank 35, bit 0), from the
-- original's new game there (oracle/re2.md, "MAP floors"). Room 201's own
-- bits come from its load.
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
