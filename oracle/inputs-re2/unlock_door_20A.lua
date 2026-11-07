-- arklay's start for unlock_door_20A: where --warp 20A:-15554:0:-15268:1866
-- leaves the player on the original's tick 344, after the warp's collision
-- push (oracle/re2.md, "Locked doors").
local api = ...
api:set_player_pos(-15486, 0, -15268)
api:set_player_angle(1866)
api:set_health(200)
api:set_re2_rnd(61822)
api:set_room_camera(0)
