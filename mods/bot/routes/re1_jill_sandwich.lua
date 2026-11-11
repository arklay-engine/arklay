-- RE1, Jill: the shotgun is lifted from 116's rack without the broken one, so
-- 115's ceiling comes down and Barry kicks the door in ("Jill Sandwich").
-- The Best ending.
--
--   arklay script [--headless] mods/bot/routes/re1_jill_sandwich.lua [PACK [SEED [STATS.json]]]
--   arklay script --headless - PACK < mods/bot/routes/re1_jill_sandwich.lua
--
-- See mods/bot/routes/README.md for the arguments and the result line.
load_game_pack(arg[1] or "re1", { mods = { "mods/bot" }, quick_turn = true, tactical_reload = true })
new_game({ character = 1 })
local run = bot_run({
    params = { sandwich = "yes" },
    seed = tonumber(arg[2]),
    stats = arg[3],
    max_ticks = 400000,
})
local missed = {}
for _, check in ipairs({
    { "ending 7: Jill, Chris and Barry", run.score.ending == 7 },
    { "common flag 0x20, 115: Jill Sandwich, Barry's rescue", bot_story(1, 0x20) },
}) do
    if not check[2] then
        missed[#missed + 1] = check[1]
    end
end
print(json({ route = "re1_jill_sandwich", outcome = run.outcome, reason = not run.score.completed and run.finish_reason or nil, missed = missed, score = run.score }))
if #missed > 0 then
    exit(1)
end
