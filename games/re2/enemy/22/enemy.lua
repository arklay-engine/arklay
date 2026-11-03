-- The licker, `Em_22` (entity id 0x22).

local licker = require("/lib/licker")

local VARIANT <const> = {
    -- `Em22_Life` and its easy table, and what flag 0xA4 takes off each.
    life = {
        [0] = 87, 101, 81, 75, 110, 115, 99, 101, 101, 99, 81, 101, 115, 110, 87, 101,
    },
    life_easy = {
        [0] = 65, 65, 70, 70, 75, 69, 62, 90, 80, 75, 82, 97, 89, 101, 90, 100,
    },
    down = 10,
    down_easy = 20,
    -- `BYTE_00527c7c` and `BYTE_00527c80`, by `Type`: where it starts.
    starts = { [0] = 0, 7, 0xB, 0x17 },
    starts_scripted = { [0] = 0x10, 0x11, 0x12, 0x16 },
    -- `Joint_move2`'s blend for the first pose.
    init_late = 0x100,
    clips = {
        slash = 0x15,
        leap = 0x1F,
        flight = 0x20,
        land = 0x21,
        run_at = 0x1E,
        let_go = 0x12,
        drop = 0x13,
        shot_down = 0x19,
        shot_up = 0x16,
        die_front = 0x1C,
        die_back = 0x1D,
    },
    -- Rows of `DAT_00527c90` that differ from the shared table: none.
    blood = {},
}

function update(e)
    licker.run(e, VARIANT)
end
