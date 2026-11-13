-- arklay's start for spider_405_up_w5: the state of the original's tick 344
-- after --warp 405:-26394:0:-1920:97 --life 200 --give 0:5:200
-- --give 1:0x14:60 --equip 5 (oracle/re2.md, the spiders on walls and ceilings).
local api = ...
api:set_player_pos(-26394, 0, -1920)
api:set_player_angle(97)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(0, 0x05, 200)
api:set_item_slot(1, 0x14, 60)
api:set_equipped(0x05)
