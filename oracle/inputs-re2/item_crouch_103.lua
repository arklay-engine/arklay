-- arklay's start for item_crouch_103: where --warp 103:2000:0:-14478:2048:3
-- leaves the player in room 103's passage, with --give 0:7:30 --equip 7
-- --life 200 (oracle/re2.md, "Items"). Room 103 places zombie Brad (enemy
-- slot 0) with his key while no item of the first stages is taken.
-- set_equipped, not only set_equipped_weapon, so the get-item window's EQUIP
-- box shows the shotgun as the original's does.
local api = ...
api:set_player_pos(2000, 0, -14478)
api:set_player_angle(2048)
api:set_health(200)
api:set_re2_rnd(6114)
api:set_room_camera(3)
api:set_item_slot(0, 0x07, 30)
api:set_equipped(0x07)
