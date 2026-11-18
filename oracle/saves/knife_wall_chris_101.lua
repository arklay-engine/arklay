-- Knife against the wall: knife_chris_101 with Chris nearer the dining-room
-- wall, so the zombie he kills falls with its body against the boundary and
-- the prone body's two-point floor probe pushes it out.
local api = ...
api:set_player_pos(13050, 0, 20100)
api:set_player_angle(166)
api:clear_inventory()
api:set_item_slot(0, 0x01, 0)
api:set_item_slot(1, 0x41, 1)
api:set_equipped(0x01)
