-- arklay's start for weapon_knife_wall: item_102.lua's, with Leon's start
-- items (the handgun with 18 rounds in slot 0, equipped, and the knife in
-- slot 1), which the original has after the warp (oracle/re2.md, "Weapons").
local api = ...
api:set_player_pos(-11378, 0, -4587)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_item_slot(0, 0x02, 18)
api:set_item_slot(1, 0x01, 1)
api:set_equipped(0x02)
