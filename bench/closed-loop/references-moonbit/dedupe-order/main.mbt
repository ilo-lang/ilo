// task: dedupe-order — first-occurrence dedupe of [3,1,3,2,1,4] -> "3 1 2 4"
fn main {
  let values : Array[Int] = [3, 1, 3, 2, 1, 4]
  let seen : Map[Int, Bool] = Map([], capacity=8)
  let out : Array[Int] = []
  for x in values {
    match seen.get(x) {
      Some(_) => ()
      None => {
        seen.set(x, true)
        out.push(x)
      }
    }
  }
  println(out.iter().map(fn(x) { x.to_string() }).to_array().join(" "))
}
