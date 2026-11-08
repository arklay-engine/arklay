-- arklay's start for the drawer runs in room 201: where --warp
-- 201:-25400:0:-18450:3072 leaves the player on the original's tick 344,
-- south of the desk and facing it, with Leon's lighter in the personal slot
-- (oracle/re2.md, "The drawer").
local api = ...
api:set_player_pos(-25400, 0, -18450)
api:set_player_angle(3072)
api:set_health(200)
api:set_re2_rnd(12049)
api:set_room_camera(2)
api:set_item_slot(2, 0)
api:set_personal_item(0x2F)
