-- RE1, Jill: Barry walks with Jill in the caves. Ending 7: Jill, Chris and Barry.
--
-- These parameters decide whether Barry and Chris reach the helicopter, so
-- they fix the ending:
--   rope=wait: Jill waits in 20C's pit until Barry is back with the rope.
--   cave_q1=yes cave_q2=no: in 308 Jill goes with Barry and goes first: he
--     walks with her (Barry's branch).
--   chris=save: Chris is freed from 512's cell before the escape lift.
-- cave_exit does nothing after this answer, and the other parameters keep
-- their defaults: none of them changes who escapes.
--
--   arklay script [--headless] mods/bot/routes/re1_jill_barry_with_jill.lua [PACK [SEED [STATS.json]]]
--   arklay script --headless - PACK < mods/bot/routes/re1_jill_barry_with_jill.lua
--
-- See mods/bot/routes/README.md for the arguments and the result line.
load_game_pack(arg[1] or "re1", { mods = { "mods/bot" }, quick_turn = true, tactical_reload = true })
new_game({ character = 1 })
local run = bot_run({
    params = {
        rope = "wait",
        cave_q1 = "yes",
        cave_q2 = "no",
        chris = "save",
    },
    seed = tonumber(arg[2]),
    stats = arg[3],
    max_ticks = 400000,
})
local missed = {}
for _, check in ipairs({
    { "ending 7: Jill, Chris and Barry", run.score.ending == 7 },
    { "common flag 0x59, 308: Barry goes with Jill (Yes, No)", bot_story(1, 0x59) },
}) do
    if not check[2] then
        missed[#missed + 1] = check[1]
    end
end
print(json({ route = "re1_jill_barry_with_jill", outcome = run.outcome, reason = not run.score.completed and run.finish_reason or nil, missed = missed, score = run.score }))
if #missed > 0 then
    exit(1)
end
