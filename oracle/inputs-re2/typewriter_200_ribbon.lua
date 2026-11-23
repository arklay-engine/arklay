-- arklay's start for typewriter_200_no: typewriter_200.lua's, with three ink
-- ribbons in slot 2 (--give 2:0x1E:3).
local api = ...
api:set_player_pos(-12200, -1800, -1900)
api:set_player_angle(1024)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_re2_flag(4, 6)
api:set_item_slot(2, 0x1E, 3)
-- What the original's record keeps from its new game's start in room 100
-- before the warp: the room's visit and floor on the map (9:0, 35:0) and
-- `Set_door`'s last room (variable 27), which its saves hold.
api:set_re2_flag(9, 0)
api:set_re2_flag(35, 0)
api:set_re2_var(27, 0x100)
