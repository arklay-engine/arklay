-- arklay's start for map_pickup_20b: where --warp 20B:-11732:0:-10700:1024
-- --life 200 leaves the player on the original's tick 344, with the original's
-- --flag 4:37 (the safe is open) and --flag 8:78 (its item is taken), and
-- room 100's visited and floor bits from the original's new game there
-- (oracle/re2.md, "MAP floors"). Room 20B's own bits come from its load.
local api = ...
api:set_player_pos(-11587, 0, -10700)
api:set_player_angle(1024)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_re2_flag(9, 0, true)
api:set_re2_flag(35, 0, true)
api:set_re2_flag(4, 37, true)
api:set_re2_flag(8, 78, true)
