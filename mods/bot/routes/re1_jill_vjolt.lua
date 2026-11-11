-- RE1, Jill: the guardhouse's number panel is solved, V-JOLT is mixed in
-- 409's drug store and poured on Plant 42's root, so the plant in 40C is
-- fought weakened and without Barry's rescue. The Best ending.
--
--   arklay script [--headless] mods/bot/routes/re1_jill_vjolt.lua [PACK [SEED [STATS.json]]]
--   arklay script --headless - PACK < mods/bot/routes/re1_jill_vjolt.lua
--
-- See mods/bot/routes/README.md for the arguments and the result line.
load_game_pack(arg[1] or "re1", { mods = { "mods/bot" }, quick_turn = true, tactical_reload = true })
new_game({ character = 1 })
local run = bot_run({
    params = { vjolt = "yes" },
    seed = tonumber(arg[2]),
    stats = arg[3],
    max_ticks = 400000,
})
local missed = {}
for _, check in ipairs({
    { "ending 7: Jill, Chris and Barry", run.score.ending == 7 },
    { "scenario flag 0x21, 408: the number panel (door to 409)", bot_story(0, 0x21) },
    { "scenario flag 0x13, 40F: V-JOLT poured", bot_story(0, 0x13) },
}) do
    if not check[2] then
        missed[#missed + 1] = check[1]
    end
end
print(json({ route = "re1_jill_vjolt", outcome = run.outcome, reason = not run.score.completed and run.finish_reason or nil, missed = missed, score = run.score }))
if #missed > 0 then
    exit(1)
end
