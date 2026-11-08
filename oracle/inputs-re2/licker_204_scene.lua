-- arklay's start for licker_204_scene: where --warp 204:-15400:0:-8000:3072
-- leaves the player, with room 204's entry scene seen (flag 4:85), so that the
-- warp's place holds (oracle/re2.md, "Room 204's licker").
local api = ...
api:set_player_pos(-15400, 0, -8000)
api:set_player_angle(3072)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_re2_flag(4, 85)
