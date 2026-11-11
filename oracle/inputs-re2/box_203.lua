-- arklay's start for the box in room 203: where --warp
-- 203:-18700:0:-19000:1024 leaves the player on the original's tick 344,
-- on the aisle north of the box, facing it (oracle/re2.md, "Climbing onto an
-- object").
local api = ...
api:set_player_pos(-18700, 0, -19000)
api:set_player_angle(1024)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
