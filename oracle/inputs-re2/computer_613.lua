-- arklay's start for room 613's terminal (oracle/re2-regress.d/computer_613.toml):
-- where --warp 613:-11300:0:-17300:0 leaves the player, in front of the
-- terminal inside event 3's area 1, with the moth (7:16) and its larvae
-- (7:33 to 7:37 and 7:46) dead, so the room has no enemies and event 3
-- raises 4:90, which lets the terminal be used.
local api = ...
api:set_player_pos(-11300, 0, -17300)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
for _, index in ipairs({ 16, 33, 34, 35, 36, 37, 46 }) do
    api:set_re2_flag(7, index)
end
-- the original's repeat counter at tick 343 (the trace's prep)
api:set_re2_pad_repeat(8)
