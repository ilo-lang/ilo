def addn(a, b) = a + b
def muln(a, b) = a * b

def compute(x, y)
  a = muln(x, y)
  b = addn(a, x)
  addn(b, y)
end

def bench(n)
  s = 0
  i = 0
  while i < n
    j = i + 1
    s += compute(i, j)
    i += 1
  end
  s
end

n = (ARGV[0] || 1000).to_i
100.times { bench(n) }
iters = 10000
start = Process.clock_gettime(Process::CLOCK_MONOTONIC, :nanosecond)
r = nil
iters.times { r = bench(n) }
elapsed = Process.clock_gettime(Process::CLOCK_MONOTONIC, :nanosecond) - start
puts "result:     #{r}"
puts "iterations: #{iters}"
puts "total:      #{'%.2f' % (elapsed / 1e6)}ms"
puts "per call:   #{elapsed / iters}ns"
