// task: prime-count — primes below 30: count=10, largest=29
fn is_prime(n : Int) -> Bool {
  if n < 2 {
    return false
  }
  let mut d = 2
  while d * d <= n {
    if n % d == 0 {
      return false
    }
    d = d + 1
  }
  true
}

///|
fn main {
  let mut count = 0
  let mut largest = 0
  for n in 1..<30 {
    if is_prime(n) {
      count = count + 1
      largest = n
    }
  }
  println("count=" + count.to_string())
  println("largest=" + largest.to_string())
}
