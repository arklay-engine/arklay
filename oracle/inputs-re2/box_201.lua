-- arklay's start for box_201: where --warp 201:-14471:0:-24100:1024 leaves
-- the player on the original's tick 344, with --give 2:0x14:30 and the box's
-- --box 0:0x14:15 --box 1:0x0E:7:3 --box 2:0x26:1 (oracle/re2.md, "The item box").
local api = ...
api:set_player_pos(-15131, 0, -23453)
api:set_player_angle(1024)
api:set_health(200)
api:set_re2_rnd(12049)
api:set_room_camera(0)
api:set_item_slot(2, 0x14, 30)
api:set_re2_box_slot(0, 0x14, 15)
api:set_re2_box_slot(1, 0x0E, 7, 3)
api:set_re2_box_slot(2, 0x26, 1)
