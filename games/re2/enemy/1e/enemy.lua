-- Zombie 0x1E: a taller body.

local zombie = require("/lib/zombie")

local VARIANT <const> = { parts0_y = 0xE66 }

function update(e)
    zombie.run(e, VARIANT)
end
