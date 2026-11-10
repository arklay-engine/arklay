-- arklay's start for the status_file_easy run: status_file_reader.lua's, on
-- EASY (the run's `difficulty`), where the new game puts the hint file in
-- slot 0, plus the developed films A and D the original's recording gives
-- with --file 1:0xB --file 2:0x13 (oracle/re2.md, "Status screen").
local api = ...
api:set_player_pos(-11378, 0, -4587)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_re2_flag(7, 0, true)
api:set_re2_file(1, 0x0B)
api:set_re2_file(2, 0x13)
