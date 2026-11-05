-- arklay's start for door_101_102_arklay.txt: room 100 at its door, as the
-- original is 780 ticks into door_101_102.txt with --no-enemies-room 0.
local api = ...
api:set_player_pos(-20524, 0, -18869)
api:set_player_angle(2064)
api:set_health(200)
api:set_re2_rnd(39135)
api:set_re2_sce_rnd(2060504824)
api:set_room_camera(8)
for i = 35, 41 do api:set_re2_flag(6, i) end
