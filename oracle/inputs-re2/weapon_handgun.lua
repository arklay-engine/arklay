-- arklay's start for weapon_handgun: item_102.lua's, with 30 handgun rounds
-- in slot 2 (--give 2:0x14:30; oracle/re2.md, "Weapons").
local api = ...
api:set_player_pos(-11378, 0, -4587)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(2, 0x14, 30)
