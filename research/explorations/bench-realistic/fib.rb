def fib(n)
  return n if n <= 1
  fib(n - 1) + fib(n - 2)
end

n = (ARGV[0] || 25).to_i
5.times { fib(n) }
iters = 100
start = Process.clock_gettime(Process::CLOCK_MONOTONIC, :nanosecond)
r = nil
iters.times { r = fib(n) }
elapsed = Process.clock_gettime(Process::CLOCK_MONOTONIC, :nanosecond) - start
puts "result:     #{r}"
puts "iterations: #{iters}"
puts "total:      #{'%.2f' % (elapsed / 1e6)}ms"
puts "per call:   #{elapsed / iters}ns"
