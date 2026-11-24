-- arklay's start for room 60B's ivies (oracle/re2-regress.d/ivy_lab.toml):
-- where --warp 60B:-14200:0:-21000:1536 --flag 4:106 --life 200 leaves the
-- player, at the panel whose check (area 5, `main_03`) lets the two kind 7
-- ivies out of their state 11 (flag 4:95).
local api = ...
api:set_player_pos(-14200, 0, -21000)
api:set_player_angle(1536)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_re2_flag(4, 106)
