-- Zombie 0x15.

local zombie = require("/lib/zombie")

local VARIANT <const> = {}

function update(e)
    zombie.run(e, VARIANT)
end
