-- arklay's start for licker24_fight: the original's
-- --warp 605:-24500:0:-24000:0 --give 0:2:200 --give 1:0x14:60 --equip 2
-- --life 200, compared from its tick 344 (oracle/re2.md, the lickers of room
-- 605): the place of the warp, the life, the weapon and `Rnd`.
local api = ...
api:set_player_pos(-24500, 0, -24000)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(0, 0x02, 200)
api:set_item_slot(1, 0x14, 60)
api:set_equipped(0x02)
