-- arklay's start for route_200_spade.txt (route_200_115.txt and route_200_208.txt before it): the new game in room 100
-- as it starts, with the enemies on and no flags raised, as a plain `record` has it.
local api = ...
-- Room 207's slot 2 starts with the Dest its work's memory holds in the original
-- (docs/internals/deviations/re2.md, "What a zombie's slot held before"), as in
-- leon_a_207.lua; the route's reference has it on the room's first tick (`zd`).
api:set_re2_enemy_dest("207:2", -5586, 128)
-- Room 114's slot 2 likewise, once for each visit, in order: 8481, 9766 on the way to the
-- S.T.A.R.S. office (which no zombie walks toward before `Root_ck` gives it the player), then
-- -32768, -32768 on the way back, as in leon_a_115_207.lua; the reference has both (`zd`).
api:set_re2_enemy_dest("114:2", 8481, 9766)
api:set_re2_enemy_dest("114:2", -32768, -32768)
