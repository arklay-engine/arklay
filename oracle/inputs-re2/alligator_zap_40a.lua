-- arklay's start for alligator_zap_40a: the state of the original's tick 344
-- after --warp 40A:2000:-1800:-23000:1128:10 --life 200 --equip 5
-- --give 0:5:200 --flag 1:1 --flag 29:16: scenario B with fg_zapping (oracle/re2.md).
local api = ...
api:set_player_pos(2000, -1800, -23000)
api:set_player_angle(1128)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(10)
api:set_item_slot(0, 0x05, 200)
api:set_equipped(0x05)
api:set_re2_flag(1, 1)
api:set_re2_flag(29, 16)
