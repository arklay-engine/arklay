-- arklay's start for room 306's boxes: where --warp
-- 306:-21200:0:-22850:1024 leaves the player on the original's tick 344,
-- in the drained pit, north of nothing and south of box 0, facing it
-- (oracle/re2.md, "Climbing onto an object").
local api = ...
api:set_player_pos(-21200, 0, -22850)
api:set_player_angle(1024)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
