// task: kmeans-assign — 1-D k-means assignment to centroids 2 and 11; ties to first
fn fmt2(x : Double) -> String {
  let neg = x < 0.0
  let v = if neg { -x } else { x }
  let scaled = (v * 100.0 + 0.5).floor()
  let whole = (scaled / 100.0).floor().to_int()
  let frac = (scaled - whole.to_double() * 100.0).floor().to_int()
  let sign = if neg { "-" } else { "" }
  sign + whole.to_string() + "." + frac.to_string().pad_start(2, '0')
}

///|
fn main {
  let points = [1, 2, 3, 10, 11, 12]
  let c0 = 2.0
  let c1 = 11.0
  let mut sum0 = 0.0
  let mut sum1 = 0.0
  let mut n0 = 0
  let mut n1 = 0
  let assign : Array[String] = []
  for p in points {
    let pd = p.to_double()
    let d0 = (pd - c0).abs()
    let d1 = (pd - c1).abs()
    if d0 <= d1 {
      assign.push("0")
      sum0 = sum0 + pd
      n0 = n0 + 1
    } else {
      assign.push("1")
      sum1 = sum1 + pd
      n1 = n1 + 1
    }
  }
  println(assign.join(","))
  println("c0=" + fmt2(sum0 / n0.to_double()))
  println("c1=" + fmt2(sum1 / n1.to_double()))
}
