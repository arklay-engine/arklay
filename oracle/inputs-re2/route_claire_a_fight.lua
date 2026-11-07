-- arklay's start for route_claire_a_fight.txt: Claire's new game in room 100 as it starts,
-- with the enemies on and no flags raised, as a plain `record --start claire` has it.
-- Room 102's zombies 2 and 3 start with the `Dest` the original's new works hold
-- (docs/re2-deviations.md, "What a zombie's slot held before").
local api = ...
api:set_re2_enemy_dest("102:2", 224, 28708)
api:set_re2_enemy_dest("102:3", -1740, 16814)
