-- arklay's start for zombies_207_fight: zombies_207.lua's (where
-- --warp 207:-25640:0:-16590:0:1 leaves the player, at room 205's door 2
-- arrival in cut 1; oracle/re2.md, "Room 207's zombies"),
-- with the record's --give 0:2:13 --give 2:0x14:39: 13 rounds in the handgun
-- and 39 spare.
local api = ...
api:set_player_pos(-25640, 0, -16590)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(1)
api:set_item_slot(0, 0x02, 13)
api:set_item_slot(2, 0x14, 39)
