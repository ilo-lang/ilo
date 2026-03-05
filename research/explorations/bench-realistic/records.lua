local function run(n)
    local s = 0
    for i = 0, n - 1 do
        local v = { x = i, y = i * 2, z = i * 3 }
        local d = v.x + v.y + v.z
        local v2 = { x = v.x + 1, y = v.y, z = v.z }
        s = s + d + v2.x
    end
    return s
end

local N = tonumber(arg[1]) or 1000
-- warmup
for i = 1, 1000 do run(N) end
local iters = 10000
local clock = os.clock
local start = clock()
local r
for i = 1, iters do r = run(N) end
local elapsed = (clock() - start) * 1e9
io.write(string.format("result:     %d\n", r))
io.write(string.format("iterations: %d\n", iters))
io.write(string.format("total:      %.2fms\n", elapsed / 1e6))
io.write(string.format("per call:   %.0fns\n", elapsed / iters))
