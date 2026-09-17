// task: hard-recursion — fib(15) + fib(12) -> 754
// bind-first pattern: each recursive call's result is bound to a local before use
fn fib(n : Int) -> Int {
  if n < 2 {
    n
  } else {
    let a = fib(n - 1)
    let b = fib(n - 2)
    a + b
  }
}

///|
fn main {
  println(fib(15) + fib(12))
}
