-- arklay's start for key_door_207: where --warp 207:-25770:0:-24897:1846
-- leaves the player on the original's tick 344, with --give 2:0x5A:2, the
-- diamond key with two uses (oracle/re2.md, "Locked doors").
local api = ...
api:set_player_pos(-25770, 0, -24897)
api:set_player_angle(1846)
api:set_health(200)
api:set_re2_rnd(55383)
api:set_room_camera(0)
api:set_item_slot(2, 0x5A, 2)
