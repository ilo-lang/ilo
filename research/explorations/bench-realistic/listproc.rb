def lev(x)
  return 5 if x >= 5000
  return 4 if x >= 3000
  return 3 if x >= 1000
  return 2 if x >= 500
  1
end

def bench(n)
  s = 0
  n.times do |i|
    a = i * 3
    b = a + 1
    c = b * 2
    d = lev(c)
    s += d
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
