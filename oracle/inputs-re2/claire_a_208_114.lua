-- arklay's start for claire_a_208_114: where --start claire
-- --warp 208:-19956:0:-19363:149:1 leaves the player, at the darkroom's
-- typewriter as route_claire_a_208 leaves Claire once its prompt is answered
-- No: 119 life, 6 rounds in the handgun, 20 spare and the ink ribbons in slot 3,
-- with the ribbons' item flag and room 207's four zombies' dead flags up.
local api = ...
api:set_player_pos(-19956, 0, -19363)
api:set_player_angle(149)
api:set_health(119)
api:set_re2_rnd(6114)
api:set_room_camera(1)
api:set_item_slot(0, 0x03, 6)
api:set_item_slot(1, 0x01, 1)
api:set_item_slot(2, 0x14, 20)
api:set_item_slot(3, 0x1E, 3)
api:set_re2_flag(8, 141)
api:set_re2_flag(6, 71)
api:set_re2_flag(6, 72)
api:set_re2_flag(6, 115)
api:set_re2_flag(6, 116)
