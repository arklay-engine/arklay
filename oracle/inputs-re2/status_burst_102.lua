-- arklay's start for status_burst_102: status_102.lua's, with the custom handgun (4) in slot 0
-- (--give 0:4:18, which the warp keeps equipped in slot 0; oracle/re2.md, "The status screen").
local api = ...
api:set_player_pos(-11378, 0, -4587)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(0, 4, 18)
api:set_equipped(0x04)
