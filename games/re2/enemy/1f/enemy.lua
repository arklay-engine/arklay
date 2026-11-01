-- Zombie 0x1F: its skin comes from `Model_type` or `Rnd`.

local zombie = require("/lib/zombie")

local VARIANT <const> = { skins = true }

function update(e)
    zombie.run(e, VARIANT)
end
