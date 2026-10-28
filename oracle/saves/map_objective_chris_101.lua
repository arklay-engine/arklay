-- MAP tab objective label: the map save plus Chris's first serum objective
-- outstanding (second scenario bank 0x2D, unacknowledged), so the Mansion 1F
-- floor plan blinks its objective square.
local api = ...
api:set_player_pos(13050, 0, 20100)
api:set_player_angle(0)
for area = 0, 7 do
    api:flag_set(8, 0x7C + area, true)
end
api:flag_set(1, 0x2D, true)
