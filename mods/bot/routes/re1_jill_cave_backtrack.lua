-- RE1, Jill: in the caves Barry goes ahead and Jill does not follow him but
-- backtracks, so she finds him dying in 307 ("Tell my family"); Chris is
-- freed: the Chris ending.
--
--   arklay script [--headless] mods/bot/routes/re1_jill_cave_backtrack.lua [PACK [SEED [STATS.json]]]
--   arklay script --headless - PACK < mods/bot/routes/re1_jill_cave_backtrack.lua
--
-- See mods/bot/routes/README.md for the arguments and the result line.
load_game_pack(arg[1] or "games/re1.akpak", { mods = { "mods/bot" }, quick_turn = true, tactical_reload = true })
new_game({ character = 1 })
local run = bot_run({
    params = { cave_exit = "backtrack" },
    seed = tonumber(arg[2]),
    stats = arg[3],
    max_ticks = 400000,
})
local missed = {}
for _, check in ipairs({
    { "ending 3: Jill and Chris", run.score.ending == 3 },
    { "common flag 0x55, 307: Tell my family (cave)", bot_story(1, 0x55) },
}) do
    if not check[2] then
        missed[#missed + 1] = check[1]
    end
end
print(json({ route = "re1_jill_cave_backtrack", outcome = run.outcome, reason = not run.score.completed and run.finish_reason or nil, missed = missed, score = run.score }))
if #missed > 0 then
    exit(1)
end
