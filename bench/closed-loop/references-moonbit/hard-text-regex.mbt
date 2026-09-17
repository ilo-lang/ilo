// task: hard-text-regex — count non-overlapping "ab" in "abababab" -> 4
// MoonBit equivalent of ilo's rgxall: regex literal re"ab" + find + count
fn main {
  let matches = re"ab".find("abababab")
  println(matches.count())
}
