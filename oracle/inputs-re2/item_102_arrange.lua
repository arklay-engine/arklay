-- arklay's start for the item_*_102 runs from an ARRANGE MODE new game: where
-- --warp 102:-11378:0:-4587:0 leaves the player (oracle/re2.md, "Items").
-- That new game leaves `Rnd` at 52295 for the warp's door.
local api = ...
api:set_player_pos(-11378, 0, -4587)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(52295)
api:set_room_camera(0)
