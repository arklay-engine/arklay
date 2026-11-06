-- arklay's start for route_100_200.txt: the new game in room 100, with the
-- enemies the original's run removes switched off (all but Kendo, room 101's
-- slot 0) and room 102's gate flag raised, as `record --no-enemies
-- --keep-enemy-in 101:0 --flag 4:135` has them. `set_re2_flag` runs the new
-- game's setup again, which draws `Sce_rnd` once more, so the seed goes back
-- to its value at the start of play.
local api = ...
api:set_re2_no_enemies({"101:0"})
api:set_re2_flag(4, 135)
api:set_re2_sce_rnd(0x138201C3)
