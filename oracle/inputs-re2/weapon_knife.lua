-- arklay's start for weapon_knife: where --warp 102:-20000:0:15000:0 leaves
-- the player, with Leon's start items (the handgun with 18 rounds in slot 0,
-- equipped, and the knife in slot 1), which the original has after the warp.
-- Room 102's zombies stand here, so their dead flags clear them, as the
-- original's --no-enemies does (oracle/re2.md, "Weapons").
local api = ...
api:set_player_pos(-20000, 0, 15000)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(0, 0x02, 18)
api:set_item_slot(1, 0x01, 1)
api:set_equipped(0x02)
for _, flag in ipairs({51, 52, 54, 55}) do api:set_re2_flag(6, flag) end
