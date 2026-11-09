-- arklay's start for leon_a_207: where --warp 207:-25640:0:-16590:0:1 leaves
-- the player, on room 205's door 2's arrival in 207, as leon_a_205 leaves
-- Leon: 60 life, 3 rounds in the handgun and 54 spare.
local api = ...
api:set_player_pos(-25640, 0, -16590)
api:set_player_angle(0)
api:set_health(60)
api:set_re2_rnd(6114)
api:set_room_camera(1)
api:set_item_slot(0, 0x02, 3)
api:set_item_slot(1, 0x01, 1)
api:set_item_slot(2, 0x14, 54)
api:set_re2_enemy_dest("207:2", -5586, 128)
