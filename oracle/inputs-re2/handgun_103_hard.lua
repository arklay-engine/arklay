-- arklay's start for handgun_103_hard: the state of the original's tick 344
-- after --difficulty hard --warp 103:2000:0:-14478:2048:3 --life 2000
-- --give 0:2:18 --give 2:0x14:250, item_crouch_103's start beside Brad with a
-- full handgun and 250 rounds spare. HARD's title route leaves Rnd at 58231
-- (oracle/re2.md, "The new-game choice").
local api = ...
api:set_player_pos(2000, 0, -14478)
api:set_player_angle(2048)
api:set_health(2000)
api:set_re2_rnd(58231)
api:set_room_camera(3)
api:set_item_slot(0, 0x02, 18)
api:set_item_slot(2, 0x14, 250)
