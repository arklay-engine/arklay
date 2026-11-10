-- arklay's start for leon_a_208_114: where --warp 208:-19005:0:-16920:1029:0
-- leaves the player, at the ink ribbons' table as route_200_208 leaves Leon:
-- 156 life, 2 rounds in the handgun, 38 spare and the ribbons in slot 3, with
-- the ribbons' item flag and room 207's four zombies' dead flags up.
local api = ...
api:set_player_pos(-19005, 0, -16920)
api:set_player_angle(1029)
api:set_health(156)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(0, 0x02, 2)
api:set_item_slot(1, 0x01, 1)
api:set_item_slot(2, 0x14, 38)
api:set_item_slot(3, 0x1E, 3)
api:set_re2_flag(8, 141)
api:set_re2_flag(6, 71)
api:set_re2_flag(6, 72)
api:set_re2_flag(6, 115)
api:set_re2_flag(6, 116)
