-- arklay's start for the zombies_207 runs: where
-- --warp 207:-25640:0:-16590:0:1 leaves the player, at room 205's door 2
-- arrival in cut 1 (oracle/re2.md, "Room 207's zombies").
local api = ...
api:set_player_pos(-25640, 0, -16590)
api:set_player_angle(0)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(1)
-- Slot 2 starts in guide area 1, away from the player, so it walks toward its
-- starting `Dest` for the ticks until its first `kara_rootck2`. Neither
-- `Sce_em_set` nor `Em10_Init` writes it: the new work is allocated at
-- `Mem_top`, so it holds whatever last sat at that address. This is
-- what the original's warp leaves there (the reference's `zd` at 343). On the
-- route, room 205's works come first and leave other values. It takes effect
-- only because `set_re2_rnd` makes the room's init run again
-- (`finish_authoring`), so the next `sce_em_set` in slot 2 takes it. Slots 0
-- and 3 share the player's area and need none.
api:set_re2_enemy_dest("207:2", -5586, 128)
