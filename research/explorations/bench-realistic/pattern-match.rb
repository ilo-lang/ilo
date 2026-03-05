def cata(x)
  if x >= 800
    return 9 if x >= 900
    return 8
  end
  if x >= 600
    return 7 if x >= 700
    return 6
  end
  if x >= 400
    return 5 if x >= 500
    return 4
  end
  if x >= 200
    return 3 if x >= 300
    return 2
  end
  1
end

def catb(x)
  return x * 3 if x >= 500
  return x * 2 if x >= 200
  x
end

def combine(a, b)
  return b + a * 10 if a >= 7
  return b + a * 5 if a >= 4
  b + a
end

def bench(n)
  s = 0
  n.times do |i|
    a = cata(i)
    b = catb(i)
    c = combine(a, b)
    s += c
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
