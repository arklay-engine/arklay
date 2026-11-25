-- arklay's start for box_weapon_201: box_201.lua's player, with the box
-- empty but for a herb in slot 10 (oracle/re2.md, "The item box").
local api = ...
api:set_player_pos(-15131, 0, -23453)
api:set_player_angle(1024)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(2, 0x14, 30)
api:set_re2_box_slot(10, 0x26, 1)
