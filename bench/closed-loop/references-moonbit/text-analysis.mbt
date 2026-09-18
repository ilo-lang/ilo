// task: text-analysis — word count, char count, avg word length, longest, shortest
fn fmt1(x : Double) -> String {
  let neg = x < 0.0
  let v = if neg { -x } else { x }
  let scaled = (v * 10.0 + 0.5).floor()
  let whole = (scaled / 10.0).floor().to_int()
  let frac = (scaled - whole.to_double() * 10.0).floor().to_int()
  let sign = if neg { "-" } else { "" }
  sign + whole.to_string() + "." + frac.to_string().pad_start(1, '0')
}

///|
fn main {
  let text = "the quick brown fox jumps over the lazy dog the fox runs fast"
  let words = text.split(" ").to_array()
  let n = words.length()
  let mut total = 0
  let mut longest = words[0]
  let mut shortest = words[0]
  for w in words {
    total = total + w.length()
    if w.length() > longest.length() {
      longest = w
    }
    if w.length() < shortest.length() {
      shortest = w
    }
  }
  println("words=" + n.to_string())
  println("chars=" + text.length().to_string())
  println("avg_len=" + fmt1(total.to_double() / n.to_double()))
  println("longest=" + longest.to_owned())
  println("shortest=" + shortest.to_owned())
}
