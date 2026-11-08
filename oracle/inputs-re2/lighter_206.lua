-- arklay's start for lighter_206: where --warp 206:-22050:0:-25600:1024:0 leaves the player,
-- facing the fireplace with the probe in the Lighter's area (2) (oracle/re2.md, "Using a key item").
local api = ...
api:set_player_pos(-22050, 0, -25600)
api:set_player_angle(1024)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(0, 0x02, 13)
api:set_item_slot(1, 0x01, 1)
api:set_item_slot(2, 0x14, 20)
api:set_item_slot(3, 0x1E, 3)
api:set_item_slot(4, 0x2F, 1)
