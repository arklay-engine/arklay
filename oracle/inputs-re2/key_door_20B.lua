-- arklay's start for key_door_20B and key_door_20B_keep: where
-- --warp 20B:-1814:0:-12103:3029 leaves the player on the original's tick
-- 344, with --give 2:0x5B:1 (the heart key, one use) and --give 3:0x14:30
-- (oracle/re2.md, "Locked doors").
local api = ...
api:set_player_pos(-1814, 0, -12103)
api:set_player_angle(3029)
api:set_health(200)
api:set_re2_rnd(61822)
api:set_room_camera(0)
api:set_item_slot(2, 0x5B, 1)
api:set_item_slot(3, 0x14, 30)
