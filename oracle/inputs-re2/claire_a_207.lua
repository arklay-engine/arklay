-- arklay's start for claire_a_207: where --start claire
-- --warp 207:-25640:0:-16590:0:1 leaves the player, on room 205's door 2's
-- arrival in 207, as route_claire_a_208 brings Claire there: 119 life, 8
-- rounds in the handgun and 38 spare.
local api = ...
api:set_player_pos(-25640, 0, -16590)
api:set_player_angle(0)
api:set_health(119)
api:set_re2_rnd(6114)
api:set_room_camera(1)
api:set_item_slot(0, 0x03, 8)
api:set_item_slot(1, 0x01, 1)
api:set_item_slot(2, 0x14, 38)
