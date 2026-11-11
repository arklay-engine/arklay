-- RE1, Jill: Jill climbs out of 20C's pit before Barry is back with the rope,
-- so he skips the caves and is lost in the lab ("Tell my family"); Chris is
-- freed: the Chris ending.
--
--   arklay script [--headless] mods/bot/routes/re1_jill_rope_leave.lua [PACK [SEED [STATS.json]]]
--   arklay script --headless - PACK < mods/bot/routes/re1_jill_rope_leave.lua
--
-- See mods/bot/routes/README.md for the arguments and the result line.
load_game_pack(arg[1] or "re1", { mods = { "mods/bot" }, quick_turn = true, tactical_reload = true })
new_game({ character = 1 })
local run = bot_run({
    params = { rope = "leave" },
    seed = tonumber(arg[2]),
    stats = arg[3],
    max_ticks = 400000,
})
local missed = {}
for _, check in ipairs({
    { "ending 3: Jill and Chris", run.score.ending == 3 },
    { "common flag 0x54, 502: Tell my family (lab)", bot_story(1, 0x54) },
    { "common flag 0xC0, Barry gone", bot_story(1, 0xC0) },
}) do
    if not check[2] then
        missed[#missed + 1] = check[1]
    end
end
print(json({ route = "re1_jill_rope_leave", outcome = run.outcome, reason = not run.score.completed and run.finish_reason or nil, missed = missed, score = run.score }))
if #missed > 0 then
    exit(1)
end
