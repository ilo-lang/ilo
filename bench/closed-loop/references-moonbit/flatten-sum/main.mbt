// task: flatten-sum — flatten [[1,2],[3,4],[5]] -> "1 2 3 4 5", sum 15
fn main {
  let nested : Array[Array[Int]] = [[1, 2], [3, 4], [5]]
  let flat : Array[Int] = []
  for xs in nested {
    for x in xs {
      flat.push(x)
    }
  }
  println("flat=" + flat.iter().map(fn(x) { x.to_string() }).to_array().join(" "))
  println("sum=" + flat.iter().fold(init=0, fn(acc, x) { acc + x }).to_string())
}
