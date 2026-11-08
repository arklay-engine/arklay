-- arklay's start for the push in room 10C: where --warp
-- 10C:-22905:-3600:-11900:1024 leaves the player on the original's tick 344,
-- on the hall's floor 2 south of statue 1, square to its +z face
-- (oracle/re2.md, "Pushing").
local api = ...
api:set_player_pos(-22905, -3600, -11900)
api:set_player_angle(1024)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
