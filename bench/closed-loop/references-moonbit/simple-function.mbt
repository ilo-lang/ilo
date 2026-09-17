// task: simple-function — triangular number of n=10 -> 55
fn triangular(n : Int) -> Int {
  n * (n + 1) / 2
}

///|
fn main {
  println(triangular(10))
}
