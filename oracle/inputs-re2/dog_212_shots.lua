-- arklay's start for dog_212_shots: where --warp 212:-11250:0:-25692:3024
-- --flag 4:83 --life 200 leaves the player, with the record's --equip 2
-- --give 0:2:200 --give 1:0x14:60, and NORMAL's Rnd (6114).
local api = ...
api:set_player_pos(-11250, 0, -25692)
api:set_player_angle(3024)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(0)
api:set_re2_flag(4, 83)
api:set_item_slot(0, 0x02, 200)
api:set_item_slot(1, 0x14, 60)
api:set_equipped(0x02)
