-- arklay's start for locked_door_20B: where --warp 20B:-1814:0:-12103:3029
-- leaves the player on the original's tick 344, without the heart key
-- (oracle/re2.md, "Locked doors").
local api = ...
api:set_player_pos(-1814, 0, -12103)
api:set_player_angle(3029)
api:set_health(200)
api:set_re2_rnd(61822)
api:set_room_camera(0)
