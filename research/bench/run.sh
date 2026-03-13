#!/usr/bin/env bash
# ilo-lang benchmark suite — compare ilo JIT/VM against other languages
# Usage: ./research/bench/run.sh [iters]
# Requires: go, node, lua, luajit, python3, rustc (via rustup)
set -euo pipefail

ITERS=${1:-10000}
DIR="$(cd "$(dirname "$0")" && pwd)"
TMP=$(mktemp -d)
cleanup() { kill $HTTP_PID 2>/dev/null || true; rm -rf "$TMP"; }
HTTP_PID=0
trap cleanup EXIT

# Ensure cargo/rustc/java are in PATH
export PATH="$HOME/.cargo/bin:$PATH"
export JAVA_HOME="/opt/homebrew/opt/openjdk"
export PATH="$JAVA_HOME/bin:$PATH"
export DOTNET_ROOT="$(dirname "$(dirname "$(readlink -f "$(which dotnet)" 2>/dev/null || which dotnet)")")/libexec"

ILO="$(cd "$DIR/../.." && pwd)/target/release/ilo"
if [[ ! -x "$ILO" ]]; then
  echo "Build ilo first: cargo build --release --features cranelift"
  exit 1
fi

check_cmd() { command -v "$1" >/dev/null 2>&1; }

# ---------- Write benchmark programs ----------

# --- ilo ---
# numeric: sum 1..n (pure arithmetic loop)
cat > "$TMP/numeric.ilo" <<< 'f n:n>n;s=0;i=1;wh <=i n{s=+s i;i=+i 1};s'

# string: build string of n "x" chars
cat > "$TMP/string.ilo" <<< 'f n:n>t;s="";i=0;wh <i n{s=+s "x";i=+i 1};s'

# record: create structs, sum fields
printf 'type pt{x:n;y:n}\nf n:n>n;s=0;i=0;wh <i n{yv=*i 2;p=pt x:i y:yv;s=+s +p.x p.y;i=+i 1};s\n' > "$TMP/record.ilo"

# mixed: build list of records, JSON-serialise
printf 'type item{name:t;val:n}\nf n:n>t;items=[];i=0;wh <i n{nm=str i;vl=*i 3;it=item name:nm val:vl;items=+=items it;i=+i 1};jdmp items\n' > "$TMP/mixed.ilo"

# guards: classify n values via guard chains (branching)
printf 'classify x:n>n;>=x 900{30};>=x 700{25};>=x 500{20};>=x 300{15};>=x 100{10};5\nf n:n>n;s=0;i=0;wh <i n{c=classify i;s=+s c;i=+i 1};s\n' > "$TMP/guards.ilo"

# recurse: fibonacci (recursive call overhead)
cat > "$TMP/recurse.ilo" <<< 'fib n:n>n;<=n 1 n;a=fib -n 1;b=fib -n 2;+a b'

# foreach: build list of n numbers, sum via @ (foreach iteration overhead)
cat > "$TMP/foreach.ilo" <<< 'f n:n>n;xs=[];i=0;wh <i n{xs=+=xs i;i=+i 1};s=0;@x xs{s=+s x};s'

# while: sum 0..n-1 via while loop (same result as foreach, no list overhead)
cat > "$TMP/while.ilo" <<< 'f n:n>n;s=0;i=0;wh <i n{s=+s i;i=+i 1};s'

# pipe: chain function calls via >> (call overhead)
printf 'dbl x:n>n;*x 2\ninc x:n>n;+x 1\nf n:n>n;s=0;i=0;wh <i n{v=i>>dbl>>inc>>dbl>>inc;s=+s v;i=+i 1};s\n' > "$TMP/pipe.ilo"

# file: read JSON file, parse 20 records, sum scores
# Write a shared JSON fixture (20 user records)
printf '[' > "$TMP/api.json"
for i in $(seq 1 20); do
  [[ $i -gt 1 ]] && printf ',' >> "$TMP/api.json"
  printf '{"id":%d,"name":"user%d","score":%d}' "$i" "$i" "$(( (i * 7 + 13) % 100 ))" >> "$TMP/api.json"
done
printf ']' >> "$TMP/api.json"

cat > "$TMP/file.ilo" << 'ILOEOF'
f p:t>n;r=rd p;?r{~j:s=0;@u j{s=+s u.score};s;_:0}
ILOEOF

# api: HTTP GET JSON from local server, parse, sum scores
BENCH_PORT=18792
API_URL="http://127.0.0.1:$BENCH_PORT/api.json"

start_api_server() {
  python3 -m http.server $BENCH_PORT --directory "$TMP" --bind 127.0.0.1 >/dev/null 2>&1 &
  HTTP_PID=$!
  # Wait for server to be ready (up to 5 seconds)
  local tries=0
  while ! curl -sf "$API_URL" >/dev/null 2>&1; do
    tries=$((tries + 1))
    if [ $tries -ge 50 ]; then
      echo "ERROR: HTTP server failed to start" >&2
      return 1
    fi
    sleep 0.1
  done
}

stop_api_server() {
  if [ "$HTTP_PID" -gt 0 ] 2>/dev/null; then
    kill $HTTP_PID 2>/dev/null || true
    wait $HTTP_PID 2>/dev/null || true
    HTTP_PID=0
  fi
}

cat > "$TMP/api.ilo" << 'ILOEOF'
f u:t>R n t;body=get! u;items=jpar! body;s=0;@x items{s=+s x.score};~s
ILOEOF

# --- Helper: run one ilo benchmark mode, extract ns/call ---
run_ilo_jit() {
  local fn="${3:-f}"
  local out
  out=$("$ILO" "$TMP/$1.ilo" --bench "$fn" "$2" 2>&1 || true)
  echo "$out" | awk '/^Cranelift JIT$/{b=1} b && /per call/{gsub(/[^0-9]/,"",$0); print; b=0}'
}
run_ilo_vm() {
  local fn="${3:-f}"
  local out
  out=$("$ILO" "$TMP/$1.ilo" --bench "$fn" "$2" 2>&1 || true)
  echo "$out" | awk '/^Register VM \(reusable\)$/{b=1} b && /per call/{gsub(/[^0-9]/,"",$0); print; b=0}'
}
run_ilo_interp() {
  local fn="${3:-f}"
  local out
  out=$("$ILO" "$TMP/$1.ilo" --bench "$fn" "$2" 2>&1 || true)
  echo "$out" | awk '/^Rust interpreter$/{b=1} b && /per call/{gsub(/[^0-9]/,"",$0); print; b=0}'
}

# --- Go ---
cat > "$TMP/numeric.go" << GOEOF
package main
import ("fmt";"time")
func bench() int { s := 0; for i := 1; i <= 1000; i++ { s += i }; return s }
func main() {
  bench(); iters := $ITERS; start := time.Now()
  var r int; for i := 0; i < iters; i++ { r = bench() }
  e := time.Since(start); fmt.Printf("%d\n", e.Nanoseconds()/int64(iters)); _ = r
}
GOEOF

cat > "$TMP/string.go" << GOEOF
package main
import ("fmt";"time")
func bench() string { s := ""; for i := 0; i < 100; i++ { s += "x" }; return s }
func main() {
  bench(); iters := $ITERS; start := time.Now()
  var r string; for i := 0; i < iters; i++ { r = bench() }
  e := time.Since(start); fmt.Printf("%d\n", e.Nanoseconds()/int64(iters)); _ = r
}
GOEOF

cat > "$TMP/record.go" << GOEOF
package main
import ("fmt";"time")
type pt struct { x, y int }
func bench() int { s := 0; for i := 0; i < 100; i++ { p := pt{i, i*2}; s += p.x + p.y }; return s }
func main() {
  bench(); iters := $ITERS; start := time.Now()
  var r int; for i := 0; i < iters; i++ { r = bench() }
  e := time.Since(start); fmt.Printf("%d\n", e.Nanoseconds()/int64(iters)); _ = r
}
GOEOF

cat > "$TMP/mixed.go" << GOEOF
package main
import ("encoding/json";"fmt";"strconv";"time")
type item struct { Name string \`json:"name"\`; Val int \`json:"val"\` }
func bench() string {
  its := make([]item, 0, 100)
  for i := 0; i < 100; i++ { its = append(its, item{strconv.Itoa(i), i*3}) }
  b, _ := json.Marshal(its); return string(b)
}
func main() {
  bench(); iters := $ITERS; start := time.Now()
  var r string; for i := 0; i < iters; i++ { r = bench() }
  e := time.Since(start); fmt.Printf("%d\n", e.Nanoseconds()/int64(iters)); _ = r
}
GOEOF

cat > "$TMP/guards.go" << GOEOF
package main
import ("fmt";"time")
func classify(x int) int {
  if x >= 900 { return 30 }; if x >= 700 { return 25 }; if x >= 500 { return 20 }
  if x >= 300 { return 15 }; if x >= 100 { return 10 }; return 5
}
func bench() int { s := 0; for i := 0; i < 1000; i++ { s += classify(i) }; return s }
func main() {
  bench(); iters := $ITERS; start := time.Now()
  var r int; for i := 0; i < iters; i++ { r = bench() }
  e := time.Since(start); fmt.Printf("%d\n", e.Nanoseconds()/int64(iters)); _ = r
}
GOEOF

cat > "$TMP/recurse.go" << GOEOF
package main
import ("fmt";"time")
func fib(n int) int { if n<=1{return n}; return fib(n-1)+fib(n-2) }
func main() {
  fib(10); iters := $ITERS; start := time.Now()
  var r int; for i := 0; i < iters; i++ { r = fib(10) }
  e := time.Since(start); fmt.Printf("%d\n", e.Nanoseconds()/int64(iters)); _ = r
}
GOEOF

cat > "$TMP/foreach.go" << GOEOF
package main
import ("fmt";"time")
func bench(n int) int { xs := make([]int, n); for i := range xs { xs[i] = i }; s := 0; for _, x := range xs { s += x }; return s }
func main() {
  bench(100); iters := $ITERS; start := time.Now()
  var r int; for i := 0; i < iters; i++ { r = bench(100) }
  e := time.Since(start); fmt.Printf("%d\n", e.Nanoseconds()/int64(iters)); _ = r
}
GOEOF

cat > "$TMP/while.go" << GOEOF
package main
import ("fmt";"time")
func bench(n int) int { s := 0; i := 0; for i < n { s += i; i++ }; return s }
func main() {
  bench(100); iters := $ITERS; start := time.Now()
  var r int; for i := 0; i < iters; i++ { r = bench(100) }
  e := time.Since(start); fmt.Printf("%d\n", e.Nanoseconds()/int64(iters)); _ = r
}
GOEOF

cat > "$TMP/pipe.go" << GOEOF
package main
import ("fmt";"time")
func dbl(x int) int { return x*2 }
func inc(x int) int { return x+1 }
func bench(n int) int { s := 0; for i := 0; i < n; i++ { s += inc(dbl(inc(dbl(i)))) }; return s }
func main() {
  bench(100); iters := $ITERS; start := time.Now()
  var r int; for i := 0; i < iters; i++ { r = bench(100) }
  e := time.Since(start); fmt.Printf("%d\n", e.Nanoseconds()/int64(iters)); _ = r
}
GOEOF

cat > "$TMP/file.go" << GOEOF
package main
import ("encoding/json";"fmt";"os";"time")
type user struct { ID int \`json:"id"\`; Name string \`json:"name"\`; Score int \`json:"score"\` }
func bench(path string) int {
  data, _ := os.ReadFile(path)
  var users []user; json.Unmarshal(data, &users)
  s := 0; for _, u := range users { s += u.Score }; return s
}
func main() {
  p := "$TMP/api.json"
  bench(p); iters := $ITERS; start := time.Now()
  var r int; for i := 0; i < iters; i++ { r = bench(p) }
  e := time.Since(start); fmt.Printf("%d\n", e.Nanoseconds()/int64(iters)); _ = r
}
GOEOF

cat > "$TMP/api.go" << GOEOF
package main
import ("encoding/json";"fmt";"io";"net/http";"time")
type user2 struct { ID int \`json:"id"\`; Name string \`json:"name"\`; Score int \`json:"score"\` }
func bench(url string) int {
  resp, err := http.Get(url); if err != nil { return 0 }; defer resp.Body.Close()
  data, _ := io.ReadAll(resp.Body)
  var users []user2; json.Unmarshal(data, &users)
  s := 0; for _, u := range users { s += u.Score }; return s
}
func main() {
  u := "$API_URL"
  bench(u); iters := $ITERS; start := time.Now()
  var r int; for i := 0; i < iters; i++ { r = bench(u) }
  e := time.Since(start); fmt.Printf("%d\n", e.Nanoseconds()/int64(iters)); _ = r
}
GOEOF

# --- Rust ---
cat > "$TMP/numeric.rs" << RSEOF
use std::time::Instant; use std::hint::black_box;
fn f(n: i64) -> i64 { let mut s=0i64; let mut i=1; while i<=n { s+=i; i+=1; } s }
fn main() {
  black_box(f(black_box(1000)));
  let iters=${ITERS}_u128; let start=Instant::now();
  for _ in 0..iters { black_box(f(black_box(1000))); }
  println!("{}", start.elapsed().as_nanos()/iters);
}
RSEOF

cat > "$TMP/string.rs" << RSEOF
use std::time::Instant; use std::hint::black_box;
fn f(n: i64) -> String { let mut s=String::new(); for _ in 0..n { s.push_str("x"); } s }
fn main() {
  black_box(f(black_box(100)));
  let iters=${ITERS}_u128; let start=Instant::now();
  for _ in 0..iters { black_box(f(black_box(100))); }
  println!("{}", start.elapsed().as_nanos()/iters);
}
RSEOF

cat > "$TMP/record.rs" << RSEOF
use std::time::Instant; use std::hint::black_box;
struct Pt { x: i64, y: i64 }
fn f(n: i64) -> i64 { let mut s=0i64; for i in 0..n { let p=Pt{x:i,y:i*2}; s+=p.x+p.y; } s }
fn main() {
  black_box(f(black_box(100)));
  let iters=${ITERS}_u128; let start=Instant::now();
  for _ in 0..iters { black_box(f(black_box(100))); }
  println!("{}", start.elapsed().as_nanos()/iters);
}
RSEOF

cat > "$TMP/mixed.rs" << RSEOF
use std::time::Instant; use std::hint::black_box;
struct Item { name: String, val: i64 }
fn f(n: i64) -> String {
  let mut its=Vec::with_capacity(n as usize);
  for i in 0..n { its.push(Item{name:format!("{}",i),val:i*3}); }
  let mut o=String::from("[");
  for (j,it) in its.iter().enumerate() { if j>0{o.push(',');} o.push_str(&format!("{{\"name\":\"{}\",\"val\":{}}}",it.name,it.val)); }
  o.push(']'); o
}
fn main() {
  black_box(f(black_box(100)));
  let iters=${ITERS}_u128; let start=Instant::now();
  for _ in 0..iters { black_box(f(black_box(100))); }
  println!("{}", start.elapsed().as_nanos()/iters);
}
RSEOF

cat > "$TMP/guards.rs" << RSEOF
use std::time::Instant; use std::hint::black_box;
fn classify(x: i64) -> i64 {
  if x >= 900 { 30 } else if x >= 700 { 25 } else if x >= 500 { 20 }
  else if x >= 300 { 15 } else if x >= 100 { 10 } else { 5 }
}
fn f(n: i64) -> i64 { let mut s=0i64; for i in 0..n { s+=classify(i); } s }
fn main() {
  black_box(f(black_box(1000)));
  let iters=${ITERS}_u128; let start=Instant::now();
  for _ in 0..iters { black_box(f(black_box(1000))); }
  println!("{}", start.elapsed().as_nanos()/iters);
}
RSEOF

cat > "$TMP/recurse.rs" << RSEOF
use std::time::Instant; use std::hint::black_box;
fn fib(n: u64) -> u64 { if n<=1{n} else{fib(n-1)+fib(n-2)} }
fn main() {
  black_box(fib(black_box(10)));
  let iters=${ITERS}_u128; let start=Instant::now();
  for _ in 0..iters { black_box(fib(black_box(10))); }
  println!("{}", start.elapsed().as_nanos()/iters);
}
RSEOF

cat > "$TMP/foreach.rs" << RSEOF
use std::time::Instant; use std::hint::black_box;
fn f(n: i64) -> i64 { let xs: Vec<i64> = (0..n).collect(); xs.iter().sum() }
fn main() {
  black_box(f(black_box(100)));
  let iters=${ITERS}_u128; let start=Instant::now();
  for _ in 0..iters { black_box(f(black_box(100))); }
  println!("{}", start.elapsed().as_nanos()/iters);
}
RSEOF

cat > "$TMP/while.rs" << RSEOF
use std::time::Instant; use std::hint::black_box;
fn f(n: i64) -> i64 { let mut s=0i64; let mut i=0; while i<n { s+=i; i+=1; } s }
fn main() {
  black_box(f(black_box(100)));
  let iters=${ITERS}_u128; let start=Instant::now();
  for _ in 0..iters { black_box(f(black_box(100))); }
  println!("{}", start.elapsed().as_nanos()/iters);
}
RSEOF

cat > "$TMP/pipe.rs" << RSEOF
use std::time::Instant; use std::hint::black_box;
#[inline(never)] fn dbl(x: i64) -> i64 { x*2 }
#[inline(never)] fn inc(x: i64) -> i64 { x+1 }
fn f(n: i64) -> i64 { let mut s=0i64; for i in 0..n { s+=inc(dbl(inc(dbl(i)))); } s }
fn main() {
  black_box(f(black_box(100)));
  let iters=${ITERS}_u128; let start=Instant::now();
  for _ in 0..iters { black_box(f(black_box(100))); }
  println!("{}", start.elapsed().as_nanos()/iters);
}
RSEOF

cat > "$TMP/file.rs" << RSEOF
use std::time::Instant; use std::hint::black_box;
fn bench(path: &str) -> i64 {
  let data = std::fs::read_to_string(path).unwrap();
  // Minimal JSON parser: extract "score":N values
  let mut s = 0i64;
  let needle = b"\"score\":";
  let bytes = data.as_bytes();
  let mut i = 0;
  while i + needle.len() < bytes.len() {
    if &bytes[i..i+needle.len()] == needle {
      i += needle.len();
      let start = i;
      while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
      let n: i64 = data[start..i].parse().unwrap_or(0);
      s += n;
    } else { i += 1; }
  }
  s
}
fn main() {
  let p = "$TMP/api.json";
  black_box(bench(black_box(p)));
  let iters=${ITERS}_u128; let start=Instant::now();
  for _ in 0..iters { black_box(bench(black_box(p))); }
  println!("{}", start.elapsed().as_nanos()/iters);
}
RSEOF

cat > "$TMP/api.rs" << RSEOF
use std::time::Instant; use std::hint::black_box;
use std::io::{Read,Write}; use std::net::TcpStream;
fn bench(host: &str, port: u16, path: &str) -> i64 {
  let mut stream = TcpStream::connect((host, port)).unwrap();
  let req = format!("GET {} HTTP/1.0\r\nHost: {}:{}\r\n\r\n", path, host, port);
  stream.write_all(req.as_bytes()).unwrap();
  let mut buf = String::new(); stream.read_to_string(&mut buf).unwrap();
  let body = buf.split("\r\n\r\n").nth(1).unwrap_or("");
  let needle = b"\"score\":"; let bytes = body.as_bytes();
  let mut s = 0i64; let mut i = 0;
  while i + needle.len() < bytes.len() {
    if &bytes[i..i+needle.len()] == needle {
      i += needle.len(); let start = i;
      while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
      s += body[start..i].parse::<i64>().unwrap_or(0);
    } else { i += 1; }
  }
  s
}
fn main() {
  bench("127.0.0.1", $BENCH_PORT, "/api.json");
  let iters=${ITERS}_u128; let start=Instant::now();
  for _ in 0..iters { black_box(bench("127.0.0.1", $BENCH_PORT, "/api.json")); }
  println!("{}", start.elapsed().as_nanos()/iters);
}
RSEOF

# --- Node/V8 ---
cat > "$TMP/numeric.js" << JSEOF
function f(n){let s=0,i=1;while(i<=n){s+=i;i++}return s}
f(1000);const I=$ITERS,t=performance.now();let r;for(let i=0;i<I;i++)r=f(1000);
console.log(Math.round((performance.now()-t)*1e6/I));
JSEOF

cat > "$TMP/string.js" << JSEOF
function f(n){let s="";for(let i=0;i<n;i++)s+="x";return s}
f(100);const I=$ITERS,t=performance.now();let r;for(let i=0;i<I;i++)r=f(100);
console.log(Math.round((performance.now()-t)*1e6/I));
JSEOF

cat > "$TMP/record.js" << JSEOF
function f(n){let s=0;for(let i=0;i<n;i++){const p={x:i,y:i*2};s+=p.x+p.y}return s}
f(100);const I=$ITERS,t=performance.now();let r;for(let i=0;i<I;i++)r=f(100);
console.log(Math.round((performance.now()-t)*1e6/I));
JSEOF

cat > "$TMP/mixed.js" << JSEOF
function f(n){const a=[];for(let i=0;i<n;i++)a.push({name:String(i),val:i*3});return JSON.stringify(a)}
f(100);const I=$ITERS,t=performance.now();let r;for(let i=0;i<I;i++)r=f(100);
console.log(Math.round((performance.now()-t)*1e6/I));
JSEOF

cat > "$TMP/guards.js" << JSEOF
function classify(x){if(x>=900)return 30;if(x>=700)return 25;if(x>=500)return 20;if(x>=300)return 15;if(x>=100)return 10;return 5}
function f(n){let s=0;for(let i=0;i<n;i++)s+=classify(i);return s}
f(1000);const I=$ITERS,t=performance.now();let r;for(let i=0;i<I;i++)r=f(1000);
console.log(Math.round((performance.now()-t)*1e6/I));
JSEOF

cat > "$TMP/recurse.js" << JSEOF
function fib(n){return n<=1?n:fib(n-1)+fib(n-2)}
fib(10);const I=$ITERS,t=performance.now();let r;for(let i=0;i<I;i++)r=fib(10);
console.log(Math.round((performance.now()-t)*1e6/I));
JSEOF

cat > "$TMP/foreach.js" << JSEOF
function f(n){const xs=[];for(let i=0;i<n;i++)xs.push(i);let s=0;for(const x of xs)s+=x;return s}
f(100);const I=$ITERS,t=performance.now();let r;for(let i=0;i<I;i++)r=f(100);
console.log(Math.round((performance.now()-t)*1e6/I));
JSEOF

cat > "$TMP/while.js" << JSEOF
function f(n){let s=0,i=0;while(i<n){s+=i;i++}return s}
f(100);const I=$ITERS,t=performance.now();let r;for(let i=0;i<I;i++)r=f(100);
console.log(Math.round((performance.now()-t)*1e6/I));
JSEOF

cat > "$TMP/pipe.js" << JSEOF
function dbl(x){return x*2}function inc(x){return x+1}
function f(n){let s=0;for(let i=0;i<n;i++)s+=inc(dbl(inc(dbl(i))));return s}
f(100);const I=$ITERS,t=performance.now();let r;for(let i=0;i<I;i++)r=f(100);
console.log(Math.round((performance.now()-t)*1e6/I));
JSEOF

cat > "$TMP/file.js" << JSEOF
const fs=require('fs');
function f(p){const d=fs.readFileSync(p,'utf8');const users=JSON.parse(d);let s=0;for(const u of users)s+=u.score;return s}
const p='$TMP/api.json';f(p);const I=$ITERS,t=performance.now();let r;for(let i=0;i<I;i++)r=f(p);
console.log(Math.round((performance.now()-t)*1e6/I));
JSEOF

cat > "$TMP/api.js" << JSEOF
async function f(u){const r=await fetch(u);const users=await r.json();let s=0;for(const x of users)s+=x.score;return s}
const u='$API_URL';
(async()=>{await f(u);const I=$ITERS,t=performance.now();for(let i=0;i<I;i++)await f(u);
console.log(Math.round((performance.now()-t)*1e6/I));})();
JSEOF

# --- TypeScript (tsx / V8) ---
cat > "$TMP/numeric.ts" << TSEOF
function f(n: number): number { let s = 0, i = 1; while (i <= n) { s += i; i++; } return s; }
f(1000); const I = $ITERS; const t = performance.now(); let r: number = 0;
for (let i = 0; i < I; i++) r = f(1000);
console.log(Math.round((performance.now() - t) * 1e6 / I));
TSEOF

cat > "$TMP/string.ts" << TSEOF
function f(n: number): string { let s = ""; for (let i = 0; i < n; i++) s += "x"; return s; }
f(100); const I = $ITERS; const t = performance.now(); let r: string = "";
for (let i = 0; i < I; i++) r = f(100);
console.log(Math.round((performance.now() - t) * 1e6 / I));
TSEOF

cat > "$TMP/record.ts" << TSEOF
interface Pt { x: number; y: number }
function f(n: number): number { let s = 0; for (let i = 0; i < n; i++) { const p: Pt = { x: i, y: i * 2 }; s += p.x + p.y; } return s; }
f(100); const I = $ITERS; const t = performance.now(); let r: number = 0;
for (let i = 0; i < I; i++) r = f(100);
console.log(Math.round((performance.now() - t) * 1e6 / I));
TSEOF

cat > "$TMP/mixed.ts" << TSEOF
interface Item { name: string; val: number }
function f(n: number): string { const a: Item[] = []; for (let i = 0; i < n; i++) a.push({ name: String(i), val: i * 3 }); return JSON.stringify(a); }
f(100); const I = $ITERS; const t = performance.now(); let r: string = "";
for (let i = 0; i < I; i++) r = f(100);
console.log(Math.round((performance.now() - t) * 1e6 / I));
TSEOF

cat > "$TMP/guards.ts" << TSEOF
function classify(x: number): number { if (x >= 900) return 30; if (x >= 700) return 25; if (x >= 500) return 20; if (x >= 300) return 15; if (x >= 100) return 10; return 5; }
function f(n: number): number { let s = 0; for (let i = 0; i < n; i++) s += classify(i); return s; }
f(1000); const I = $ITERS; const t = performance.now(); let r: number = 0;
for (let i = 0; i < I; i++) r = f(1000);
console.log(Math.round((performance.now() - t) * 1e6 / I));
TSEOF

cat > "$TMP/recurse.ts" << TSEOF
function fib(n: number): number { return n <= 1 ? n : fib(n - 1) + fib(n - 2); }
fib(10); const I = $ITERS; const t = performance.now(); let r: number = 0;
for (let i = 0; i < I; i++) r = fib(10);
console.log(Math.round((performance.now() - t) * 1e6 / I));
TSEOF

cat > "$TMP/foreach.ts" << TSEOF
function f(n: number): number { const xs: number[] = []; for (let i = 0; i < n; i++) xs.push(i); let s = 0; for (const x of xs) s += x; return s; }
f(100); const I = $ITERS; const t = performance.now(); let r: number = 0;
for (let i = 0; i < I; i++) r = f(100);
console.log(Math.round((performance.now() - t) * 1e6 / I));
TSEOF

cat > "$TMP/while.ts" << TSEOF
function f(n: number): number { let s = 0, i = 0; while (i < n) { s += i; i++; } return s; }
f(100); const I = $ITERS; const t = performance.now(); let r: number = 0;
for (let i = 0; i < I; i++) r = f(100);
console.log(Math.round((performance.now() - t) * 1e6 / I));
TSEOF

cat > "$TMP/pipe.ts" << TSEOF
function dbl(x: number): number { return x * 2; }
function inc(x: number): number { return x + 1; }
function f(n: number): number { let s = 0; for (let i = 0; i < n; i++) s += inc(dbl(inc(dbl(i)))); return s; }
f(100); const I = $ITERS; const t = performance.now(); let r: number = 0;
for (let i = 0; i < I; i++) r = f(100);
console.log(Math.round((performance.now() - t) * 1e6 / I));
TSEOF

cat > "$TMP/file.ts" << TSEOF
import { readFileSync } from 'fs';
interface User { id: number; name: string; score: number }
function f(p: string): number { const d = readFileSync(p, 'utf8'); const users: User[] = JSON.parse(d); let s = 0; for (const u of users) s += u.score; return s; }
const p = '$TMP/api.json'; f(p); const I = $ITERS; const t = performance.now(); let r: number = 0;
for (let i = 0; i < I; i++) r = f(p);
console.log(Math.round((performance.now() - t) * 1e6 / I));
TSEOF

cat > "$TMP/api.ts" << TSEOF
interface ApiUser { id: number; name: string; score: number }
async function f(u: string): Promise<number> { const r = await fetch(u); const users: ApiUser[] = await r.json(); let s = 0; for (const x of users) s += x.score; return s; }
const u = '$API_URL';
(async () => { await f(u); const I = $ITERS; const t = performance.now(); for (let i = 0; i < I; i++) await f(u); console.log(Math.round((performance.now() - t) * 1e6 / I)); })();
TSEOF

# --- Python 3 (CPython) ---
for bench in numeric string record mixed guards recurse foreach while pipe file api; do
  case $bench in
    numeric) cat > "$TMP/$bench.py" << PYEOF
import time
def f(n):
    s, i = 0, 1
    while i <= n: s += i; i += 1
    return s
f(1000)
iters = $ITERS; start = time.monotonic_ns()
for _ in range(iters): f(1000)
print((time.monotonic_ns() - start) // iters)
PYEOF
    ;;
    string) cat > "$TMP/$bench.py" << PYEOF
import time
def f(n):
    s = ""
    for _ in range(n): s += "x"
    return s
f(100)
iters = $ITERS; start = time.monotonic_ns()
for _ in range(iters): f(100)
print((time.monotonic_ns() - start) // iters)
PYEOF
    ;;
    record) cat > "$TMP/$bench.py" << PYEOF
import time
class Pt:
    __slots__ = ('x', 'y')
    def __init__(self, x, y): self.x = x; self.y = y
def f(n):
    s = 0
    for i in range(n): p = Pt(i, i*2); s += p.x + p.y
    return s
f(100)
iters = $ITERS; start = time.monotonic_ns()
for _ in range(iters): f(100)
print((time.monotonic_ns() - start) // iters)
PYEOF
    ;;
    mixed) cat > "$TMP/$bench.py" << PYEOF
import time, json
def f(n):
    items = []
    for i in range(n): items.append({"name": str(i), "val": i*3})
    return json.dumps(items)
f(100)
iters = $ITERS; start = time.monotonic_ns()
for _ in range(iters): f(100)
print((time.monotonic_ns() - start) // iters)
PYEOF
    ;;
    guards) cat > "$TMP/$bench.py" << PYEOF
import time
def classify(x):
    if x >= 900: return 30
    if x >= 700: return 25
    if x >= 500: return 20
    if x >= 300: return 15
    if x >= 100: return 10
    return 5
def f(n):
    s = 0
    for i in range(n): s += classify(i)
    return s
f(1000)
iters = $ITERS; start = time.monotonic_ns()
for _ in range(iters): f(1000)
print((time.monotonic_ns() - start) // iters)
PYEOF
    ;;
    recurse) cat > "$TMP/$bench.py" << PYEOF
import time
def fib(n):
    if n <= 1: return n
    return fib(n-1) + fib(n-2)
fib(10)
iters = $ITERS; start = time.monotonic_ns()
for _ in range(iters): fib(10)
print((time.monotonic_ns() - start) // iters)
PYEOF
    ;;
    foreach) cat > "$TMP/$bench.py" << PYEOF
import time
def f(n):
    xs = list(range(n))
    s = 0
    for x in xs: s += x
    return s
f(100)
iters = $ITERS; start = time.monotonic_ns()
for _ in range(iters): f(100)
print((time.monotonic_ns() - start) // iters)
PYEOF
    ;;
    while) cat > "$TMP/$bench.py" << PYEOF
import time
def f(n):
    s, i = 0, 0
    while i < n: s += i; i += 1
    return s
f(100)
iters = $ITERS; start = time.monotonic_ns()
for _ in range(iters): f(100)
print((time.monotonic_ns() - start) // iters)
PYEOF
    ;;
    pipe) cat > "$TMP/$bench.py" << PYEOF
import time
def dbl(x): return x * 2
def inc(x): return x + 1
def f(n):
    s = 0
    for i in range(n): s += inc(dbl(inc(dbl(i))))
    return s
f(100)
iters = $ITERS; start = time.monotonic_ns()
for _ in range(iters): f(100)
print((time.monotonic_ns() - start) // iters)
PYEOF
    ;;
    file) cat > "$TMP/$bench.py" << PYEOF
import time, json
def f(p):
    data = open(p).read()
    users = json.loads(data)
    s = 0
    for u in users: s += u['score']
    return s
p = '$TMP/api.json'
f(p)
iters = $ITERS; start = time.monotonic_ns()
for _ in range(iters): f(p)
print((time.monotonic_ns() - start) // iters)
PYEOF
    ;;
    api) cat > "$TMP/$bench.py" << PYEOF
import time, json, urllib.request
def f(u):
    r = urllib.request.urlopen(u)
    data = r.read().decode('utf-8')
    users = json.loads(data)
    s = 0
    for x in users: s += x['score']
    return s
u = '$API_URL'
f(u)
iters = $ITERS; start = time.monotonic_ns()
for _ in range(iters): f(u)
print((time.monotonic_ns() - start) // iters)
PYEOF
    ;;
  esac
done

# --- Lua ---
cat > "$TMP/numeric.lua" << LUAEOF
local function f(n) local s=0; for i=1,n do s=s+i end; return s end
f(1000); local iters=$ITERS; local t=os.clock()
for _=1,iters do f(1000) end
print(math.floor((os.clock()-t)*1e9/iters))
LUAEOF

cat > "$TMP/string.lua" << LUAEOF
local function f(n) local s=""; for _=1,n do s=s.."x" end; return s end
f(100); local iters=$ITERS; local t=os.clock()
for _=1,iters do f(100) end
print(math.floor((os.clock()-t)*1e9/iters))
LUAEOF

cat > "$TMP/record.lua" << LUAEOF
local function f(n) local s=0; for i=0,n-1 do local p={x=i,y=i*2}; s=s+p.x+p.y end; return s end
f(100); local iters=$ITERS; local t=os.clock()
for _=1,iters do f(100) end
print(math.floor((os.clock()-t)*1e9/iters))
LUAEOF

cat > "$TMP/mixed.lua" << LUAEOF
local function jenc(items)
  local p={}; for _,it in ipairs(items) do p[#p+1]=string.format('{"name":"%s","val":%d}',it.name,it.val) end
  return "["..table.concat(p,",").."]"
end
local function f(n)
  local a={}; for i=0,n-1 do a[#a+1]={name=tostring(i),val=i*3} end; return jenc(a)
end
f(100); local iters=$ITERS; local t=os.clock()
for _=1,iters do f(100) end
print(math.floor((os.clock()-t)*1e9/iters))
LUAEOF

cat > "$TMP/guards.lua" << LUAEOF
local function classify(x)
  if x >= 900 then return 30 end; if x >= 700 then return 25 end
  if x >= 500 then return 20 end; if x >= 300 then return 15 end
  if x >= 100 then return 10 end; return 5
end
local function f(n) local s=0; for i=0,n-1 do s=s+classify(i) end; return s end
f(1000); local iters=$ITERS; local t=os.clock()
for _=1,iters do f(1000) end
print(math.floor((os.clock()-t)*1e9/iters))
LUAEOF

cat > "$TMP/recurse.lua" << LUAEOF
local function fib(n) if n<=1 then return n end; return fib(n-1)+fib(n-2) end
fib(10); local iters=$ITERS; local t=os.clock()
for _=1,iters do fib(10) end
print(math.floor((os.clock()-t)*1e9/iters))
LUAEOF

cat > "$TMP/foreach.lua" << LUAEOF
local function f(n) local xs={}; for i=0,n-1 do xs[#xs+1]=i end; local s=0; for _,x in ipairs(xs) do s=s+x end; return s end
f(100); local iters=$ITERS; local t=os.clock()
for _=1,iters do f(100) end
print(math.floor((os.clock()-t)*1e9/iters))
LUAEOF

cat > "$TMP/while.lua" << LUAEOF
local function f(n) local s=0; local i=0; while i<n do s=s+i; i=i+1 end; return s end
f(100); local iters=$ITERS; local t=os.clock()
for _=1,iters do f(100) end
print(math.floor((os.clock()-t)*1e9/iters))
LUAEOF

cat > "$TMP/pipe.lua" << LUAEOF
local function dbl(x) return x*2 end
local function inc(x) return x+1 end
local function f(n) local s=0; for i=0,n-1 do s=s+inc(dbl(inc(dbl(i)))) end; return s end
f(100); local iters=$ITERS; local t=os.clock()
for _=1,iters do f(100) end
print(math.floor((os.clock()-t)*1e9/iters))
LUAEOF

cat > "$TMP/file.lua" << LUAEOF
local function f(path)
  local fh=io.open(path,"r"); local data=fh:read("*a"); fh:close()
  local sum=0
  for sc in data:gmatch('"score":(%d+)') do sum=sum+tonumber(sc) end
  return sum
end
local p="$TMP/api.json"
f(p); local iters=$ITERS; local t=os.clock()
for _=1,iters do f(p) end
print(math.floor((os.clock()-t)*1e9/iters))
LUAEOF

cat > "$TMP/api.lua" << LUAEOF
package.path=package.path..";/Users/dan/.luarocks/share/lua/5.1/?.lua;/Users/dan/.luarocks/share/lua/5.1/?/init.lua"
package.cpath=package.cpath..";/Users/dan/.luarocks/lib/lua/5.1/?.so"
local http=require("socket.http")
local function f(url)
  local body=http.request(url)
  local sum=0
  for sc in body:gmatch('"score":(%d+)') do sum=sum+tonumber(sc) end
  return sum
end
local u="$API_URL"
f(u); local iters=$ITERS; local t=os.clock()
for _=1,iters do f(u) end
print(math.floor((os.clock()-t)*1e9/iters))
LUAEOF

# --- Ruby ---
cat > "$TMP/numeric.rb" << RBEOF
def f(n);s=0;i=1;while i<=n;s+=i;i+=1;end;s;end
f(1000);iters=$ITERS;t=Process.clock_gettime(Process::CLOCK_MONOTONIC)
iters.times{f(1000)};e=Process.clock_gettime(Process::CLOCK_MONOTONIC)-t
puts (e*1e9/iters).to_i
RBEOF

cat > "$TMP/string.rb" << RBEOF
def f(n);s="";i=0;while i<n;s+="x";i+=1;end;s;end
f(100);iters=$ITERS;t=Process.clock_gettime(Process::CLOCK_MONOTONIC)
iters.times{f(100)};e=Process.clock_gettime(Process::CLOCK_MONOTONIC)-t
puts (e*1e9/iters).to_i
RBEOF

cat > "$TMP/record.rb" << RBEOF
Pt=Struct.new(:x,:y)
def f(n);s=0;i=0;while i<n;p=Pt.new(i,i*2);s+=p.x+p.y;i+=1;end;s;end
f(100);iters=$ITERS;t=Process.clock_gettime(Process::CLOCK_MONOTONIC)
iters.times{f(100)};e=Process.clock_gettime(Process::CLOCK_MONOTONIC)-t
puts (e*1e9/iters).to_i
RBEOF

cat > "$TMP/mixed.rb" << RBEOF
require 'json'
def f(n);a=[];i=0;while i<n;a<<{name:i.to_s,val:i*3};i+=1;end;JSON.generate(a);end
f(100);iters=$ITERS;t=Process.clock_gettime(Process::CLOCK_MONOTONIC)
iters.times{f(100)};e=Process.clock_gettime(Process::CLOCK_MONOTONIC)-t
puts (e*1e9/iters).to_i
RBEOF

cat > "$TMP/guards.rb" << RBEOF
def classify(x);return 30 if x>=900;return 25 if x>=700;return 20 if x>=500;return 15 if x>=300;return 10 if x>=100;5;end
def f(n);s=0;i=0;while i<n;s+=classify(i);i+=1;end;s;end
f(1000);iters=$ITERS;t=Process.clock_gettime(Process::CLOCK_MONOTONIC)
iters.times{f(1000)};e=Process.clock_gettime(Process::CLOCK_MONOTONIC)-t
puts (e*1e9/iters).to_i
RBEOF

cat > "$TMP/recurse.rb" << RBEOF
def fib(n);n<=1?n:fib(n-1)+fib(n-2);end
fib(10);iters=$ITERS;t=Process.clock_gettime(Process::CLOCK_MONOTONIC)
iters.times{fib(10)};e=Process.clock_gettime(Process::CLOCK_MONOTONIC)-t
puts (e*1e9/iters).to_i
RBEOF

cat > "$TMP/foreach.rb" << RBEOF
def f(n);xs=(0...n).to_a;s=0;xs.each{|x|s+=x};s;end
f(100);iters=$ITERS;t=Process.clock_gettime(Process::CLOCK_MONOTONIC)
iters.times{f(100)};e=Process.clock_gettime(Process::CLOCK_MONOTONIC)-t
puts (e*1e9/iters).to_i
RBEOF

cat > "$TMP/while.rb" << RBEOF
def f(n);s=0;i=0;while i<n;s+=i;i+=1;end;s;end
f(100);iters=$ITERS;t=Process.clock_gettime(Process::CLOCK_MONOTONIC)
iters.times{f(100)};e=Process.clock_gettime(Process::CLOCK_MONOTONIC)-t
puts (e*1e9/iters).to_i
RBEOF

cat > "$TMP/pipe.rb" << RBEOF
def dbl(x);x*2;end
def inc(x);x+1;end
def f(n);s=0;i=0;while i<n;s+=inc(dbl(inc(dbl(i))));i+=1;end;s;end
f(100);iters=$ITERS;t=Process.clock_gettime(Process::CLOCK_MONOTONIC)
iters.times{f(100)};e=Process.clock_gettime(Process::CLOCK_MONOTONIC)-t
puts (e*1e9/iters).to_i
RBEOF

cat > "$TMP/file.rb" << RBEOF
require 'json'
def f(p);d=File.read(p);users=JSON.parse(d);s=0;users.each{|u|s+=u['score']};s;end
p='$TMP/api.json'
f(p);iters=$ITERS;t=Process.clock_gettime(Process::CLOCK_MONOTONIC)
iters.times{f(p)};e=Process.clock_gettime(Process::CLOCK_MONOTONIC)-t
puts (e*1e9/iters).to_i
RBEOF

cat > "$TMP/api.rb" << RBEOF
require 'json'; require 'net/http'; require 'uri'
def f(u);r=Net::HTTP.get(URI(u));users=JSON.parse(r);s=0;users.each{|x|s+=x['score']};s;end
u='$API_URL'
f(u);iters=$ITERS;t=Process.clock_gettime(Process::CLOCK_MONOTONIC)
iters.times{f(u)};e=Process.clock_gettime(Process::CLOCK_MONOTONIC)-t
puts (e*1e9/iters).to_i
RBEOF

# ---------- Compile ----------
# --- PHP ---
cat > "$TMP/numeric.php" << 'PHPEOF'
<?php
function f($n) { $s=0; for($i=1;$i<=$n;$i++) $s+=$i; return $s; }
f(1000); $iters=ITERS; $t=hrtime(true);
for($i=0;$i<$iters;$i++) f(1000);
echo intdiv(hrtime(true)-$t,$iters)."\n";
PHPEOF

cat > "$TMP/string.php" << 'PHPEOF'
<?php
function f($n) { $s=""; for($i=0;$i<$n;$i++) $s.="x"; return $s; }
f(100); $iters=ITERS; $t=hrtime(true);
for($i=0;$i<$iters;$i++) f(100);
echo intdiv(hrtime(true)-$t,$iters)."\n";
PHPEOF

cat > "$TMP/record.php" << 'PHPEOF'
<?php
function f($n) { $s=0; for($i=0;$i<$n;$i++) { $p=['x'=>$i,'y'=>$i*2]; $s+=$p['x']+$p['y']; } return $s; }
f(100); $iters=ITERS; $t=hrtime(true);
for($i=0;$i<$iters;$i++) f(100);
echo intdiv(hrtime(true)-$t,$iters)."\n";
PHPEOF

cat > "$TMP/mixed.php" << 'PHPEOF'
<?php
function f($n) { $a=[]; for($i=0;$i<$n;$i++) $a[]=['name'=>(string)$i,'val'=>$i*3]; return json_encode($a); }
f(100); $iters=ITERS; $t=hrtime(true);
for($i=0;$i<$iters;$i++) f(100);
echo intdiv(hrtime(true)-$t,$iters)."\n";
PHPEOF

cat > "$TMP/guards.php" << 'PHPEOF'
<?php
function classify($x) { if($x>=900) return 30; if($x>=700) return 25; if($x>=500) return 20; if($x>=300) return 15; if($x>=100) return 10; return 5; }
function f($n) { $s=0; for($i=0;$i<$n;$i++) $s+=classify($i); return $s; }
f(1000); $iters=ITERS; $t=hrtime(true);
for($i=0;$i<$iters;$i++) f(1000);
echo intdiv(hrtime(true)-$t,$iters)."\n";
PHPEOF

cat > "$TMP/recurse.php" << 'PHPEOF'
<?php
function fib($n) { return $n<=1?$n:fib($n-1)+fib($n-2); }
fib(10); $iters=ITERS; $t=hrtime(true);
for($i=0;$i<$iters;$i++) fib(10);
echo intdiv(hrtime(true)-$t,$iters)."\n";
PHPEOF

cat > "$TMP/foreach.php" << 'PHPEOF'
<?php
function f($n) { $xs=range(0,$n-1); $s=0; foreach($xs as $x) $s+=$x; return $s; }
f(100); $iters=ITERS; $t=hrtime(true);
for($i=0;$i<$iters;$i++) f(100);
echo intdiv(hrtime(true)-$t,$iters)."\n";
PHPEOF

cat > "$TMP/while.php" << 'PHPEOF'
<?php
function f($n) { $s=0; $i=0; while($i<$n) { $s+=$i; $i++; } return $s; }
f(100); $iters=ITERS; $t=hrtime(true);
for($i=0;$i<$iters;$i++) f(100);
echo intdiv(hrtime(true)-$t,$iters)."\n";
PHPEOF

cat > "$TMP/pipe.php" << 'PHPEOF'
<?php
function dbl($x) { return $x*2; }
function inc2($x) { return $x+1; }
function f($n) { $s=0; for($i=0;$i<$n;$i++) $s+=inc2(dbl(inc2(dbl($i)))); return $s; }
f(100); $iters=ITERS; $t=hrtime(true);
for($i=0;$i<$iters;$i++) f(100);
echo intdiv(hrtime(true)-$t,$iters)."\n";
PHPEOF

cat > "$TMP/file.php" << PHPEOF
<?php
function f(\$p) { \$d=file_get_contents(\$p); \$users=json_decode(\$d,true); \$s=0; foreach(\$users as \$u) \$s+=\$u['score']; return \$s; }
\$p='$TMP/api.json';
f(\$p); \$iters=$ITERS; \$t=hrtime(true);
for(\$i=0;\$i<\$iters;\$i++) f(\$p);
echo intdiv(hrtime(true)-\$t,\$iters)."\\n";
PHPEOF

cat > "$TMP/api.php" << PHPEOF
<?php
function f(\$u) { \$d=file_get_contents(\$u); \$users=json_decode(\$d,true); \$s=0; foreach(\$users as \$x) \$s+=\$x['score']; return \$s; }
\$u='$API_URL';
f(\$u); \$iters=$ITERS; \$t=hrtime(true);
for(\$i=0;\$i<\$iters;\$i++) f(\$u);
echo intdiv(hrtime(true)-\$t,\$iters)."\\n";
PHPEOF

# Replace ITERS placeholder in PHP files
for f in "$TMP"/*.php; do
  sed -i '' "s/ITERS/$ITERS/g" "$f"
done

# --- C# (.NET) ---
mkdir -p "$TMP/csharp"
cat > "$TMP/csharp/bench.csproj" << 'CSEOF'
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net10.0</TargetFramework>
    <ImplicitUsings>enable</ImplicitUsings>
  </PropertyGroup>
</Project>
CSEOF

for bench in numeric string record mixed guards recurse foreach while pipe file api; do
  mkdir -p "$TMP/csharp_$bench"
  cp "$TMP/csharp/bench.csproj" "$TMP/csharp_$bench/bench.csproj"
  case $bench in
    numeric) cat > "$TMP/csharp_$bench/Program.cs" << CSEOF
using System.Diagnostics;
static long F(long n) { long s = 0, i = 1; while (i <= n) { s += i; i++; } return s; }
F(1000); int iters = $ITERS; var sw = Stopwatch.StartNew();
for (int i = 0; i < iters; i++) F(1000);
sw.Stop(); Console.WriteLine((long)(sw.Elapsed.TotalMilliseconds * 1e6 / iters));
CSEOF
    ;;
    string) cat > "$TMP/csharp_$bench/Program.cs" << CSEOF
using System.Diagnostics;
static string F(int n) { string s = ""; for (int i = 0; i < n; i++) s += "x"; return s; }
F(100); int iters = $ITERS; var sw = Stopwatch.StartNew();
for (int i = 0; i < iters; i++) F(100);
sw.Stop(); Console.WriteLine((long)(sw.Elapsed.TotalMilliseconds * 1e6 / iters));
CSEOF
    ;;
    record) cat > "$TMP/csharp_$bench/Program.cs" << CSEOF
using System.Diagnostics;
static long F(int n) { long s = 0; for (int i = 0; i < n; i++) { var p = (X: (long)i, Y: (long)(i*2)); s += p.X + p.Y; } return s; }
F(100); int iters = $ITERS; var sw = Stopwatch.StartNew();
for (int i = 0; i < iters; i++) F(100);
sw.Stop(); Console.WriteLine((long)(sw.Elapsed.TotalMilliseconds * 1e6 / iters));
CSEOF
    ;;
    mixed) cat > "$TMP/csharp_$bench/Program.cs" << CSEOF
using System.Diagnostics; using System.Text.Json;
static string F(int n) { var a = new List<Dictionary<string,object>>(); for (int i = 0; i < n; i++) a.Add(new Dictionary<string,object>{["name"]=i.ToString(),["val"]=i*3}); return JsonSerializer.Serialize(a); }
F(100); int iters = $ITERS; var sw = Stopwatch.StartNew();
for (int i = 0; i < iters; i++) F(100);
sw.Stop(); Console.WriteLine((long)(sw.Elapsed.TotalMilliseconds * 1e6 / iters));
CSEOF
    ;;
    guards) cat > "$TMP/csharp_$bench/Program.cs" << CSEOF
using System.Diagnostics;
static int Classify(int x) { if (x>=900) return 30; if (x>=700) return 25; if (x>=500) return 20; if (x>=300) return 15; if (x>=100) return 10; return 5; }
static long F(int n) { long s = 0; for (int i = 0; i < n; i++) s += Classify(i); return s; }
F(1000); int iters = $ITERS; var sw = Stopwatch.StartNew();
for (int i = 0; i < iters; i++) F(1000);
sw.Stop(); Console.WriteLine((long)(sw.Elapsed.TotalMilliseconds * 1e6 / iters));
CSEOF
    ;;
    recurse) cat > "$TMP/csharp_$bench/Program.cs" << CSEOF
using System.Diagnostics;
static long Fib(long n) { return n <= 1 ? n : Fib(n-1) + Fib(n-2); }
Fib(10); int iters = $ITERS; var sw = Stopwatch.StartNew();
for (int i = 0; i < iters; i++) Fib(10);
sw.Stop(); Console.WriteLine((long)(sw.Elapsed.TotalMilliseconds * 1e6 / iters));
CSEOF
    ;;
    foreach) cat > "$TMP/csharp_$bench/Program.cs" << CSEOF
using System.Diagnostics;
static long F(int n) { var xs = new List<long>(n); for (int i = 0; i < n; i++) xs.Add(i); long s = 0; foreach (var x in xs) s += x; return s; }
F(100); int iters = $ITERS; var sw = Stopwatch.StartNew();
for (int i = 0; i < iters; i++) F(100);
sw.Stop(); Console.WriteLine((long)(sw.Elapsed.TotalMilliseconds * 1e6 / iters));
CSEOF
    ;;
    while) cat > "$TMP/csharp_$bench/Program.cs" << CSEOF
using System.Diagnostics;
static long F(int n) { long s = 0; int i = 0; while (i < n) { s += i; i++; } return s; }
F(100); int iters = $ITERS; var sw = Stopwatch.StartNew();
for (int i = 0; i < iters; i++) F(100);
sw.Stop(); Console.WriteLine((long)(sw.Elapsed.TotalMilliseconds * 1e6 / iters));
CSEOF
    ;;
    pipe) cat > "$TMP/csharp_$bench/Program.cs" << CSEOF
using System.Diagnostics;
using System.Runtime.CompilerServices;
[MethodImpl(MethodImplOptions.NoInlining)] static long Dbl(long x) => x * 2;
[MethodImpl(MethodImplOptions.NoInlining)] static long Inc(long x) => x + 1;
static long F(int n) { long s = 0; for (int i = 0; i < n; i++) s += Inc(Dbl(Inc(Dbl(i)))); return s; }
F(100); int iters = $ITERS; var sw = Stopwatch.StartNew();
for (int i = 0; i < iters; i++) F(100);
sw.Stop(); Console.WriteLine((long)(sw.Elapsed.TotalMilliseconds * 1e6 / iters));
CSEOF
    ;;
    file) cat > "$TMP/csharp_$bench/Program.cs" << CSEOF
using System.Diagnostics; using System.Text.Json;
static long F(string p) { var d = File.ReadAllText(p); using var doc = JsonDocument.Parse(d); long s = 0; foreach (var el in doc.RootElement.EnumerateArray()) s += el.GetProperty("score").GetInt64(); return s; }
var p = "$TMP/api.json";
F(p); int iters = $ITERS; var sw = Stopwatch.StartNew();
for (int i = 0; i < iters; i++) F(p);
sw.Stop(); Console.WriteLine((long)(sw.Elapsed.TotalMilliseconds * 1e6 / iters));
CSEOF
    ;;
    api) cat > "$TMP/csharp_$bench/Program.cs" << CSEOF
using System.Diagnostics; using System.Text.Json;
var client = new HttpClient();
static long F(HttpClient c, string u) { var d = c.GetStringAsync(u).Result; using var doc = JsonDocument.Parse(d); long s = 0; foreach (var el in doc.RootElement.EnumerateArray()) s += el.GetProperty("score").GetInt64(); return s; }
var u = "$API_URL";
F(client, u); int iters = $ITERS; var sw = Stopwatch.StartNew();
for (int i = 0; i < iters; i++) F(client, u);
sw.Stop(); Console.WriteLine((long)(sw.Elapsed.TotalMilliseconds * 1e6 / iters));
CSEOF
    ;;
  esac
done

# --- Kotlin ---
for bench in numeric string record mixed guards recurse foreach while pipe file api; do
  case $bench in
    numeric) cat > "$TMP/$bench.kt" << KTEOF
fun f(n: Long): Long { var s = 0L; var i = 1L; while (i <= n) { s += i; i++ }; return s }
fun main() { f(1000); val iters = $ITERS; val t = System.nanoTime(); repeat(iters) { f(1000) }; println((System.nanoTime()-t)/iters) }
KTEOF
    ;;
    string) cat > "$TMP/$bench.kt" << KTEOF
fun f(n: Int): String { var s = ""; for (i in 0 until n) s += "x"; return s }
fun main() { f(100); val iters = $ITERS; val t = System.nanoTime(); repeat(iters) { f(100) }; println((System.nanoTime()-t)/iters) }
KTEOF
    ;;
    record) cat > "$TMP/$bench.kt" << KTEOF
data class Pt(val x: Long, val y: Long)
fun f(n: Int): Long { var s = 0L; for (i in 0 until n) { val p = Pt(i.toLong(), i*2L); s += p.x + p.y }; return s }
fun main() { f(100); val iters = $ITERS; val t = System.nanoTime(); repeat(iters) { f(100) }; println((System.nanoTime()-t)/iters) }
KTEOF
    ;;
    mixed) cat > "$TMP/$bench.kt" << KTEOF
fun f(n: Int): String { val a = mutableListOf<String>(); for (i in 0 until n) a.add("{\"name\":\"" + i + "\",\"val\":" + (i*3) + "}"); return "[" + a.joinToString(",") + "]" }
fun main() { f(100); val iters = $ITERS; val t = System.nanoTime(); repeat(iters) { f(100) }; println((System.nanoTime()-t)/iters) }
KTEOF
    ;;
    guards) cat > "$TMP/$bench.kt" << KTEOF
fun classify(x: Int): Int { return if (x>=900) 30 else if (x>=700) 25 else if (x>=500) 20 else if (x>=300) 15 else if (x>=100) 10 else 5 }
fun f(n: Int): Long { var s = 0L; for (i in 0 until n) s += classify(i); return s }
fun main() { f(1000); val iters = $ITERS; val t = System.nanoTime(); repeat(iters) { f(1000) }; println((System.nanoTime()-t)/iters) }
KTEOF
    ;;
    recurse) cat > "$TMP/$bench.kt" << KTEOF
fun fib(n: Int): Long { return if (n<=1) n.toLong() else fib(n-1)+fib(n-2) }
fun main() { fib(10); val iters = $ITERS; val t = System.nanoTime(); repeat(iters) { fib(10) }; println((System.nanoTime()-t)/iters) }
KTEOF
    ;;
    foreach) cat > "$TMP/$bench.kt" << KTEOF
fun f(n: Int): Long { val xs = (0L until n).toList(); var s = 0L; for (x in xs) s += x; return s }
fun main() { f(100); val iters = $ITERS; val t = System.nanoTime(); repeat(iters) { f(100) }; println((System.nanoTime()-t)/iters) }
KTEOF
    ;;
    while) cat > "$TMP/$bench.kt" << KTEOF
fun f(n: Int): Long { var s = 0L; var i = 0; while (i < n) { s += i; i++ }; return s }
fun main() { f(100); val iters = $ITERS; val t = System.nanoTime(); repeat(iters) { f(100) }; println((System.nanoTime()-t)/iters) }
KTEOF
    ;;
    pipe) cat > "$TMP/$bench.kt" << KTEOF
fun dbl(x: Long): Long = x * 2
fun inc(x: Long): Long = x + 1
fun f(n: Int): Long { var s = 0L; for (i in 0 until n) s += inc(dbl(inc(dbl(i.toLong())))); return s }
fun main() { f(100); val iters = $ITERS; val t = System.nanoTime(); repeat(iters) { f(100) }; println((System.nanoTime()-t)/iters) }
KTEOF
    ;;
    file) cat > "$TMP/$bench.kt" << KTEOF
fun f(p: String): Long { val d = java.io.File(p).readText(); val re = Regex("\"score\":(\\\\d+)"); var s = 0L; for (m in re.findAll(d)) s += m.groupValues[1].toLong(); return s }
fun main() { val p = "$TMP/api.json"; f(p); val iters = $ITERS; val t = System.nanoTime(); repeat(iters) { f(p) }; println((System.nanoTime()-t)/iters) }
KTEOF
    ;;
    api) cat > "$TMP/$bench.kt" << KTEOF
fun f(u: String): Long { val d = java.net.URL(u).readText(); val re = Regex("\"score\":(\\\\d+)"); var s = 0L; for (m in re.findAll(d)) s += m.groupValues[1].toLong(); return s }
fun main() { val u = "$API_URL"; f(u); val iters = $ITERS; val t = System.nanoTime(); repeat(iters) { f(u) }; println((System.nanoTime()-t)/iters) }
KTEOF
    ;;
  esac
done

BENCHES=(numeric string record mixed guards recurse foreach while pipe file api)

echo "Compiling..." >&2
COMPILE_PIDS=()
for bench in "${BENCHES[@]}"; do
  if check_cmd go && [[ -f "$TMP/$bench.go" ]]; then
    go build -o "$TMP/${bench}_go" "$TMP/$bench.go" &
    COMPILE_PIDS+=($!)
  fi
done
for bench in "${BENCHES[@]}"; do
  if check_cmd rustup && [[ -f "$TMP/$bench.rs" ]]; then
    rustup run stable rustc -O "$TMP/$bench.rs" -o "$TMP/${bench}_rs" 2>/dev/null &
    COMPILE_PIDS+=($!)
  fi
done
# Kotlin — compile each to jar (in parallel)
for bench in "${BENCHES[@]}"; do
  if check_cmd kotlinc && [[ -f "$TMP/$bench.kt" ]]; then
    kotlinc "$TMP/$bench.kt" -include-runtime -d "$TMP/${bench}_kt.jar" 2>/dev/null &
    COMPILE_PIDS+=($!)
  fi
done
# C# — build each project (in parallel)
for bench in "${BENCHES[@]}"; do
  if check_cmd dotnet && [[ -d "$TMP/csharp_$bench" ]]; then
    dotnet build "$TMP/csharp_$bench/bench.csproj" -c Release -o "$TMP/csharp_${bench}_out" --nologo -v q 2>/dev/null &
    COMPILE_PIDS+=($!)
  fi
done
# ilo AOT — compile eligible benchmarks to native bench binaries
for bench in "${BENCHES[@]}"; do
  if [[ -f "$TMP/$bench.ilo" ]]; then
    case $bench in
      recurse) aot_fn="fib" ;;
      *)       aot_fn="f"   ;;
    esac
    "$ILO" compile "$TMP/$bench.ilo" -o "$TMP/${bench}_aot" --bench "$aot_fn" 2>/dev/null &
    COMPILE_PIDS+=($!)
    # Also compile a non-bench binary for result validation (same entry function)
    "$ILO" compile "$TMP/$bench.ilo" -o "$TMP/${bench}_aot_check" "$aot_fn" 2>/dev/null &
    COMPILE_PIDS+=($!)
  fi
done
for pid in "${COMPILE_PIDS[@]}"; do wait "$pid" 2>/dev/null || true; done

# Validate AOT binaries produce correct results — remove bench binary if wrong
# Skip api (needs HTTP server not yet started)
for spec in numeric:1000 string:100 record:100 mixed:100 guards:1000 recurse:10 foreach:100 while:100 pipe:100 file:$TMP/api.json; do
  bench="${spec%%:*}"
  arg="${spec#*:}"
  if [[ -x "$TMP/${bench}_aot_check" ]]; then
    aot_result=$("$TMP/${bench}_aot_check" "$arg" 2>/dev/null || true)
    case $bench in
      recurse) ilo_fn="fib" ;;
      *)       ilo_fn="f"   ;;
    esac
    vm_result=$("$ILO" "$TMP/$bench.ilo" "$ilo_fn" "$arg" 2>/dev/null || true)
    if [[ "$aot_result" != "$vm_result" ]]; then
      rm -f "$TMP/${bench}_aot"
    fi
    rm -f "$TMP/${bench}_aot_check"
  fi
done
# api AOT validation is deferred to after the HTTP server starts

# ---------- Run ----------
echo "Running benchmarks ($ITERS iterations)..." >&2
echo ""

RESULTS="$TMP/results.txt"
> "$RESULTS"

# Safe runner: record result only if we get a valid number
try_run() {
  local bench="$1" lang="$2"
  shift 2
  local ns
  ns=$("$@" 2>/dev/null || true)
  if [[ "$ns" =~ ^[0-9]+$ && "$ns" -gt 0 ]]; then
    echo "$bench $lang $ns" >> "$RESULTS"
  fi
}

for spec in numeric:1000 string:100 record:100 mixed:100 guards:1000 recurse:10 foreach:100 while:100 pipe:100 file:$TMP/api.json api:$API_URL; do
  bench="${spec%%:*}"
  arg="${spec#*:}"   # use single # to strip only the shortest prefix (bench:)
  echo "  $bench..." >&2

  # Start HTTP server for api benchmark, validate AOT, stop when done
  if [[ "$bench" == "api" ]]; then
    start_api_server
    # Validate api AOT now that the server is running
    if [[ -x "$TMP/api_aot_check" ]]; then
      aot_result=$("$TMP/api_aot_check" "$API_URL" 2>/dev/null || true)
      vm_result=$("$ILO" "$TMP/api.ilo" "f" "$API_URL" 2>/dev/null || true)
      if [[ "$aot_result" != "$vm_result" ]]; then
        rm -f "$TMP/api_aot"
      fi
      rm -f "$TMP/api_aot_check"
    fi
  fi

  # Compiled languages
  [[ -x "$TMP/${bench}_rs" ]] && try_run "$bench" rust "$TMP/${bench}_rs"
  [[ -x "$TMP/${bench}_go" ]] && try_run "$bench" go "$TMP/${bench}_go"

  # JIT runtimes
  check_cmd luajit && [[ -f "$TMP/$bench.lua" ]] && try_run "$bench" luajit luajit "$TMP/$bench.lua"
  check_cmd node && [[ -f "$TMP/$bench.js" ]] && try_run "$bench" node node "$TMP/$bench.js"

  # TypeScript (tsx)
  check_cmd tsx && [[ -f "$TMP/$bench.ts" ]] && try_run "$bench" tsx tsx "$TMP/$bench.ts"

  # ilo modes
  case $bench in
    recurse) ilo_fn="fib" ;;
    *)       ilo_fn="f"   ;;
  esac
  if [[ -f "$TMP/$bench.ilo" ]]; then
    # AOT (native binary with timing loop — only works for numeric-only programs)
    if [[ -x "$TMP/${bench}_aot" ]]; then
      try_run "$bench" ilo_aot "$TMP/${bench}_aot" "$ITERS" "$arg"
    fi
    jit_ns=$(run_ilo_jit "$bench" "$arg" "$ilo_fn")
    [[ "$jit_ns" =~ ^[0-9]+$ && "$jit_ns" -gt 0 ]] && echo "$bench ilo_jit $jit_ns" >> "$RESULTS"
    vm_ns=$(run_ilo_vm "$bench" "$arg" "$ilo_fn")
    [[ "$vm_ns" =~ ^[0-9]+$ && "$vm_ns" -gt 0 ]] && echo "$bench ilo_vm $vm_ns" >> "$RESULTS"
    interp_ns=$(run_ilo_interp "$bench" "$arg" "$ilo_fn")
    [[ "$interp_ns" =~ ^[0-9]+$ && "$interp_ns" -gt 0 ]] && echo "$bench ilo_interp $interp_ns" >> "$RESULTS"
  fi

  # C# (.NET)
  [[ -f "$TMP/csharp_${bench}_out/bench" ]] && try_run "$bench" csharp "$TMP/csharp_${bench}_out/bench"

  # Kotlin (JVM)
  [[ -f "$TMP/${bench}_kt.jar" ]] && try_run "$bench" kotlin java -jar "$TMP/${bench}_kt.jar"

  # Interpreted
  check_cmd lua && [[ -f "$TMP/$bench.lua" ]] && try_run "$bench" lua lua "$TMP/$bench.lua"
  check_cmd ruby && [[ -f "$TMP/$bench.rb" ]] && try_run "$bench" ruby ruby "$TMP/$bench.rb"
  check_cmd php && [[ -f "$TMP/$bench.php" ]] && try_run "$bench" php php "$TMP/$bench.php"
  check_cmd python3 && [[ -f "$TMP/$bench.py" ]] && try_run "$bench" python python3 "$TMP/$bench.py"

  # PyPy (Python JIT)
  check_cmd pypy3 && [[ -f "$TMP/$bench.py" ]] && try_run "$bench" pypy pypy3 "$TMP/$bench.py"

  # Stop HTTP server after api benchmark
  if [[ "$bench" == "api" ]]; then
    stop_api_server
  fi
done

# ---------- Format ----------
fmt_ns() {
  local ns=$1
  if [[ ! "$ns" =~ ^[0-9]+$ ]]; then
    printf "n/a"
    return
  fi
  if [ "$ns" -ge 1000000 ] 2>/dev/null; then
    awk "BEGIN{printf \"%.1fms\", $ns/1000000}"
  elif [ "$ns" -ge 1000 ] 2>/dev/null; then
    awk "BEGIN{printf \"%.1fus\", $ns/1000}"
  else
    printf "%dns" "$ns"
  fi
}

get_result() {
  awk -v b="$1" -v l="$2" '$1==b && $2==l {print $3}' "$RESULTS"
}

LANGS=(rust go csharp kotlin luajit node tsx ilo_aot ilo_jit ilo_vm ilo_interp lua ruby php python pypy)

lang_label() {
  case $1 in
    rust)    echo "Rust (native)" ;;
    go)      echo "Go" ;;
    csharp)  echo "C# (.NET)" ;;
    kotlin)  echo "Kotlin (JVM)" ;;
    luajit)  echo "LuaJIT" ;;
    node)    echo "Node/V8" ;;
    tsx)     echo "TypeScript" ;;
    ilo_aot) echo "ilo AOT" ;;
    ilo_jit) echo "ilo JIT" ;;
    ilo_vm)  echo "ilo VM" ;;
    ilo_interp) echo "ilo Interpreter" ;;
    lua)     echo "Lua" ;;
    ruby)    echo "Ruby" ;;
    php)     echo "PHP" ;;
    python)  echo "Python 3" ;;
    pypy)    echo "PyPy 3" ;;
  esac
}

bench_desc() {
  case $1 in
    numeric) echo "sum 1..1000 (pure arithmetic loop)" ;;
    string)  echo "build 100-char string via concat" ;;
    record)  echo "create 100 structs, sum fields" ;;
    mixed)   echo "build 100 records, JSON-serialise" ;;
    guards)  echo "classify 1000 values via guard chains" ;;
    recurse) echo "fibonacci(10) — recursive calls" ;;
    foreach) echo "build list of 100, sum via foreach" ;;
    while)   echo "sum 0..99 via while loop" ;;
    pipe)    echo "chain 4 function calls × 100 (call overhead)" ;;
    file)    echo "read JSON file, parse 20 records, sum scores" ;;
    api)     echo "HTTP GET JSON, parse 20 records, sum scores" ;;
  esac
}

# One table per benchmark, sorted by speed
for bench in "${BENCHES[@]}"; do
  echo ""
  echo "── $bench: $(bench_desc "$bench") ──"
  printf "  %-16s  %10s  %s\n" "Language" "ns/call" ""
  printf "  %-16s  %10s  %s\n" "----------------" "----------" ""

  # Collect results for this benchmark, sort by ns
  sorted=$(
    for lang in "${LANGS[@]}"; do
      ns=$(get_result "$bench" "$lang")
      [[ -n "$ns" ]] && echo "$ns $lang"
    done | sort -n
  )

  # Find fastest for relative comparison
  fastest=$(echo "$sorted" | head -1 | awk '{print $1}')

  while read -r ns lang; do
    label=$(lang_label "$lang")
    formatted=$(fmt_ns "$ns")
    if [[ "$ns" -eq "$fastest" ]]; then
      rel="(fastest)"
    else
      rel=$(awk "BEGIN{printf \"%.1fx\", $ns/$fastest}")
    fi
    printf "  %-16s  %10s  %s\n" "$label" "$formatted" "$rel"
  done <<< "$sorted"
done

echo ""
echo "── $ITERS iterations | $(date '+%Y-%m-%d %H:%M') | $(uname -ms) ──"

# ---------- Generate site page ----------
SITE_PAGE="$(cd "$DIR/../.." && pwd)/../site/src/content/docs/docs/reference/benchmarks.md"
if [[ -d "$(dirname "$SITE_PAGE")" ]]; then
  echo "" >&2
  echo "Updating site benchmarks page..." >&2

  DATE=$(date '+%Y-%m-%d')
  PLATFORM=$(uname -ms)

  cat > "$SITE_PAGE" << PAGEEOF
---
title: Benchmarks
description: Runtime performance comparison of ilo against other languages
---

Micro-benchmarks comparing ilo's execution engines against compiled, JIT, and interpreted languages. Each benchmark runs **$ITERS iterations** and reports the median per-call time in nanoseconds.

:::note
These are micro-benchmarks — they measure raw execution speed, not end-to-end agent workflow performance. ilo's primary optimisation target is **total token cost** (generation + errors + retries), not runtime speed.
:::

## Execution engines

ilo has four execution backends:

| Backend | Flag | Notes |
|---------|------|-------|
| **ilo AOT** | \`ilo compile\` | Cranelift ahead-of-time compiler → standalone native binary |
| **ilo JIT** | \`ilo\` *(default)* | Cranelift-based just-in-time compiler |
| **ilo VM** | \`ilo --run-vm\` | Register-based bytecode virtual machine |
| **Interpreter** | \`ilo --run-tree\` | Tree-walking interpreter (simplest, slowest) |

## Languages tested

| Category | Languages |
|----------|-----------|
| **Compiled (AOT)** | Rust (\`rustc -O\`), Go, C# (.NET), Kotlin (JVM) |
| **JIT** | LuaJIT, Node.js (V8), TypeScript (tsx/V8), PyPy 3 |
| **ilo** | ilo JIT, ilo VM, ilo Interpreter |
| **Interpreted** | Lua, Ruby, PHP, Python 3 (CPython) |

## Results

PAGEEOF

  for bench in "${BENCHES[@]}"; do
    desc=$(bench_desc "$bench")
    cat >> "$SITE_PAGE" << BENCHEOF

### $bench

*$desc*

| Language | ns/call | vs fastest |
|----------|--------:|------------|
BENCHEOF

    sorted=$(
      for lang in "${LANGS[@]}"; do
        ns=$(get_result "$bench" "$lang")
        [[ -n "$ns" ]] && echo "$ns $lang"
      done | sort -n
    )

    fastest=$(echo "$sorted" | head -1 | awk '{print $1}')

    while read -r ns lang; do
      label=$(lang_label "$lang")
      formatted=$(fmt_ns "$ns")
      if [[ "$ns" -eq "$fastest" ]]; then
        rel="**fastest**"
      else
        rel=$(awk "BEGIN{printf \"%.1fx\", $ns/$fastest}")
      fi
      echo "| $label | $formatted | $rel |" >> "$SITE_PAGE"
    done <<< "$sorted"
  done

  cat >> "$SITE_PAGE" << FOOTEOF

## Methodology

- All benchmarks run on the same machine ($PLATFORM) in a single session
- Each benchmark warms up before timing begins
- Compiled languages use optimised builds (\`-O2\` / \`-O\`)
- V8 and LuaJIT benefit from JIT warmup during the iteration loop
- Results are from $DATE
FOOTEOF

  echo "  Written to: $SITE_PAGE" >&2

  # Auto-commit and push to site repo
  SITE_DIR="$(dirname "$SITE_PAGE")/../../../.."
  SITE_DIR="$(cd "$SITE_DIR" && pwd)"
  if [[ -d "$SITE_DIR/.git" ]]; then
    echo "  Committing and pushing site changes..." >&2
    cd "$SITE_DIR"
    git add src/content/docs/docs/reference/benchmarks.md astro.config.mjs
    if git diff --cached --quiet; then
      echo "  No changes to commit." >&2
    else
      git commit -m "docs: update benchmark results ($DATE)"
      git push
      echo "  Pushed to site repo." >&2
    fi
  fi
fi

# ---------- Update ilo README with combined matrix ----------
ILO_DIR="$(cd "$DIR/../.." && pwd)"
README="$ILO_DIR/README.md"
if [[ -f "$README" ]]; then
  echo "" >&2
  echo "Updating ilo README benchmark matrix..." >&2

  DATE=$(date '+%Y-%m-%d')
  PLATFORM=$(uname -ms)

  # Build the new benchmark section into a temp file
  BENCH_TMP="$TMP/bench_section.md"
  {
    echo "## Benchmarks"
    echo ""
    echo "Per-call time (ns) across 8 micro-benchmarks. Lower is better. [Full results →](https://ilo-lang.ai/docs/reference/benchmarks/)"
    echo ""

    # Table header
    row="| Language |"
    sep="|----------|"
    for bench in "${BENCHES[@]}"; do
      row="$row $bench |"
      sep="${sep}--------:|"
    done
    echo "$row"
    echo "$sep"

    # Rows
    for lang in rust go csharp kotlin luajit node tsx ilo_aot ilo_jit ilo_vm ilo_interp lua ruby php python pypy; do
      label=$(lang_label "$lang")
      has_any=false
      for bench in "${BENCHES[@]}"; do
        ns=$(get_result "$bench" "$lang")
        [[ -n "$ns" ]] && has_any=true && break
      done
      [[ "$has_any" == "false" ]] && continue

      row="| $label |"
      for bench in "${BENCHES[@]}"; do
        ns=$(get_result "$bench" "$lang")
        if [[ -n "$ns" ]]; then
          row="$row $(fmt_ns "$ns") |"
        else
          row="$row n/a |"
        fi
      done
      echo "$row"
    done

    echo ""
    echo "*${ITERS} iterations, ${PLATFORM}, ${DATE}*"
  } > "$BENCH_TMP"

  # Remove existing Benchmarks section if present, then insert before ## Community
  awk '
    /^## Benchmarks/ { skip=1; next }
    /^## / && skip { skip=0 }
    !skip { print }
  ' "$README" > "$README.tmp"

  # Insert bench section before ## Community
  awk -v benchfile="$BENCH_TMP" '
    /^## Community/ {
      while ((getline line < benchfile) > 0) print line
      close(benchfile)
      print ""
    }
    { print }
  ' "$README.tmp" > "$README"
  rm -f "$README.tmp"

  echo "  Updated: $README" >&2

  # Auto-commit and push to ilo repo
  cd "$ILO_DIR"
  git add README.md
  if git diff --cached --quiet; then
    echo "  No README changes to commit." >&2
  else
    git commit -m "docs: update benchmark matrix in README ($DATE)"
    git push
    echo "  Pushed to ilo repo." >&2
  fi
fi
