-- arklay's start for item_merge_wrap_arrange_hard: item_102.lua's, with a
-- stack of 240 handgun rounds in slot 2 (--give 2:0x14:240), which HARD's 20
-- wraps to 4 (oracle/re2.md, "Items").
local api = ...
api:set_player_pos(-11378, 0, -4587)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(2, 0x14, 240)
