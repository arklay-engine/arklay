-- arklay's start for route_claire_a_208.txt: Claire's new game in room 100 as it
-- starts, with the enemies on and no flags raised, as a plain `record --start
-- claire` has it. Room 102's zombies 2 and 3 and room 207's slot 2 start with
-- the `Dest` the original's works hold (docs/internals/deviations/re2.md, "What
-- a zombie's slot held before"), as in route_claire_a_card.lua and
-- leon_a_207.lua.
local api = ...
api:set_re2_enemy_dest("102:2", 224, 28708)
api:set_re2_enemy_dest("102:3", -1740, 16814)
api:set_re2_enemy_dest("207:2", -5586, 128)
