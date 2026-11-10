-- arklay's start for claire_a_208: where --start claire
-- --warp 208:-15462:0:-15336:1029:0 leaves the player, on room 207's door 2's
-- arrival in the darkroom, as route_claire_a_208 brings Claire there: 119
-- life, 6 rounds in the handgun and 20 spare.
local api = ...
api:set_player_pos(-15462, 0, -15336)
api:set_player_angle(1029)
api:set_health(119)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(0, 0x03, 6)
api:set_item_slot(1, 0x01, 1)
api:set_item_slot(2, 0x14, 20)
