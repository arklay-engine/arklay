-- arklay's start for licker_204_ceiling: where --warp 204:-16000:0:-10000:1024 --life 2000 leaves the player,
-- with room 204's entry scene seen (flag 4:85) and the revisit's licker on the ceiling (flag 4:86, `main_02`'s
-- `Type` 0x0512) (oracle/re2.md, "Room 204's licker").
local api = ...
api:set_player_pos(-16000, 0, -10000)
api:set_player_angle(1024)
api:set_health(2000)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_re2_flag(4, 85)
api:set_re2_flag(4, 86)
api:set_item_slot(0, 0x02, 200)
api:set_equipped(0x02)
