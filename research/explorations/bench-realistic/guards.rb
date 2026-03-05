def classify(x)
  return 30 if x >= 900
  return 25 if x >= 700
  return 20 if x >= 500
  return 15 if x >= 300
  return 10 if x >= 100
  5
end

def bench(n)
  s = 0
  n.times { |i| s += classify(i) }
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
