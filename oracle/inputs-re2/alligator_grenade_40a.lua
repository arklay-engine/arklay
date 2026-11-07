-- arklay's start for alligator_grenade_40a: the state of the original's tick
-- 344 after --warp 40A:2000:-1800:-23000:1128:10 --life 200 --equip-load 9
-- --give 0:9:30, as alligator_face_40a's with the grenade launcher
-- (oracle/re2.md).
local api = ...
api:set_player_pos(2000, -1800, -23000)
api:set_player_angle(1128)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(10)
api:set_item_slot(0, 0x09, 30)
api:set_equipped(0x09)
