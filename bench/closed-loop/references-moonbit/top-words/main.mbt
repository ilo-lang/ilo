// task: top-words — top 3 by frequency (freq desc, ties alphabetical)
fn main {
  let counts : Map[String, Int] = Map([], capacity=8)
  for w in "go go stop stop stop run run run run".split(" ") {
    let key = w.to_owned()
    counts.set(key, counts.get(key).unwrap_or(0) + 1)
  }
  let pairs = counts.to_array()
  pairs.sort_by(fn(a, b) {
    let by_freq = b.1.compare(a.1)
    if by_freq == 0 {
      a.0.compare(b.0)
    } else {
      by_freq
    }
  })
  for i in 0..<3 {
    println(pairs[i].0 + " " + pairs[i].1.to_string())
  }
}
