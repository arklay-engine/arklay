-- The second licker, `Em_24` (entity id 0x24): `Em_22`'s code with its own
-- clips, life and choices, and no climbing or scripted drops.

local licker = require("/lib/licker")

local VARIANT <const> = {
    em24 = true,
    -- `WORD_ARRAY_005280e4` and the table before it, and what flag 0xA4 takes
    -- off each.
    life = {
        [0] = 80, 80, 85, 85, 90, 84, 77, 105, 95, 90, 97, 112, 104, 116, 105, 115,
    },
    life_easy = {
        [0] = 130, 90, 96, 116, 125, 102, 114, 125, 116, 114, 96, 125, 102, 125, 130, 90,
    },
    down = 30,
    down_easy = 40,
    -- `BYTE_00528130`, by `Type`: where it starts. There's no scripted table.
    starts = { [0] = 0, 0x17, 0xB, 0x17 },
    -- `Joint_move`'s blend for the first pose.
    init_late = 0x1000,
    clips = {
        slash = 0xF,
        leap = 0x17,
        flight = 0x18,
        land = 0x19,
        run_at = 0x16,
        let_go = 0xC,
        drop = 0xD,
        shot_down = 0x13,
        shot_up = 0x10,
        die_front = 0x14,
        die_back = 0x15,
    },
    -- `DAT_00528360`: the licker's blood, but for the last weapon's.
    blood = {
        [19] = { 1, 0, 12, 12, 0, 6, 0x2000 },
    },
}

function update(e)
    licker.run(e, VARIANT)
end
