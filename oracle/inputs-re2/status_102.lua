-- arklay's start for the status_map and status_file runs: item_102.lua's,
-- plus room 100's visited bit (fg_map, bank 9, bit 0) and its floor's (bank
-- 35, bit 0). The original's runs start a new game in room 100 and warp to
-- 102, so its MAP page draws both rooms; arklay boots straight into 102
-- (oracle/re2.md, "Status screen").
local api = ...
api:set_player_pos(-11378, 0, -4587)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_re2_flag(9, 0, true)
api:set_re2_flag(35, 0, true)
