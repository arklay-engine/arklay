-- arklay's start for the inv_use_full and inv_check runs: item_102.lua's
-- place, with a green herb in slot 2 and a red herb in slot 3, as the
-- recordings' --give 2:0x26:1 --give 3:0x27:1 (oracle/re2.md, "Items").
local api = ...
api:set_player_pos(-11378, 0, -4587)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(2, 0x26, 1)
api:set_item_slot(3, 0x27, 1)
