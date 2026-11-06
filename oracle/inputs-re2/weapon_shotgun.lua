-- arklay's start for weapon_shotgun: weapon_knife_wall.lua's (item_102.lua's
-- with Leon's start items), with the shotgun's 5 shells in slot 2 and 10
-- shells in slot 3 (--give 2:7:5 --give 3:0x15:10; oracle/re2.md, "Weapons").
local api = ...
api:set_player_pos(-11378, 0, -4587)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(0, 0x02, 18)
api:set_item_slot(1, 0x01, 1)
api:set_item_slot(2, 0x07, 5)
api:set_item_slot(3, 0x15, 10)
api:set_equipped(0x02)
