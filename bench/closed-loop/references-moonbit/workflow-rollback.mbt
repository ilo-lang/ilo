// task: workflow-rollback — safe-div with zero-divisor guard -> "5\n0"
fn safe_div(a : Int, b : Int) -> Int {
  if b == 0 {
    0
  } else {
    a / b
  }
}

///|
fn main {
  println(safe_div(10, 2))
  println(safe_div(5, 0))
}
