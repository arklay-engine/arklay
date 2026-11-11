-- RE1, Chris: the Bad ending. Ending 1: Chris alone.
--
-- These parameters decide whether Rebecca and Jill reach the helicopter, so
-- they fix the ending:
--   rebecca_meet=richard: Chris finds Richard in 20D before the save room;
--     Rebecca is with him.
--   richard=leave: Richard gets no serum and dies, which arms Rebecca's
--     upstairs call and 206's hunter attack.
--   rebecca=leave: Chris keeps out of 206 after Rebecca's upstairs call, and
--     walks out if he is there during the attack, so the hunter kills her.
--   jill=leave: Chris keeps off 50B's door to 512: Jill stays in her cell.
-- Here rebecca_along does nothing after Richard is found first. Yawn's bite,
-- Rebecca's serum run after it and Plant 42's V-JOLT follow from the game
-- state, not from a parameter. The other parameters keep their defaults: none
-- of them changes who escapes.
--
--   arklay script [--headless] mods/bot/routes/re1_chris_alone.lua [PACK [SEED [STATS.json]]]
--   arklay script --headless - PACK < mods/bot/routes/re1_chris_alone.lua
--
-- See mods/bot/routes/README.md for the arguments and the result line.
load_game_pack(arg[1] or "re1", { mods = { "mods/bot" }, quick_turn = true, tactical_reload = true })
new_game({ character = 0 })
local run = bot_run({
    params = {
        rebecca_meet = "richard",
        richard = "leave",
        rebecca = "leave",
        jill = "leave",
    },
    seed = tonumber(arg[2]),
    stats = arg[3],
    max_ticks = 400000,
})
local missed = {}
for _, check in ipairs({
    { "ending 1: Chris alone", run.score.ending == 1 },
    { "common flag 0xC0, Rebecca lost", bot_story(1, 0xC0) },
}) do
    if not check[2] then
        missed[#missed + 1] = check[1]
    end
end
print(json({ route = "re1_chris_alone", outcome = run.outcome, reason = not run.score.completed and run.finish_reason or nil, missed = missed, score = run.score }))
if #missed > 0 then
    exit(1)
end
