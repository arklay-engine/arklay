-- RE1, Jill: the Chris ending: Jill backtracks from Barry. Ending 3: Jill and Chris.
--
-- These parameters decide whether Barry and Chris reach the helicopter, so
-- they fix the ending:
--   rope=wait: Jill waits in 20C's pit until Barry is back with the rope.
--   cave_q1=yes cave_q2=yes: in 308 Jill goes with Barry and lets him go
--     first.
--   cave_exit=backtrack: Jill keeps off door 1 behind him and goes back:
--     Barry dies in 307 ("Tell my family", cave).
--   chris=save: Chris is freed from 512's cell before the escape lift.
-- The other parameters keep their defaults: none of them changes who escapes.
--
--   arklay script [--headless] mods/bot/routes/re1_jill_cave_backtrack.lua [PACK [SEED [STATS.json]]]
--   arklay script --headless - PACK < mods/bot/routes/re1_jill_cave_backtrack.lua
--
-- See mods/bot/routes/README.md for the arguments and the result line.
load_game_pack(arg[1] or "re1", { mods = { "mods/bot" }, quick_turn = true, tactical_reload = true })
new_game({ character = 1 })
local run = bot_run({
    params = {
        rope = "wait",
        cave_q1 = "yes",
        cave_q2 = "yes",
        cave_exit = "backtrack",
        chris = "save",
    },
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
