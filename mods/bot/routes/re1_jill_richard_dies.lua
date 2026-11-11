-- RE1, Jill: Richard dies. Ending 7: Jill, Chris and Barry.
--
-- richard=leave: Richard gets no serum and dies in 20D; the door to Yawn
--   opens when his time runs out.
--
-- These parameters decide whether Barry and Chris reach the helicopter, so
-- they fix the ending:
--   rope=wait: Jill waits in 20C's pit until Barry is back with the rope.
--   cave_q1=yes cave_q2=yes: in 308 Jill goes with Barry and lets him go
--     first.
--   cave_exit=follow: Jill leaves 308 after him by door 1 (the other doors
--     are held shut while he is ahead): Barry's branch.
--   chris=save: Chris is freed from 512's cell before the escape lift.
-- The other parameters keep their defaults: none of them changes who escapes.
--
--   arklay script [--headless] mods/bot/routes/re1_jill_richard_dies.lua [PACK [SEED [STATS.json]]]
--   arklay script --headless - PACK < mods/bot/routes/re1_jill_richard_dies.lua
--
-- See mods/bot/routes/README.md for the arguments and the result line.
load_game_pack(arg[1] or "re1", { mods = { "mods/bot" }, quick_turn = true, tactical_reload = true })
new_game({ character = 1 })
local run = bot_run({
    params = {
        richard = "leave",
        rope = "wait",
        cave_q1 = "yes",
        cave_q2 = "yes",
        cave_exit = "follow",
        chris = "save",
    },
    seed = tonumber(arg[2]),
    stats = arg[3],
    max_ticks = 400000,
})
local missed = {}
for _, check in ipairs({
    { "ending 7: Jill, Chris and Barry", run.score.ending == 7 },
    { "common flag 0x64, 20D: Richard dead", bot_story(1, 0x64) },
}) do
    if not check[2] then
        missed[#missed + 1] = check[1]
    end
end
print(json({ route = "re1_jill_richard_dies", outcome = run.outcome, reason = not run.score.completed and run.finish_reason or nil, missed = missed, score = run.score }))
if #missed > 0 then
    exit(1)
end
