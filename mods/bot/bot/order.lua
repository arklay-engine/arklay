-- A deterministic `pairs` for the bot VM.
--
-- `pairs` in a fixed key order: numbers, then strings, each ascending (then
-- booleans). Lua 5.4 seeds its string hash from the clock and ASLR, so the
-- stock order over string keys (the planner's marks, the box memory) changed
-- from run to run, and one seed's runs parted at the first tie it broke.
-- Every module looks `pairs` up at call time, so this one serves them
-- all. A key whose value is cleared during the walk is skipped, as the
-- stock walk does.
do
    local raw_next = next
    local rank = { number = 1, string = 2, boolean = 3 }
    local function before(a, b)
        local ta, tb = type(a), type(b)
        if ta ~= tb then
            return (rank[ta] or 4) < (rank[tb] or 4)
        end
        if ta == "number" or ta == "string" then
            return a < b
        end
        if ta == "boolean" then
            return (not a) and b
        end
        return false
    end
    function pairs(t)
        local keys = {}
        for k in raw_next, t do
            keys[#keys + 1] = k
        end
        table.sort(keys, before)
        local i = 0
        return function()
            while true do
                i = i + 1
                local k = keys[i]
                if k == nil then
                    return nil
                end
                local v = t[k]
                if v ~= nil then
                    return k, v
                end
            end
        end, t, nil
    end
end
