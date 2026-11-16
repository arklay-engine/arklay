-- arklay's start for em2d_shots_205: the state of the original's tick 344
-- after --warp 205:-16798:0:-10671:3136:4 --flag 4:48 --life 200
-- --give 0:2:200 --give 1:0x14:60 --equip 2 (oracle/re2.md, "Em_2D").
local api = ...
api:set_player_pos(-16798, 0, -10671)
api:set_player_angle(3136)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(4)
api:set_re2_flag(4, 48)
api:set_item_slot(0, 0x02, 200)
api:set_item_slot(1, 0x14, 60)
api:set_equipped(0x02)
