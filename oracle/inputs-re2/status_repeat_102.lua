-- arklay's start for status_repeat_102: status_102.lua's, with four more items (six in all)
-- (--give 2:0x26:1 --give 3:0x27:1 --give 4:0x14:30 --give 5:0x28:1;
-- oracle/re2.md, "Key repeat").
local api = ...
api:set_player_pos(-11378, 0, -4587)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(2, 0x26, 1)
api:set_item_slot(3, 0x27, 1)
api:set_item_slot(4, 0x14, 30)
api:set_item_slot(5, 0x28, 1)
-- the original's repeat counter at tick 343 (the trace's prep)
api:set_re2_pad_repeat(8)
