-- Zombie 0x17: it bites harder.

local zombie = require("/lib/zombie")

local VARIANT <const> = { tough = true }

function update(e)
    zombie.run(e, VARIANT)
end
