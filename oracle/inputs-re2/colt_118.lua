-- arklay's start for colt_118: the state of the original's tick 344 after
-- --warp 118:-24617:0:-24996:1024 --life 200 --equip-load 0xD --give 0:0xD:6
-- --give 1:0x14:60, as run_119's with the Colt S.A.A. (oracle/re2.md).
local api = ...
api:set_player_pos(-24617, 0, -24996)
api:set_player_angle(1024)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(0, 0x0D, 6)
api:set_item_slot(1, 0x14, 60)
api:set_equipped(0x0D)
