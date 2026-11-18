-- Knife: Jill in the dining room (room 101) with the Combat Knife equipped,
-- on the open floor south of the table and facing the zombie that walks up
-- to it. A new game equips nothing, so the knife scenarios boot from this save.
local api = ...
api:set_player_pos(12000, 0, 18200)
api:set_player_angle(199)
api:clear_inventory()
api:set_item_slot(0, 0x01, 0)
api:set_item_slot(1, 0x41, 1)
api:set_equipped(0x01)
