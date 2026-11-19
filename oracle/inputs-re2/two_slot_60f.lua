-- arklay's start for the two_slot_60f run: where --warp leaves Leon on the
-- original's tick 344, in the sub-machine gun's area, with handgun bullets in
-- slot 2 (--give 2:0x14:30; oracle/re2.md, "Two-slot weapons").
local api = ...
-- --no-enemies: the room's three bodies are gone.
api:set_re2_no_enemies()
api:set_player_pos(-11960, 0, -20830)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(30415)
api:set_room_camera(0)
api:set_item_slot(2, 0x14, 30)
