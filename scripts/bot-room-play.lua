-- Play one scene scripts/bot-room-record.lua wrote: load its save, give the
-- bot the memory it had, and let it play until it leaves the room (or dies,
-- or MAX ticks pass). scripts/bot-rooms.py runs it over scenes and seeds.
--
--   arklay script --headless scripts/bot-room-play.lua PACK SCENE [SEED [MAX [EXIT [log]]]]
--
-- SCENE is the scene's path without its extension. EXIT, when given and not
-- empty, is a room to walk to first, whatever the bot would choose. The
-- result is one JSON line: the ticks the room took, the hits and damage
-- taken, the shots fired, whether the player died, and where the bot went.
local pack, scene = arg[1], arg[2]
if not pack or not scene then
    error("usage: bot-room-play.lua PACK SCENE [SEED [MAX [EXIT [log]]]]")
end
local max = tonumber(arg[4]) or 3000
load_game_pack(pack, { mods = { "mods/bot" }, quick_turn = true, tactical_reload = true })
load_save(scene .. ".dat")
local head = arg[5] ~= nil and arg[5] ~= "" and arg[5] or nil
bot_start({
    seed = tonumber(arg[3]),
    memory = scene .. ".json",
    remember = { head_for = head },
    log = arg[6] == "log",
})
local room, ticks = game:room(), 0
while ticks < max and not bot_finished() and mode() == "play" and game:room() == room
    and game:health() > 0 do
    tick()
    ticks = ticks + 1
end
local stats = bot_stats()
local playing = mode() == "play"
local dead = not playing or game:health() <= 0
local left = not dead and game:room() ~= room
print(json({
    scene = scene:match("[^/]+$"),
    seed = tonumber(arg[3]) or 0,
    room = room,
    exit = left and game:room() or nil,
    ticks = ticks,
    hits = stats.hits,
    damage = stats.health.damage_taken,
    shots = stats.combat.shots_fired_proxy,
    -- The report counts a death on the tick after it; the replay ends on it.
    died = stats.deaths > 0 or dead,
    health = playing and game:health() or nil,
    reason = select(2, bot_finished()),
}))
