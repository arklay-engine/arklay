-- RE2's new-game choice. The original has no screen of its own for it: it is
-- the title's rows below ORIGINAL MODE (`ui/title.lua`, `0x504240` and
-- `0x5044D0`). An engine boot that asks for the choice (`--ui select`) opens
-- the title there, faded in from black, as if ORIGINAL MODE had just been
-- confirmed.

local Title = require("ui/title")

local M = {}

function M.open(api, args)
    args = args or {}
    args.choice = true
    Title.open(api, args)
    M.state = Title.state
end

M.update = Title.update
M.draw = Title.draw

return M
