-- arklay's start for To-fu's results (oracle/re2-regress.d/result.toml), as
-- `record --flag 0:28 --flag 0:31 --flag 1:6 --flag 0:10 --var 35:330` has
-- them: result_4th.lua's, with To-fu's `fg_system` 1.
local api = ...
api:set_re2_flag(0, 28)
api:set_re2_flag(1, 6)
api:set_re2_flag(0, 31)
api:set_re2_flag(0, 10)
api:set_re2_var(35, 330)
api:set_re2_sce_rnd(0x138201C3)
