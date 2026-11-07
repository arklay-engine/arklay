-- arklay's start for g1_magnum_303: the state of the original's tick 344 after
-- --warp 303:-16852:-5400:-19090:552:3 --flag 1:1 --flag 4:55 --life 200, with
-- Leon's weapons for room 303 in scenario B (oracle/re2.md, "G1").
local api = ...
api:set_player_pos(-16852, -5400, -19090)
api:set_player_angle(552)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(3)
api:set_re2_flag(1, 1)
api:set_re2_flag(4, 55)
api:set_item_slot(0, 0x05, 200)
api:set_item_slot(1, 0x02, 15)
api:set_item_slot(2, 0x07, 5)
api:set_item_slot(3, 0x16, 30)
api:set_equipped(0x05)
