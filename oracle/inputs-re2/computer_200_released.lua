-- arklay's start for computer_200_released: where --warp 200:-16200:-1800:2100:2048 leaves
-- the player inside the counter computer's area (it pushes them to x -14935),
-- with room 200's entry scene seen (4:6), 4:7, and 4:102 so the hall's event 5
-- doesn't start (oracle/re2.md, "Room 200's computer").
local api = ...
api:set_player_pos(-14935, -1800, 2099)
api:set_player_angle(2048)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_re2_flag(4, 6)
api:set_re2_flag(4, 7)
api:set_re2_flag(4, 102)
api:set_item_slot(0, 0x02, 18)
api:set_item_slot(1, 0x01, 1)
api:set_re2_flag(4, 70)
api:set_item_slot(2, 0x35, 1)
