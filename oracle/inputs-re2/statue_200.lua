-- arklay's start for statue_200: where --warp 200:-13700:0:-9900:3072:11 leaves the player,
-- facing the statue with the probe in the Unicorn Medal's area (13), with room 200's entry
-- scene seen (4:6), 4:7, the computer's doors released (4:70) and 4:102 so the hall's
-- event 5 doesn't start (oracle/re2.md, "Using a key item").
local api = ...
api:set_player_pos(-13700, 0, -9900)
api:set_player_angle(3072)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(11)
api:set_re2_flag(4, 6)
api:set_re2_flag(4, 7)
api:set_re2_flag(4, 70)
api:set_re2_flag(4, 102)
api:set_item_slot(0, 0x02, 13)
api:set_item_slot(1, 0x01, 1)
api:set_item_slot(2, 0x14, 20)
api:set_item_slot(3, 0x1E, 3)
api:set_item_slot(4, 0x47, 1)
