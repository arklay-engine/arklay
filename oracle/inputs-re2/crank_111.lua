-- arklay's start for crank_111: where --warp 111:-23154:0:-16272:3072 leaves the player, in
-- the Crank's area (7), with the Crank in slot 2 (oracle/re2.md, "Using a key item").
local api = ...
api:set_player_pos(-23154, 0, -16954)
api:set_player_angle(3072)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(0, 0x02, 13)
api:set_item_slot(1, 0x01, 1)
api:set_item_slot(2, 0x43, 1)
