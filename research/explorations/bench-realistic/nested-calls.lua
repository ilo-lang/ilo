local function addn(a, b) return a + b end
local function muln(a, b) return a * b end

local function compute(x, y)
    local a = muln(x, y)
    local b = addn(a, x)
    return addn(b, y)
end

local function bench(n)
    local s = 0
    for i = 0, n - 1 do
        s = s + compute(i, i + 1)
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
