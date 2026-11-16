-- arklay's start for spark_equip_118: the state of the original's tick 344 after
-- --start claire --warp 118:-24617:0:-24996:1024 --life 200 --give 2:0xE:100.
-- The inputs EQUIP the spark shot in slot 2 through the status screen (oracle/re2.md).
local api = ...
api:set_player_pos(-24617, 0, -24996)
api:set_player_angle(1024)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
-- Claire's own: the handgun and its 13 and the knife.
api:set_item_slot(0, 0x03, 13)
api:set_item_slot(1, 0x01, 1)
api:set_item_slot(2, 0x0E, 100)
api:set_equipped(0x03)
