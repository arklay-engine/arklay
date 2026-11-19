-- arklay's start for the two_slot_60f_one_free run: two_slot_60f's, with
-- seven of Leon's eight slots held (--give 2:0x14:30 --give 3:0x26:1
-- --give 4:0x27:1 --give 5:0x28:1 --give 6:0x23:1; oracle/re2.md, "Two-slot
-- weapons").
local api = ...
-- --no-enemies: the room's three bodies are gone.
api:set_re2_no_enemies()
api:set_player_pos(-11960, 0, -20830)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(30415)
api:set_room_camera(0)
api:set_item_slot(2, 0x14, 30)
api:set_item_slot(3, 0x26, 1)
api:set_item_slot(4, 0x27, 1)
api:set_item_slot(5, 0x28, 1)
api:set_item_slot(6, 0x23, 1)
