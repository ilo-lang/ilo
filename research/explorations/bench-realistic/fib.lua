local function fib(n)
    if n <= 1 then return n end
    return fib(n - 1) + fib(n - 2)
end

local N = tonumber(arg[1]) or 25
-- warmup
for i = 1, 100 do fib(N) end
local iters = 1000
local clock = os.clock
local start = clock()
local r
for i = 1, iters do r = fib(N) end
local elapsed = (clock() - start) * 1e9
io.write(string.format("result:     %d\n", r))
io.write(string.format("iterations: %d\n", iters))
io.write(string.format("total:      %.2fms\n", elapsed / 1e6))
io.write(string.format("per call:   %.0fns\n", elapsed / iters))
