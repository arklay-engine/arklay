-- arklay's start for the file_pickup_repeat_201 run: where --warp leaves the player on the
-- original's tick 344, at the file's area and facing it, with Leon's lighter in
-- the personal slot (oracle/re2.md, "Picking up a file").
local api = ...
api:set_player_pos(-19000, 0, -24850)
api:set_player_angle(1024)
api:set_health(200)
api:set_re2_rnd(12049)
api:set_room_camera(0)
api:set_item_slot(2, 0)
api:set_personal_item(0x2F)
-- --file 0:0: Chris's Diary is in slot 0, so the memo lands in slot 1.
api:set_re2_file(0, 0)
-- the original's repeat counter at tick 343 (the trace's prep)
api:set_re2_pad_repeat(8)
