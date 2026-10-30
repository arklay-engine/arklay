-- The new-game start inventory (`Stage_init_item`): `0` Leon, `1` Claire.
-- A mod replaces it by shipping its own `game/new_game.lua`.

local api, character = ...

local KNIFE = 0x01
local HANDGUN_LEON = 0x02
local HANDGUN_CLAIRE = 0x03
local LIGHTER = 0x2F
local LOCKPICK = 0x30

api:set_item_slot(0, KNIFE, 0)
if character == 0 then
    api:set_item_slot(1, HANDGUN_LEON, 18)
    api:set_item_slot(2, LIGHTER, 1)
else
    api:set_item_slot(1, HANDGUN_CLAIRE, 13)
    api:set_item_slot(2, LOCKPICK, 1)
end
