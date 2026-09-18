// task: fizzbuzz-range — FizzBuzz over 1..15, joined by commas
fn fizzbuzz(n : Int) -> String {
  if n % 15 == 0 {
    "FizzBuzz"
  } else if n % 3 == 0 {
    "Fizz"
  } else if n % 5 == 0 {
    "Buzz"
  } else {
    n.to_string()
  }
}

///|
fn main {
  let parts : Array[String] = []
  for n in 1..<16 {
    parts.push(fizzbuzz(n))
  }
  println(parts.join(","))
}
