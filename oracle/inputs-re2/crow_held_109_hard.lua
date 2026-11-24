-- arklay's start for crow_held_109_hard: where a HARD new game's --warp 109:-14000:0:-14800:1024 --equip 7 --give 0:7:200
-- --give 1:0x14:60 --flag 1:1 leaves the player in scenario B's room 109, with its eight crows perched
-- (oracle/re2.md, "crow_held.txt").
local api = ...
api:set_player_pos(-14000, 0, -14800)
api:set_player_angle(1024)
api:set_health(200)
api:set_re2_rnd(58231)
api:set_item_slot(0, 0x07, 200)
api:set_item_slot(1, 0x14, 60)
api:set_equipped(0x07)
api:set_re2_flag(1, 1)
