local function cata(x)
    if x >= 800 then if x >= 900 then return 9 end; return 8 end
    if x >= 600 then if x >= 700 then return 7 end; return 6 end
    if x >= 400 then if x >= 500 then return 5 end; return 4 end
    if x >= 200 then if x >= 300 then return 3 end; return 2 end
    return 1
end

local function catb(x)
    if x >= 500 then return x * 3 end
    if x >= 200 then return x * 2 end
    return x
end

local function combine(a, b)
    if a >= 7 then return b + a * 10 end
    if a >= 4 then return b + a * 5 end
    return b + a
end

local function bench(n)
    local s = 0
    for i = 0, n - 1 do
        local a = cata(i)
        local b = catb(i)
        local c = combine(a, b)
        s = s + c
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
