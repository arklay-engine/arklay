-- arklay's start for claire_a_205: where --start claire
-- --warp 205:-25670:0:-25775:3072:0 leaves the player, on room 204's door 1's
-- arrival in 205, as route_claire_a_208 brings Claire there: 119 life, 9
-- rounds in the handgun and 38 spare. 4:48 is clear, so the bodies in the
-- walls stay frozen.
local api = ...
api:set_player_pos(-25670, 0, -25775)
api:set_player_angle(3072)
api:set_health(119)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(0, 0x03, 9)
api:set_item_slot(1, 0x01, 1)
api:set_item_slot(2, 0x14, 38)
