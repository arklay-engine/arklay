-- arklay's start for typewriter_200_no_ribbon: where --warp 200:-12200:-1800:-1900:1024
-- leaves the player, facing the typewriter (area 5) left of the ink ribbons'
-- area, with room 200's entry scene seen (flag 4:6; oracle/re2.md, "The
-- typewriter").
local api = ...
api:set_player_pos(-12200, -1800, -1900)
api:set_player_angle(1024)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_re2_flag(4, 6)
