-- arklay's start for room 10C's statue puzzle: where --warp
-- 10C:-21300:-3600:-10500:2048 leaves the player on the original's tick 344,
-- on the hall's floor 2 on the pedestal's +z side, facing -x (oracle/re2.md,
-- "Room 10C's statues").
local api = ...
api:set_player_pos(-21300, -3600, -10500)
api:set_player_angle(2048)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
