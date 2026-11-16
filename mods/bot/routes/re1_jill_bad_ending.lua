-- RE1, Jill: the Bad ending. Ending 2: Jill alone.
--
-- These parameters decide whether Barry and Chris reach the helicopter, so
-- they fix the ending:
--   rope=leave: Jill pushes the switch below 20C's hatch and goes down the
--     ladder to the basement (21A) before Barry is back with the rope: he
--     skips the caves and dies in the lab (502, "Tell my family").
--   chris=leave: Jill keeps off 50B's door to 512: Chris stays in his cell.
-- Without the rope Barry's 308 scene never plays, so the cave parameters do
-- nothing here; the other parameters keep their defaults, and none of them
-- changes who escapes.
--
--   arklay script [--headless] mods/bot/routes/re1_jill_bad_ending.lua [PACK [SEED [STATS.json]]]
--   arklay script --headless - PACK < mods/bot/routes/re1_jill_bad_ending.lua
--
-- See mods/bot/routes/README.md for the arguments and the result line.
load_game_pack(arg[1] or "re1", { mods = { "mods/bot" }, quick_turn = true, tactical_reload = true })
new_game({ character = 1 })
local run = bot_run({
    params = {
        rope = "leave",
        chris = "leave",
    },
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
