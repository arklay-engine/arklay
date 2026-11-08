-- arklay's start for statue 2 in room 10C: where --warp
-- 10C:-20100:-3600:-13900:3072 --flag 4:38 leaves the player on the
-- original's tick 344, on the hall's floor 2 north of statue 2, square to its
-- -z face, with statue 1 already on its plate (oracle/re2.md, "Room 10C's
-- statues").
local api = ...
api:set_player_pos(-20100, -3600, -13900)
api:set_player_angle(3072)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_re2_flag(4, 38)
