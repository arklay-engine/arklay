-- arklay's start for spider_302_wall: the state of the original's tick 344
-- after --warp 302:-12500:0:-13000:2048 --life 200 --give 0:2:200
-- --give 1:0x14:60 --equip 2 (oracle/re2.md, the spiders on walls and ceilings).
local api = ...
api:set_player_pos(-12500, 0, -13000)
api:set_player_angle(2048)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(0, 0x02, 200)
api:set_item_slot(1, 0x14, 60)
api:set_equipped(0x02)
