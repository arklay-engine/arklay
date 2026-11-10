-- arklay's start for the status_file_easy_claire run: Claire in room 102 on
-- EASY (the run's `difficulty`), where the new game puts the hint file in
-- slot 0, with the flags the original's recording raises with --flag 4:15
-- --flag 9:64, which grow the hint file to its fourth page (oracle/re2.md,
-- "Status screen").
local api = ...
api:set_player_pos(-11378, 0, -4587)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_re2_flag(7, 0, true)
api:set_re2_flag(4, 15, true)
api:set_re2_flag(9, 64, true)
