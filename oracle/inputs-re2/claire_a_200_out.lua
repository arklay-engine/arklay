-- arklay's start for claire_a_200_out: where --start claire
-- --warp 200:-14847:-1800:1892:2136:6 leaves the player, where room 200's
-- computer hands Claire back once the Blue Card Key is used (flags 4:6, 4:7
-- and 4:102 from the hall's scenes, 4:70 from the card), with the handgun's 7
-- rounds and 6 spare, as route_claire_a_card leaves her.
local api = ...
api:set_player_pos(-14847, -1800, 1892)
api:set_player_angle(2136)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(6)
api:set_re2_flag(4, 6)
api:set_re2_flag(4, 7)
api:set_re2_flag(4, 102)
api:set_re2_flag(4, 70)
api:set_item_slot(0, 0x03, 7)
api:set_item_slot(1, 0x01, 1)
api:set_item_slot(2, 0x14, 6)
