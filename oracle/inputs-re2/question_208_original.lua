-- arklay's start for question_208_original: where --warp
-- 208:-19900:0:-15500:3072 --life 200 --flag 4:173 --flag 4:175 leaves the
-- player on the original's tick 344, facing the costume locker (aot 5), open
-- (4:175) and with a costume taken (4:173).
local api = ...
api:set_player_pos(-19900, 0, -15562)
api:set_player_angle(3072)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_re2_flag(4, 173, true)
api:set_re2_flag(4, 175, true)
-- Room 100's visited and floor bits from the original's new game there
-- (oracle/re2.md, "MAP floors"); the room's own bits come from its load.
api:set_re2_flag(9, 0, true)
api:set_re2_flag(35, 0, true)
