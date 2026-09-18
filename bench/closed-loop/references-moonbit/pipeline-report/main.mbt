// task: pipeline-report — revenue=qty*price per name:qty:price record; keep >20, sort desc
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
  let data = "widget:10:5.00,gadget:3:15.00,gizmo:7:8.00,tool:2:20.00,book:15:2.00"
  let revs : Array[(String, Double)] = []
  for rec in data.split(",") {
    match rec.split_once(":") {
      Some((name, rest)) =>
        match rest.split_once(":") {
          Some((qty, price)) => {
            let q = @string.parse_int(qty) catch { _ => 0 }
            let p = @string.parse_double(price) catch { _ => 0.0 }
            let revenue = q.to_double() * p
            if revenue > 20.0 {
              revs.push((name.to_owned(), revenue))
            }
          }
          None => ()
        }
      None => ()
    }
  }
  revs.sort_by(fn(a, b) { b.1.compare(a.1) })
  let lines : Array[String] = []
  for r in revs {
    lines.push(r.0 + ": " + fmt2(r.1))
  }
  println(lines.join("\n"))
}
