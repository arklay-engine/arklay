-- arklay's start for locked_inside_116: where
-- --warp 116:-26159:-7200:-25126:2048 leaves the player on the original's
-- tick 344, on floor 4 (oracle/re2.md, "Locked doors").
local api = ...
api:set_player_pos(-26159, -7200, -25126)
api:set_player_angle(2048)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
