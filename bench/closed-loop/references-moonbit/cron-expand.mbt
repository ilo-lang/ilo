// task: cron-expand — expand range-list "1-3,7,9-10" -> "1 2 3 7 9 10"
fn parse_field(s : StringView) -> Int {
  @string.parse_int(s) catch { _ => 0 }
}

///|
fn main {
  let field = "1-3,7,9-10"
  let vals : Array[Int] = []
  for part in field.split(",") {
    match part.split_once("-") {
      Some((lo, hi)) => {
        let a = parse_field(lo)
        let b = parse_field(hi)
        for v in a..<(b + 1) {
          vals.push(v)
        }
      }
      None => vals.push(parse_field(part))
    }
  }
  vals.sort()
  println(vals.iter().map(fn(v) { v.to_string() }).to_array().join(" "))
}
