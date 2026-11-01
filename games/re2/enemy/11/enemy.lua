-- Zombie 0x11: already smoking, with much more life, and it bites harder.

local zombie = require("/lib/zombie")

local VARIANT <const> = { life = 0xFA, tough = true }

function update(e)
    zombie.run(e, VARIANT)
end
