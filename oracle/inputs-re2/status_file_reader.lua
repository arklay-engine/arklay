-- arklay's start for the status_file_reader run: status_102.lua's, plus the
-- files the original's recording gives with --file 0:1 --file 1:3 (binder 0
-- holds file 1 in slot 0 and file 3 in slot 1; oracle/re2.md, "Status screen").
local api = ...
api:set_player_pos(-11378, 0, -4587)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_re2_flag(7, 0, true)
api:set_re2_file(0, 1)
api:set_re2_file(1, 3)
