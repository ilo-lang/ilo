// task: least-squares-slope — OLS on (1,2),(2,4),(3,5),(4,4),(5,5): slope 0.60, intercept 2.20
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
  let xs = [1, 2, 3, 4, 5]
  let ys = [2, 4, 5, 4, 5]
  let n = xs.length()
  let mut sx = 0
  let mut sy = 0
  let mut sxx = 0
  let mut sxy = 0
  for i in 0..<n {
    sx = sx + xs[i]
    sy = sy + ys[i]
    sxx = sxx + xs[i] * xs[i]
    sxy = sxy + xs[i] * ys[i]
  }
  let slope = (n * sxy - sx * sy).to_double() / (n * sxx - sx * sx).to_double()
  let intercept = (sy.to_double() - slope * sx.to_double()) / n.to_double()
  println("slope=" + fmt2(slope))
  println("intercept=" + fmt2(intercept))
}
