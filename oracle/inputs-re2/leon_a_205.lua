-- arklay's start for leon_a_205: where --warp 205:-25670:0:-25775:3072:0 leaves
-- the player, on room 204's door 1's arrival in 205, as leon_a_204 leaves Leon:
-- 60 life after the licker, 3 rounds in the handgun and 54 spare. 4:48 is
-- clear, so the bodies in the walls stay frozen.
local api = ...
api:set_player_pos(-25670, 0, -25775)
api:set_player_angle(3072)
api:set_health(60)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(0, 0x02, 3)
api:set_item_slot(1, 0x01, 1)
api:set_item_slot(2, 0x14, 54)
