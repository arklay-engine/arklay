-- arklay's start for smg_equip_118: the state of the original's tick 344 after
-- --warp 118:-24617:0:-24996:1024 --life 200 --give 2:0x0F:100
-- --give 3:0x1B:100, as run_119's with Leon's own handgun and knife; the
-- inputs EQUIP slot 2 through the status screen (oracle/re2.md).
local api = ...
api:set_player_pos(-24617, 0, -24996)
api:set_player_angle(1024)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(0, 0x02, 18)
api:set_item_slot(1, 0x01, 1)
api:set_item_slot(2, 0x0F, 100)
api:set_item_slot(3, 0x1B, 100)
api:set_equipped(0x02)
