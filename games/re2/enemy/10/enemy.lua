-- Zombie 0x10: burning leaves its colours alone.

local zombie = require("/lib/zombie")

local VARIANT <const> = { burns_unmarked = true }

function update(e)
    zombie.run(e, VARIANT)
end
