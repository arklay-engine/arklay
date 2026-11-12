-- arklay's start for colt_licker_204: licker_204_fight's (--warp
-- 204:-15400:0:-8000:3072 --flag 4:85 --life 2000) with the Colt S.A.A. and
-- its rounds: --equip-load 0xD --give 0:0xD:6 --give 1:0x14:200.
local api = ...
api:set_player_pos(-15400, 0, -8000)
api:set_player_angle(3072)
api:set_health(2000)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_re2_flag(4, 85)
api:set_item_slot(0, 0x0D, 6)
api:set_item_slot(1, 0x14, 200)
api:set_equipped(0x0D)
