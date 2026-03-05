Entry = Struct.new(:k, :v, :sum)

def bench(n)
  s = 0
  n.times do |i|
    e = Entry.new(i, i * 7, 0)
    e2 = Entry.new(e.k, e.v, e.k + e.v)
    s += e2.sum
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
