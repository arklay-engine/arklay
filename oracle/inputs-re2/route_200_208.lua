-- arklay's start for route_200_208.txt: the new game in room 100 as it starts, with
-- the enemies on and no flags raised, as a plain `record` has it.
local api = ...
-- Room 207's slot 2 starts with the Dest its work's memory holds in the original
-- (docs/internals/deviations/re2.md, "What a zombie's slot held before"), as in
-- leon_a_207.lua; the route's reference has it on the room's first tick (`zd`).
api:set_re2_enemy_dest("207:2", -5586, 128)
