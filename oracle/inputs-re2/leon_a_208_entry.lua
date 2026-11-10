-- arklay's start for leon_a_208_entry: where --no-enemies
-- --warp 207:-15328:0:-12706:990 leaves the player, in front of room 207's
-- door 2 to the darkroom (208), as leon_a_207 leaves Leon: 60 life, 10 rounds
-- in the handgun and 21 spare.
local api = ...
api:set_re2_no_enemies()
api:set_player_pos(-15328, 0, -12706)
api:set_player_angle(990)
api:set_health(60)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(0, 0x02, 10)
api:set_item_slot(1, 0x01, 1)
api:set_item_slot(2, 0x14, 21)
