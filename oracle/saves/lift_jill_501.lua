-- Lift: Jill at room 501's elevator socket with the self-destruct
-- running (F_SCENARIO_SELF_DESTRUCT_ON) and the passage voice line already
-- heard (F_COMMON_HELIPORT_PASSAGE_TALK). The battery is in hand, so the route
-- sets it, rides the lift through 50D onto the heliport (303), fires the flare
-- and kills the Tyrant with the launcher (slot 1) for the replay and the ending.
local api = ...
api:set_player_pos(4000, 0, 29800)
api:set_player_angle(3072)
api:set_health(30000)
api:clear_inventory()
api:set_item_slot(0, 0x27, 1)
api:set_item_slot(1, 0x0A, 4)
api:set_item_slot(2, 0x02, 15)
api:set_item_slot(3, 0x2A, 1)
api:flag_set(0, 0x2B)
api:flag_set(1, 0x45)
