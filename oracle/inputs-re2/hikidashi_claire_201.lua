-- arklay's start for hikidashi_claire_201: where --start claire --warp
-- 201:-25400:0:-18450:3072 leaves Claire on the original's tick 344, south of
-- the desk and facing it, with her lockpick in the personal slot
-- (oracle/re2.md, "The drawer").
local api = ...
api:set_player_pos(-25400, 0, -18450)
api:set_player_angle(3072)
api:set_health(200)
api:set_re2_rnd(12049)
api:set_room_camera(2)
api:set_item_slot(0, 3, 13)
api:set_item_slot(1, 1, 1)
api:set_item_slot(2, 0)
api:set_personal_item(0x30)
