// task: weekday-of-date — Zeller's congruence for 2026-09-18 -> friday
// q=18 (day), m=9 (month; Jan/Feb count as 13/14 of prior year, not needed here)
// K=year%100=26, J=year/100=20; h: 0=Saturday
fn main {
  let q = 18
  let m = 9
  let k = 26
  let j = 20
  let h = (q + (13 * (m + 1)) / 5 + k + k / 4 + j / 4 + 5 * j) % 7
  let names = [
    "saturday", "sunday", "monday", "tuesday", "wednesday", "thursday", "friday",
  ]
  println(names[h])
}
