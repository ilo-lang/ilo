// task: with-dependencies — quad via double composition -> 28
fn double(x : Int) -> Int {
  x * 2
}

///|
fn quad(x : Int) -> Int {
  double(double(x))
}

///|
fn main {
  println(quad(7))
}
