-- arklay's start for question_214: where --warp 214:-14355:0:-7100:3072
-- --life 200 leaves the player on the original's tick 344, facing the
-- weapons locker's area (aot 7) in the basement (B1).
local api = ...
api:set_player_pos(-14355, 0, -7100)
api:set_player_angle(3072)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
-- Room 100's visited and floor bits from the original's new game there
-- (oracle/re2.md, "MAP floors"); the room's own bits come from its load.
api:set_re2_flag(9, 0, true)
api:set_re2_flag(35, 0, true)
