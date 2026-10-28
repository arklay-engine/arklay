-- Tyrant (the heliport encounter), entity id 0x10.
--
-- The same machine as the lab fight: only the init state word (the eruption
-- entrance), the health (600), the ceiling burst, the rooftop decision layer
-- and the rocket death differ. The shared machine lives in `enemy/0c/enemy.lua`.

local tyrant = require("../0c/enemy")

function update(e)
    tyrant.update(e)
end
