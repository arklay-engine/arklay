-- arklay's start for g3_shotgun_509: the state of the original's tick 344 after
-- --warp 509:-3670:0:-14172:2048:0 --flag 1:1 --flag 4:99 --life 2000, with the shotgun
-- and Leon's other weapons (oracle/re2.md, "G3").
local api = ...
api:set_player_pos(-3670, 0, -14172)
api:set_player_angle(2048)
api:set_health(2000)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_re2_flag(1, 1)
api:set_re2_flag(4, 99)
api:set_item_slot(0, 0x07, 200)
api:set_item_slot(1, 0x02, 15)
api:set_item_slot(2, 0x05, 6)
api:set_item_slot(3, 0x15, 30)
api:set_equipped(0x07)
