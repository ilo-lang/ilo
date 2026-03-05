local function bench(n)
    local s = 0
    for i = 0, n - 1 do
        local t = "item-" .. tostring(i)
        -- split on "-"
        local parts = {}
        for part in t:gmatch("[^-]+") do
            parts[#parts + 1] = part
        end
        local j = table.concat(parts, "_")
        s = s + #j
    end
    return s
end

local N = tonumber(arg[1]) or 1000
-- warmup
for i = 1, 100 do bench(N) end
local iters = 10000
local clock = os.clock
local start = clock()
local r
for i = 1, iters do r = bench(N) end
local elapsed = (clock() - start) * 1e9
io.write(string.format("result:     %d\n", r))
io.write(string.format("iterations: %d\n", iters))
io.write(string.format("total:      %.2fms\n", elapsed / 1e6))
io.write(string.format("per call:   %.0fns\n", elapsed / iters))
