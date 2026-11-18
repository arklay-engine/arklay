-- Pick-up: Chris on the heliport (303) with the self-destruct running
-- (F_SCENARIO_SELF_DESTRUCT_ON), the flare in slot 0, the Beretta and the
-- knife. The route fires the flare, keeps away from the Tyrant until Brad drops
-- the rocket launcher, takes it from the floor and equips it.
local api = ...
api:set_player_pos(19850, 0, 24600)
api:set_player_angle(0)
api:set_health(30000)
api:clear_inventory()
api:set_item_slot(0, 0x2A, 1)
api:set_item_slot(1, 0x02, 15)
api:set_item_slot(2, 0x01, 0)
api:flag_set(0, 0x2B)
