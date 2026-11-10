-- RE1, Jill: the Bad ending. Jill climbs out of 20C's pit before Barry is
-- back with the rope, so he is lost in the lab, and Chris is left in his
-- cell: Jill leaves on the helicopter alone.
--
--   arklay script [--headless] mods/bot/routes/re1_jill_bad_ending.lua [PACK [SEED [STATS.json]]]
--   arklay script --headless - PACK < mods/bot/routes/re1_jill_bad_ending.lua
--
-- See mods/bot/routes/README.md for the arguments and the result line.
load_game_pack(arg[1] or "games/re1.akpak", { mods = { "mods/bot" }, quick_turn = true, tactical_reload = true })
new_game({ character = 1 })
local run = bot_run({
    params = { rope = "leave", chris = "leave" },
    seed = tonumber(arg[2]),
    stats = arg[3],
    max_ticks = 400000,
})
local missed = {}
for _, check in ipairs({
    { "ending 2: Jill alone", run.score.ending == 2 },
    { "common flag 0xC0, Barry gone", bot_story(1, 0xC0) },
}) do
    if not check[2] then
        missed[#missed + 1] = check[1]
    end
end
print(json({ route = "re1_jill_bad_ending", outcome = run.outcome, reason = not run.score.completed and run.finish_reason or nil, missed = missed, score = run.score }))
if #missed > 0 then
    exit(1)
end
