-- arklay's start for claire_a_208_entry: where --start claire --no-enemies
-- --warp 207:-15162:0:-13112:49 leaves the player, in front of room 207's
-- door 2 to the darkroom (208), as route_claire_a_208 brings Claire there:
-- 119 life, 6 rounds in the handgun and 20 spare.
local api = ...
api:set_re2_no_enemies()
api:set_player_pos(-15162, 0, -13112)
api:set_player_angle(49)
api:set_health(119)
api:set_re2_rnd(6114)
api:set_room_camera(4)
api:set_item_slot(0, 0x03, 6)
api:set_item_slot(1, 0x01, 1)
api:set_item_slot(2, 0x14, 20)
