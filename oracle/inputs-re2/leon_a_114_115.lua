-- arklay's start for leon_a_114_115: where --warp 114:-25260:0:-3420:2048:5
-- leaves the player, on room 10C's door 2's arrival in the corridor, as
-- leon_a_208_114 leaves Leon: 156 life, 2 rounds in the handgun, 38 spare, the
-- ink ribbons in slot 3 and the Red Jewel in slot 4.
local api = ...
api:set_player_pos(-25260, 0, -3420)
api:set_player_angle(2048)
api:set_health(156)
api:set_re2_rnd(6114)
api:set_room_camera(5)
api:set_item_slot(0, 0x02, 2)
api:set_item_slot(1, 0x01, 1)
api:set_item_slot(2, 0x14, 38)
api:set_item_slot(3, 0x1E, 3)
api:set_item_slot(4, 0x33, 1)
