-- Zombie 0x12: `Em10_Init` draws twice more for it.

local zombie = require("/lib/zombie")

local VARIANT <const> = { init_draws = 2 }

function update(e)
    zombie.run(e, VARIANT)
end
