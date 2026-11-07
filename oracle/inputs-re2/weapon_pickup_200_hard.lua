-- arklay's start for weapon_pickup_200_hard, as weapon_pickup_200.lua but for
-- `Rnd`: HARD's longer title route leaves it at 58231 when play begins.
-- weapon_pickup_200: where --warp 200:-8950:-1800:-700:3072
-- leaves the player, before the counter's handgun rounds, with room 200's
-- entry scene seen (flag 4:6) and one round in the handgun (--give 0:2:1;
-- oracle/re2.md, "Weapons").
local api = ...
api:set_player_pos(-8950, -1800, -700)
api:set_player_angle(3072)
api:set_health(200)
api:set_re2_rnd(58231)
api:set_room_camera(0)
api:set_re2_flag(4, 6)
api:set_item_slot(0, 0x02, 1)
