-- arklay's start for film_204: where --warp 204:-15400:0:-8000:3072 leaves the
-- player, with room 204's entry scene seen (flag 4:85) and only the scene's
-- licker (slot 1) left running, as `record --no-enemies --keep-enemy 1` has it
-- (oracle/re2.md, "Films").
local api = ...
api:set_re2_no_enemies({"204:1"})
api:set_player_pos(-15400, 0, -8000)
api:set_player_angle(3072)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_re2_flag(4, 85)
