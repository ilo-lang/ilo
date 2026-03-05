local function lev(x)
    if x >= 5000 then return 5 end
    if x >= 3000 then return 4 end
    if x >= 1000 then return 3 end
    if x >= 500 then return 2 end
    return 1
end

local function bench(n)
    local s = 0
    for i = 0, n - 1 do
        local a = i * 3
        local b = a + 1
        local c = b * 2
        local d = lev(c)
        s = s + d
    end
    return s
end

local N = tonumber(arg[1]) or 1000
for i = 1, 1000 do bench(N) end
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
