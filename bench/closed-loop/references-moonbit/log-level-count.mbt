// task: log-level-count — count lines by level; ERROR=3, WARN=2, INFO=3
fn main {
  let log = "INFO start\nERROR disk full\nINFO retry\nWARN slow\nERROR timeout\nINFO done\nWARN mem high\nERROR fatal"
  let mut errors = 0
  let mut warns = 0
  let mut infos = 0
  for line in log.split("\n") {
    if line.has_prefix("ERROR") {
      errors = errors + 1
    } else if line.has_prefix("WARN") {
      warns = warns + 1
    } else if line.has_prefix("INFO") {
      infos = infos + 1
    }
  }
  println("ERROR=" + errors.to_string())
  println("WARN=" + warns.to_string())
  println("INFO=" + infos.to_string())
}
