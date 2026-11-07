-- arklay's start for swarm_stand_40b: the state of the original's tick 344
-- after --warp 40B:-7824:0:-25140:2000:0 --life 200 (oracle/re2.md). The
-- original's `Pl_neck` first looks on tick 386, one tick before arklay's
-- count from a room's start would.
local api = ...
api:set_player_pos(-7824, 0, -25140)
api:set_player_angle(2000)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_re2_neck_wait(41)
