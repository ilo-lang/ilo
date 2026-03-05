def bench(n)
  s = 0
  n.times do |i|
    t = "item-#{i}"
    parts = t.split("-")
    j = parts.join("_")
    s += j.length
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
