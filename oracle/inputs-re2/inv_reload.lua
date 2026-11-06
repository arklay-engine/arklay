-- arklay's start for inv_reload: item_102.lua's, with 5 rounds in the
-- handgun and 30 in slot 2 (--give 0:2:5 --give 2:0x14:30; oracle/re2.md,
-- "Items").
local api = ...
api:set_player_pos(-11378, 0, -4587)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(0, 0x02, 5)
api:set_item_slot(2, 0x14, 30)
