-- arklay's start for box_sidepack_201: box_201.lua's place, with the Side Pack's item flag
-- (8:63), eight held items and two in the box (oracle/re2.md, "The item box").
local api = ...
api:set_player_pos(-15131, 0, -23453)
api:set_player_angle(1024)
api:set_health(200)
api:set_re2_rnd(12049)
api:set_room_camera(0)
api:set_re2_flag(8, 63)
api:set_item_slot(2, 0x14, 30)
api:set_item_slot(3, 0x26, 1)
api:set_item_slot(4, 0x26, 1)
api:set_item_slot(5, 0x14, 20)
api:set_item_slot(6, 0x26, 1)
api:set_item_slot(7, 0x14, 10)
api:set_re2_box_slot(0, 0x26, 1)
api:set_re2_box_slot(1, 0x14, 15)
