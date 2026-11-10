-- RE1, Jill: in the caves Jill goes with Barry but not behind him (Yes, No),
-- so he walks with her: Barry's branch. The Best ending.
--
--   arklay script [--headless] mods/bot/routes/re1_jill_barry_with_jill.lua [PACK [SEED [STATS.json]]]
--   arklay script --headless - PACK < mods/bot/routes/re1_jill_barry_with_jill.lua
--
-- See mods/bot/routes/README.md for the arguments and the result line.
load_game_pack(arg[1] or "games/re1.akpak", { mods = { "mods/bot" }, quick_turn = true, tactical_reload = true })
new_game({ character = 1 })
local run = bot_run({
    params = { cave_q2 = "no" },
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
