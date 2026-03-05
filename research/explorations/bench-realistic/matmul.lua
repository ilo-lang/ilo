local function bench(n)
    local s = 0
    for i = 0, n - 1 do
        for j = 0, 9 do
            for k = 0, 9 do
                s = s + (i + j) * (j + k)
            end
        end
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
