-- arklay's start for the extras' results (oracle/re2-regress.d/result.toml), as
-- `record --flag 0:28 --flag 1:6 --flag 0:10 --var 35:330` has them: the new
-- game in room 100 with `fg_system`'s 8 (an extra mode's game), `fg_status`'s
-- `0x2000000` (flag 1:6) and `fg_system`'s `0x200000` (room 11C's
-- `set(0, 10, 1)`, the goal), and the extra's clear time, script variable 35,
-- at 330 seconds. `Game_loop` chains `Result` on the first tick.
-- `set_re2_flag` runs the new game's setup again, which draws `Sce_rnd` once
-- more, so the seed goes back to its value at the start of play.
local api = ...
api:set_re2_flag(0, 28)
api:set_re2_flag(1, 6)
api:set_re2_flag(0, 10)
api:set_re2_var(35, 330)
api:set_re2_sce_rnd(0x138201C3)
