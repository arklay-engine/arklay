-- arklay's start for the item_*_102 runs from a HARD ARRANGE MODE new game: where
-- --warp 102:-11378:0:-4587:0 leaves the player (oracle/re2.md, "Items").
-- That new game leaves `Rnd` at 9202 for the warp's door.
local api = ...
api:set_player_pos(-11378, 0, -4587)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(9202)
api:set_room_camera(0)
