-- arklay's start for claire_a_201, claire_a_204_entry and claire_a_204: where
-- --start claire --warp 201:-15773:0:-21630:2040:0 leaves the player, on room
-- 200's door 1's arrival in 201, with the handgun's 7 rounds and the 36 spare
-- that room 200's counter brought it to. The licker behind the window is on
-- (4:5 clear).
local api = ...
api:set_player_pos(-15773, 0, -21630)
api:set_player_angle(2040)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(0, 0x03, 7)
api:set_item_slot(1, 0x01, 1)
api:set_item_slot(2, 0x14, 36)
