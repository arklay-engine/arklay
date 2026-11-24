-- arklay's start for the HARD dogs of room 212 (oracle/re2-regress.d/dog_212.toml):
-- where --warp 212:-11250:0:-25692:3024 --flag 4:83 --life 200 leaves the
-- player, with HARD's Rnd as the title leaves it (58231).
local api = ...
api:set_player_pos(-11250, 0, -25692)
api:set_player_angle(3024)
api:set_health(200)
api:set_re2_rnd(58231)
api:set_room_camera(0)
api:set_re2_flag(4, 83)
