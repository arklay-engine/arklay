-- arklay's start for inv_mix: inv_herbs.lua's, and 30 handgun rounds in
-- slot 4 (--give 4:0x14:30; oracle/re2.md, "Items").
local api = ...
api:set_player_pos(-11378, 0, -4587)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(2, 0x26, 1)
api:set_item_slot(3, 0x27, 1)
api:set_item_slot(4, 0x14, 30)
