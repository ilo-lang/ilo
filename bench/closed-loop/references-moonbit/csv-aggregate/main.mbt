// task: csv-aggregate — sum amounts per region, alphabetical, 2 decimals
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
  let data = "east,10.5\nwest,4.25\neast,6.5\nnorth,3.0\nwest,2.5"
  let sums : Map[String, Double] = Map([], capacity=8)
  for line in data.split("\n") {
    match line.split_once(",") {
      Some((region, amount)) => {
        let key = region.to_owned()
        let v = @string.parse_double(amount) catch { _ => 0.0 }
        sums.set(key, sums.get(key).unwrap_or(0.0) + v)
      }
      None => ()
    }
  }
  // NOTE: this MoonBit core build's String::compare orders by length first, then
  // content (verified against moon 0.1.20260915), so `>` and `sort_by` do not give
  // lexicographic order. We sort by hand comparing char codes explicitly.
  let keys = sums.keys().to_array()
  for i in 1..<keys.length() {
    let k = keys[i]
    let mut j = i
    while j > 0 && sless(k, keys[j - 1]) {
      keys[j] = keys[j - 1]
      j -= 1
    }
    keys[j] = k
  }
  for k in keys {
    println(k + ": " + fmt2(sums.get(k).unwrap_or(0.0)))
  }
}

///|
// True lexicographic a < b by char code.
fn sless(a : String, b : String) -> Bool {
  let la = a.char_length()
  let lb = b.char_length()
  let n = if la < lb { la } else { lb }
  for i in 0..<n {
    let ca = a.at(i)
    let cb = b.at(i)
    if ca != cb {
      return ca < cb
    }
  }
  la < lb
}
