-- arklay's start for leon_a_200_out: where --warp 200:-14936:-1800:1202:1992:6
-- leaves the player, at room 200's computer once the Blue Card Key is used
-- (flags 4:6, 4:7 and 4:102 from the hall's scenes, 4:70 from the card), with
-- the handgun's 13 rounds and 9 spare.
local api = ...
api:set_player_pos(-14936, -1800, 1202)
api:set_player_angle(1992)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(6)
api:set_re2_flag(4, 6)
api:set_re2_flag(4, 7)
api:set_re2_flag(4, 102)
api:set_re2_flag(4, 70)
api:set_item_slot(0, 0x02, 13)
api:set_item_slot(1, 0x01, 1)
api:set_item_slot(2, 0x14, 9)
