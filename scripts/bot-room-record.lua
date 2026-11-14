-- Play a game with the bot and write a scene at every room entry: a save
-- (NNNN-ROOM.dat) and the bot's memory (NNNN-ROOM.json) at that moment, for
-- scripts/bot-room-play.lua to play on from. scripts/bot-rooms.py runs it.
--
--   arklay script --headless scripts/bot-room-record.lua PACK OUTDIR [SEED [NAME=VALUE ...]]
--
-- NAME=VALUE are the bot's story parameters (mods/bot/params/re1.xml). The
-- index of the scenes is printed as one JSON line: each scene's number,
-- room, the room it was entered from, the tick and the player's health.
local pack, out = arg[1], arg[2]
if not pack or not out then
    error("usage: bot-room-record.lua PACK OUTDIR [SEED [NAME=VALUE ...]]")
end
local params = {}
for i = 4, #arg do
    local name, value = arg[i]:match("^([%w_]+)=(.+)$")
    if not name then
        error("bad parameter `" .. arg[i] .. "` (want NAME=VALUE)")
    end
    params[name] = value
end
load_game_pack(pack, { mods = { "mods/bot" }, quick_turn = true, tactical_reload = true })
new_game({ character = 1 })
bot_start({ seed = tonumber(arg[3]), params = params, log = false })
local room, scenes, ticks = nil, {}, 0
while ticks < 400000 and not bot_finished() do
    tick()
    ticks = ticks + 1
    if mode() == "play" and game:room() ~= room then
        local from = room
        room = game:room()
        local stem = string.format("%04d-%s", #scenes + 1, room)
        save_game(out .. "/" .. stem .. ".dat")
        bot_memory(out .. "/" .. stem .. ".json")
        scenes[#scenes + 1] = { scene = stem, room = room, from = from, tick = ticks, health = game:health() }
    end
end
local finished, reason = bot_finished()
print(json({ finished = finished, reason = reason, ticks = ticks, scenes = scenes }))
