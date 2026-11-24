-- arklay's start for the file_photo_208 run: where --warp leaves the player on the
-- original's tick 344, west of the darkroom's trays and facing them, with the film
-- (0x44) in slot 2 and Leon's lighter in the personal slot (oracle/re2.md,
-- "Picking up a file").
local api = ...
api:set_player_pos(-15505, 0, -23800)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(2, 0x44, 1)
api:set_personal_item(0x2F)
