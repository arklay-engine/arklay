-- arklay's start for question_20b: where --warp 20B:-11732:0:-10700:1024
-- --life 200 leaves the player on the original's tick 344, at the safe's
-- keypad, with room 100's visited and floor bits from the original's new
-- game there (oracle/re2.md, "MAP floors"). Room 20B's own bits come from
-- its load.
local api = ...
-- --no-enemies: room 20B's zombies are switched off.
api:set_re2_no_enemies()
api:set_player_pos(-11587, 0, -10700)
api:set_player_angle(1024)
api:set_health(200)
-- Rnd before the room's load, which draws 48 to reach the reference's 61822 on 344.
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_re2_flag(9, 0, true)
api:set_re2_flag(35, 0, true)
