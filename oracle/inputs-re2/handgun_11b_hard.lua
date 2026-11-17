-- arklay's start for handgun_11b_hard: the state of the original's tick 344
-- after --difficulty hard --warp 11B:8000:0:-19500:0 --life 2000
-- --give 0:2:18 --give 2:0x14:250, stand_11b_hard's with 2000 life, a full
-- handgun and 250 rounds spare. HARD's title route leaves Rnd at 58231
-- (oracle/re2.md, "The new-game choice").
local api = ...
api:set_player_pos(8000, 0, -19500)
api:set_player_angle(0)
api:set_health(2000)
api:set_re2_rnd(58231)
api:set_room_camera(0)
api:set_item_slot(0, 0x02, 18)
api:set_item_slot(2, 0x14, 250)
