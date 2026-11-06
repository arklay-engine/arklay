-- arklay's start for route_100_200.txt with the enemies on, as `record
-- --flag 4:135` has them: the new game in room 100 with room 102's gate flag
-- raised. `set_re2_flag` runs the new game's setup again, which draws
-- `Sce_rnd` once more, so the seed goes back to its value at the start of play.
local api = ...
api:set_re2_flag(4, 135)
api:set_re2_sce_rnd(0x138201C3)
