-- The new-game start inventory (`Stage_init_item`'s table): `0` Leon, `1` Claire.
-- The table keeps the lighter and the lockpick in slot 10, past the eight this
-- inventory has.
-- A mod replaces it by shipping its own `game/new_game.lua`.

local api, character = ...

local KNIFE = 0x01
local HANDGUN_LEON = 0x02
local HANDGUN_CLAIRE = 0x03
local LIGHTER = 0x2F
local LOCKPICK = 0x30

if character == 0 then
    api:set_item_slot(0, HANDGUN_LEON, 18)
    api:set_item_slot(1, KNIFE, 1)
    api:set_item_slot(2, LIGHTER, 1)
    api:set_equipped(HANDGUN_LEON)
else
    api:set_item_slot(0, HANDGUN_CLAIRE, 13)
    api:set_item_slot(1, KNIFE, 1)
    api:set_item_slot(2, LOCKPICK, 1)
    api:set_equipped(HANDGUN_CLAIRE)
end
