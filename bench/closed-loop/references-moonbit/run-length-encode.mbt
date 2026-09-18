// task: run-length-encode — "aaabbccccd" -> "a3b2c4d1"
fn main {
  let text = "aaabbccccd"
  let sb = StringBuilder(size_hint=32)
  let mut prev = ' '
  let mut run = 0
  for ch in text.iter() {
    if run > 0 && ch == prev {
      run = run + 1
    } else {
      if run > 0 {
        sb.write_char(prev)
        sb.write_string(run.to_string())
      }
      prev = ch
      run = 1
    }
  }
  if run > 0 {
    sb.write_char(prev)
    sb.write_string(run.to_string())
  }
  println(sb.to_string())
}
