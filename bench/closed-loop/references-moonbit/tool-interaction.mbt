// task: tool-interaction — read BENCH_VAR (default "hello"), append " world"
// Imports live in tool-interaction/moon.pkg (moon.pkg-only declarations).
// Run from this directory with: moon run tool-interaction
fn main {
  let value : String = @env.get_env_var("BENCH_VAR").unwrap_or("hello")
  println(value + " world")
}
