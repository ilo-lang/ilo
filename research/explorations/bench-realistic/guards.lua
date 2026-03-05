local function classify(x)
    if x >= 900 then return 30 end
    if x >= 700 then return 25 end
    if x >= 500 then return 20 end
    if x >= 300 then return 15 end
    if x >= 100 then return 10 end
    return 5
end

local function bench(n)
    local s = 0
    for i = 0, n - 1 do
        s = s + classify(i)
    end
    return s
end

local N = tonumber(arg[1]) or 1000
-- warmup
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
