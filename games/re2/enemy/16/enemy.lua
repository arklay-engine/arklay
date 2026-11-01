-- Zombie 0x16.

local zombie = require("/lib/zombie")

local VARIANT <const> = {}

function update(e)
    zombie.run(e, VARIANT)
end
