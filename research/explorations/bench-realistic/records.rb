Vec3 = Struct.new(:x, :y, :z)

def run(n)
  s = 0
  n.times do |i|
    v = Vec3.new(i, i * 2, i * 3)
    d = v.x + v.y + v.z
    v2 = Vec3.new(v.x + 1, v.y, v.z)
    s += d + v2.x
  end
  s
end

n = (ARGV[0] || 1000).to_i
100.times { run(n) }
iters = 10000
start = Process.clock_gettime(Process::CLOCK_MONOTONIC, :nanosecond)
r = nil
iters.times { r = run(n) }
elapsed = Process.clock_gettime(Process::CLOCK_MONOTONIC, :nanosecond) - start
puts "result:     #{r}"
puts "iterations: #{iters}"
puts "total:      #{'%.2f' % (elapsed / 1e6)}ms"
puts "per call:   #{elapsed / iters}ns"
