-- RE1, Jill: the Chris ending: Barry goes off alone. Ending 3: Jill and Chris.
--
-- These parameters decide whether Barry and Chris reach the helicopter, so
-- they fix the ending:
--   rope=wait: Jill waits in 20C's pit until Barry is back with the rope.
--   cave_q1=no cave_q2=yes: in 308 Jill will not go with Barry and stays
--     behind: he goes off alone and dies in the lab (502, "Tell my family").
--   chris=save: Chris is freed from 512's cell before the escape lift.
-- cave_exit does nothing after this answer, and the other parameters keep
-- their defaults: none of them changes who escapes.
--
--   arklay script [--headless] mods/bot/routes/re1_jill_chris_ending.lua [PACK [SEED [STATS.json]]]
--   arklay script --headless - PACK < mods/bot/routes/re1_jill_chris_ending.lua
--
-- See mods/bot/routes/README.md for the arguments and the result line.
load_game_pack(arg[1] or "re1", { mods = { "mods/bot" }, quick_turn = true, tactical_reload = true })
new_game({ character = 1 })
local run = bot_run({
    params = {
        rope = "wait",
        cave_q1 = "no",
        cave_q2 = "yes",
        chris = "save",
    },
    seed = tonumber(arg[2]),
    stats = arg[3],
    max_ticks = 400000,
})
local missed = {}
for _, check in ipairs({
    { "ending 3: Jill and Chris", run.score.ending == 3 },
    { "common flag 0x52, 308: Barry goes alone (No, Yes)", bot_story(1, 0x52) },
    { "common flag 0x54, 502: Tell my family (lab)", bot_story(1, 0x54) },
}) do
    if not check[2] then
        missed[#missed + 1] = check[1]
    end
end
print(json({ route = "re1_jill_chris_ending", outcome = run.outcome, reason = not run.score.completed and run.finish_reason or nil, missed = missed, score = run.score }))
if #missed > 0 then
    exit(1)
end
