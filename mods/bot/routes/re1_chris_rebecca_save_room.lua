-- RE1, Chris: Rebecca waits in the save room. Ending 6: Chris, Jill and Rebecca.
--
-- These parameters decide whether Rebecca and Jill reach the helicopter, so
-- they fix the ending:
--   rebecca_meet=save_room: Chris meets Rebecca in the save room (100) before
--     Richard in 20D, who is then found dead.
--   rebecca_along=no: Chris leaves Rebecca in the save room: after 10A's
--     hunter film her radio call sends him back to 101, where a hunter
--     attacks her if he is late.
--   rebecca=save: Chris answers every hunter attack on Rebecca (101, and 206
--     after the upstairs call a slow serum run or Richard's death arms) and
--     kills the hunter there, so she lives.
--   jill=save: Jill is freed from 512's cell before the escape lift.
-- Here richard does nothing once Rebecca is met first (he is found dead).
-- Yawn's bite, Rebecca's serum run after it and Plant 42's V-JOLT follow from
-- the game state, not from a parameter. The other parameters keep their
-- defaults: none of them changes who escapes.
--
--   arklay script [--headless] mods/bot/routes/re1_chris_rebecca_save_room.lua [PACK [SEED [STATS.json]]]
--   arklay script --headless - PACK < mods/bot/routes/re1_chris_rebecca_save_room.lua
--
-- See mods/bot/routes/README.md for the arguments and the result line.
load_game_pack(arg[1] or "re1", { mods = { "mods/bot" }, quick_turn = true, tactical_reload = true })
new_game({ character = 0 })
local run = bot_run({
    params = {
        rebecca_meet = "save_room",
        rebecca_along = "no",
        rebecca = "save",
        jill = "save",
    },
    seed = tonumber(arg[2]),
    stats = arg[3],
    max_ticks = 400000,
})
local missed = {}
for _, check in ipairs({
    { "ending 6: Chris, Jill and Rebecca", run.score.ending == 6 },
    { "common flag 0x22, 100: Rebecca waits in the save room", bot_story(1, 0x22) },
}) do
    if not check[2] then
        missed[#missed + 1] = check[1]
    end
end
print(json({ route = "re1_chris_rebecca_save_room", outcome = run.outcome, reason = not run.score.completed and run.finish_reason or nil, missed = missed, score = run.score }))
if #missed > 0 then
    exit(1)
end
