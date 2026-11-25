-- arklay's start for the main game's results
-- (oracle/re2-regress.d/result.toml), as `record --flag 0:10` has them: the
-- new game in room 100 with `fg_system`'s `0x200000` (room 11C's `set(0, 10,
-- 1)`, the goal): Leon A's results. `Game_loop` chains `Result` on the first
-- tick. `set_re2_flag` runs the new game's setup again, which draws `Sce_rnd`
-- once more, so the seed goes back to its value at the start of play.
local api = ...
api:set_re2_flag(0, 10)
api:set_re2_sce_rnd(0x138201C3)
