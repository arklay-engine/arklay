-- RE1, Chris: the Rebecca ending. Ending 4: Chris and Rebecca.
--
-- These parameters decide whether Rebecca and Jill reach the helicopter, so
-- they fix the ending:
--   rebecca_meet=save_room: Chris meets Rebecca in the save room (100) before
--     Richard in 20D, who is then found dead.
--   rebecca_along=yes: Chris takes Rebecca along from 100, so no hunter is
--     sent after her there.
--   rebecca=save: Chris answers every hunter attack on Rebecca (101, and 206
--     after the upstairs call a slow serum run or Richard's death arms) and
--     kills the hunter there, so she lives.
--   jill=leave: Chris keeps off 50B's door to 512: Jill stays in her cell.
-- Here richard does nothing once Rebecca is met first (he is found dead).
-- Yawn's bite, Rebecca's serum run after it and Plant 42's V-JOLT follow from
-- the game state, not from a parameter. The other parameters keep their
-- defaults: none of them changes who escapes.
--
--   arklay script [--headless] mods/bot/routes/re1_chris_rebecca_ending.lua [PACK [SEED [STATS.json]]]
--   arklay script --headless - PACK < mods/bot/routes/re1_chris_rebecca_ending.lua
--
-- See mods/bot/routes/README.md for the arguments and the result line.
load_game_pack(arg[1] or "re1", { mods = { "mods/bot" }, quick_turn = true, tactical_reload = true })
new_game({ character = 0 })
local run = bot_run({
    params = {
        rebecca_meet = "save_room",
        rebecca_along = "yes",
        rebecca = "save",
        jill = "leave",
    },
    seed = tonumber(arg[2]),
    stats = arg[3],
    max_ticks = 400000,
})
local missed = {}
for _, check in ipairs({
    { "ending 4: Chris and Rebecca", run.score.ending == 4 },
}) do
    if not check[2] then
        missed[#missed + 1] = check[1]
    end
end
print(json({ route = "re1_chris_rebecca_ending", outcome = run.outcome, reason = not run.score.completed and run.finish_reason or nil, missed = missed, score = run.score }))
if #missed > 0 then
    exit(1)
end
