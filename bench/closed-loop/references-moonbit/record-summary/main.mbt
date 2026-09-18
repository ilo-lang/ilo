// task: record-summary — qty*price over three records; total=151.00, over=widget,gadget,gizmo
struct Record {
  name : String
  qty : Int
  price : Double
}

///|
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
  let records : Array[Record] = [
    Record::{ name: "widget", qty: 10, price: 5.00 },
    Record::{ name: "gadget", qty: 3, price: 15.00 },
    Record::{ name: "gizmo", qty: 7, price: 8.00 },
  ]
  let mut total = 0.0
  let over : Array[String] = []
  for r in records {
    let value = r.qty.to_double() * r.price
    total = total + value
    if value > 40.0 {
      over.push(r.name)
    }
  }
  println("total=" + fmt2(total))
  println("over=" + over.join(","))
}
