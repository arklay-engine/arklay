-- arklay's start for the file_pickup_208 run: where --warp leaves the player on the
-- original's tick 344, at the file's area and facing it, with Leon's lighter in
-- the personal slot (oracle/re2.md, "Picking up a file").
local api = ...
api:set_player_pos(-19000, 0, -19400)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(1)
api:set_item_slot(2, 0)
api:set_personal_item(0x2F)
