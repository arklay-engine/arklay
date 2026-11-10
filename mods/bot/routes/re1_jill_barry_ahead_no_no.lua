-- RE1, Jill: in the caves Jill answers No, No to Barry, which sends him ahead
-- as Yes, Yes does, and follows him. The Best ending.
--
--   arklay script [--headless] mods/bot/routes/re1_jill_barry_ahead_no_no.lua [PACK [SEED [STATS.json]]]
--   arklay script --headless - PACK < mods/bot/routes/re1_jill_barry_ahead_no_no.lua
--
-- See mods/bot/routes/README.md for the arguments and the result line.
load_game_pack(arg[1] or "games/re1.akpak", { mods = { "mods/bot" }, quick_turn = true, tactical_reload = true })
new_game({ character = 1 })
local run = bot_run({
    params = { cave_q1 = "no", cave_q2 = "no" },
    seed = tonumber(arg[2]),
    stats = arg[3],
    max_ticks = 400000,
})
local missed = {}
for _, check in ipairs({
    { "ending 7: Jill, Chris and Barry", run.score.ending == 7 },
    { "common flag 0x5C, 308: Barry goes ahead (Yes, Yes or No, No)", bot_story(1, 0x5C) },
}) do
    if not check[2] then
        missed[#missed + 1] = check[1]
    end
end
print(json({ route = "re1_jill_barry_ahead_no_no", outcome = run.outcome, reason = not run.score.completed and run.finish_reason or nil, missed = missed, score = run.score }))
if #missed > 0 then
    exit(1)
end
