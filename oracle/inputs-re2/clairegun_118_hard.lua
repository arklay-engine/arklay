-- arklay's start for clairegun_118_hard: the state of the original's tick 344
-- after --start claire --difficulty hard --warp 118:-24617:0:-24996:1024
-- --life 2000 --give 2:0x14:250, bowgun_118's start on HARD with 2000 life,
-- Claire's own handgun and 250 rounds spare. HARD's title route leaves Rnd at
-- 58231 (oracle/re2.md, "The new-game choice").
local api = ...
api:set_player_pos(-24617, 0, -24996)
api:set_player_angle(1024)
api:set_health(2000)
api:set_re2_rnd(58231)
api:set_room_camera(0)
api:set_item_slot(0, 0x03, 13)
api:set_item_slot(1, 0x01, 1)
api:set_item_slot(2, 0x14, 250)
api:set_equipped(0x03)
