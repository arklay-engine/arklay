-- arklay's start for the status_start runs: item_102.lua's place, with the G-Virus in the
-- personal slot (the recording's --give 10:0x52:1), the scenario flag that says it is gone
-- (3:162) and the Side Pack's item flag (8:63) (oracle/re2.md, "Items").
local api = ...
api:set_player_pos(-11378, 0, -4587)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_personal_item(0x52, 1)
api:set_re2_flag(3, 162)
api:set_re2_flag(8, 63)
