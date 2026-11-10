-- RE1, Jill: the Chris ending. In the caves Jill does not go with Barry and
-- stays behind (No, Yes), so he goes off alone and dies in the lab ("Tell my
-- family"); Chris is freed: Jill and Chris leave on the helicopter.
--
--   arklay script [--headless] mods/bot/routes/re1_jill_chris_ending.lua [PACK [SEED [STATS.json]]]
--   arklay script --headless - PACK < mods/bot/routes/re1_jill_chris_ending.lua
--
-- See mods/bot/routes/README.md for the arguments and the result line.
load_game_pack(arg[1] or "games/re1.akpak", { mods = { "mods/bot" }, quick_turn = true, tactical_reload = true })
new_game({ character = 1 })
local run = bot_run({
    params = { cave_q1 = "no" },
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
