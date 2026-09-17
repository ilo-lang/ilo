// task: data-transform — sum of squares of [1..5] greater than 10 -> 41
fn main {
  let total = [1, 2, 3, 4, 5].map(fn(x : Int) -> Int {
    x * x
  }).filter(fn(x : Int) -> Bool {
    x > 10
  }).fold(init=0, fn(acc : Int, x : Int) -> Int {
    acc + x
  })
  println(total)
}
