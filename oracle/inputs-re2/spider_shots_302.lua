-- arklay's start for spider_shots_302: the state of the original's tick 344
-- after --warp 302:-13000:0:-12000:0 --flag 1:6 --life 200 --give 0:2:200
-- --give 1:0x14:60 --equip 2 (oracle/re2.md, the floor spiders' shots).
local api = ...
api:set_player_pos(-13000, 0, -12000)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_re2_flag(1, 6)
api:set_item_slot(0, 0x02, 200)
api:set_item_slot(1, 0x14, 60)
api:set_equipped(0x02)
