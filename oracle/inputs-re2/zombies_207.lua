-- arklay's start for the zombies_207 runs: where
-- --warp 207:-25640:0:-16590:0:1 leaves the player, at room 205's door 2
-- arrival in cut 1 (oracle/re2.md, "Room 207's zombies").
local api = ...
api:set_player_pos(-25640, 0, -16590)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(1)
