-- RE1, Chris: Richard gets the serum. Ending 6: Chris, Jill and Rebecca.
--
-- These parameters decide whether Rebecca and Jill reach the helicopter, so
-- they fix the ending:
--   rebecca_meet=richard: Chris finds Richard in 20D before the save room;
--     Rebecca is with him.
--   richard=save: Chris brings Richard the serum from 100 within seven
--     minutes, and Rebecca stays to tend him.
--   rebecca=save: Chris answers every hunter attack on Rebecca (101, and 206
--     after the upstairs call a slow serum run or Richard's death arms) and
--     kills the hunter there, so she lives.
--   jill=save: Jill is freed from 512's cell before the escape lift.
-- Here rebecca_along does nothing after Richard is found first. Yawn's bite,
-- Rebecca's serum run after it and Plant 42's V-JOLT follow from the game
-- state, not from a parameter. The other parameters keep their defaults: none
-- of them changes who escapes.
--
--   arklay script [--headless] mods/bot/routes/re1_chris_richard_saved.lua [PACK [SEED [STATS.json]]]
--   arklay script --headless - PACK < mods/bot/routes/re1_chris_richard_saved.lua
--
-- See mods/bot/routes/README.md for the arguments and the result line.
load_game_pack(arg[1] or "re1", { mods = { "mods/bot" }, quick_turn = true, tactical_reload = true })
new_game({ character = 0 })
local run = bot_run({
    params = {
        rebecca_meet = "richard",
        richard = "save",
        rebecca = "save",
        jill = "save",
    },
    seed = tonumber(arg[2]),
    stats = arg[3],
    max_ticks = 600000,
})
local missed = {}
for _, check in ipairs({
    { "ending 6: Chris, Jill and Rebecca", run.score.ending == 6 },
    { "common flag 0x63, 20D: Richard saved", bot_story(1, 0x63) },
}) do
    if not check[2] then
        missed[#missed + 1] = check[1]
    end
end
print(json({ route = "re1_chris_richard_saved", outcome = run.outcome, reason = not run.score.completed and run.finish_reason or nil, missed = missed, score = run.score }))
if #missed > 0 then
    exit(1)
end
