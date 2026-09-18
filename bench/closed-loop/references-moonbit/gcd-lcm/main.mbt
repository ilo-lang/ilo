// task: gcd-lcm — gcd(48,18) -> 6, lcm = 48*18/gcd -> 144
fn gcd(a : Int, b : Int) -> Int {
  if b == 0 {
    a
  } else {
    gcd(b, a % b)
  }
}

///|
fn main {
  let g = gcd(48, 18)
  println("gcd=" + g.to_string())
  println("lcm=" + (48 * 18 / g).to_string())
}
