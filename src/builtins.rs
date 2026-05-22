/// Enum of all builtin functions in ilo.
///
/// Resolving a function name to a `Builtin` variant should happen at
/// compile time (in the bytecode compiler) or once at the start of
/// interpretation, so that hot dispatch paths use integer-discriminant
/// matching rather than string comparisons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Builtin {
    // Conversion
    Str,
    Num,

    // Math
    Abs,
    Flr,
    Cel,
    Rou,
    Min,
    Max,
    Mod,
    Fmod,
    Clamp,
    Pow,
    Sqrt,
    Log,
    Exp,
    Sin,
    Cos,
    Tan,
    Log10,
    Log2,
    Asin,
    Acos,
    Atan,
    Atan2,
    Sum,
    Prod,
    Cumsum,
    Cprod,
    Ewm,
    Avg,
    Median,
    Quantile,
    Stdev,
    Variance,
    Fft,
    Ifft,

    // Linear algebra
    Transpose,
    Matmul,
    Dot,
    // `matvec xm ys > L n` — native matrix-vector multiply. Returns the
    // dot product of each row of `xm` with `ys`. Skips the
    // wrap-as-column-matrix + flatten ceremony required to use `matmul`
    // for this case (`flatten matmul xm (map (y:n>L n;[y]) ys)`).
    // Tree-bridge eligible: composes existing matrix/vector helpers so
    // VM and Cranelift inherit through the bridge without new opcodes.
    // Added in 0.12.1.
    Matvec,

    // Collections
    Len,
    Hd,
    At,
    Tl,
    Rev,
    Srt,
    Rsrt,
    Slc,
    Lst,
    Take,
    Drop,
    Unq,
    Flat,
    Has,
    Spl,
    Cat,
    Zip,
    Enumerate,
    Range,
    Linspace,
    Ones,
    Rep,
    Window,
    Chunks,
    Setunion,
    Setinter,
    Setdiff,

    // Higher-order
    Map,
    Flt,
    Fld,
    Ct,
    Grp,
    Uniqby,
    Partition,
    Frq,
    Flatmap,
    Mapr,

    // Random / time
    Rnd,
    Rndn,
    RandBytes,
    Seed,
    Now,
    NowMs,
    Dtfmt,
    Dtparse,
    DtparseRel,
    Sleep,
    // `tz-offset tz:t epoch:n > R n t` — UTC offset in seconds for the given
    // IANA timezone at the given Unix epoch. Handles DST transitions correctly
    // via chrono-tz. Returns Err on unknown timezone name.
    TzOffset,

    // I/O
    Rd,
    Rdl,
    Rdb,
    // `rdin > R t t` — read all of stdin to a text string.
    // `rdinl > R (L t) t` — read stdin line by line, returning `R (L t) t`.
    // Both return Err on I/O failure. On WASM targets they always return
    // `Err("rdin: stdin not available on wasm")`.
    Rdin,
    Rdinl,
    // `for-line stdin > LazyStdinLines` — lazy line iterator over stdin.
    // Unlike `rdinl` (which buffers all of stdin before returning), `for-line`
    // produces a lazy handle that the `@binding` foreach consumes one line at
    // a time. This enables processing unbounded streams (e.g. `tail -f` output,
    // streaming log producers) without ever buffering the full input.
    //
    // Canonical usage:
    //   `@line (for-line stdin) { prnt line }`
    //
    // The single argument must be the text "stdin"; other values are a
    // runtime error (ILO-R009). On WASM the builtin returns Err immediately.
    // Partial trailing lines at EOF are emitted unchanged (no newline added).
    // Tree-interpreter only in this release; VM/Cranelift inherit via the
    // tree-bridge (OP_CALL_BUILTIN_TREE) because the return type
    // (`LazyStdinLines`) is opaque to the register-based engines.
    ForLine,
    Wr,
    Wra,
    Wro,
    Wrl,
    Prnt,
    Env,
    Ls,
    Walk,
    Glob,
    Fsize,
    Mtime,
    Isfile,
    Isdir,
    EnvAll,

    // String
    Trm,
    Upr,
    Lwr,
    Cap,
    Padl,
    Padr,
    Ord,
    Chr,
    Chars,
    Fmt,
    Fmt2,
    Rgx,
    Rgxall,
    Rgxall1,
    Rgxsub,
    RgxallMulti,

    // JSON
    Jpth,
    Jkeys,
    Jdmp,
    Jpar,
    JparList,
    Rdjl,

    // HTTP
    Get,
    Post,
    GetMany,
    // `get-to url timeout-ms > R t t` — like `get` but with an explicit
    // per-request timeout (milliseconds). Rounds up to nearest whole second
    // for minreq (which takes u64 seconds). Returns Err when the deadline
    // is exceeded, identical to a connection error from the caller's view.
    // Tree-bridge eligible; no new native opcode needed.
    GetTo,
    // `pst-to url body timeout-ms > R t t` — like `pst` but with an explicit
    // per-request timeout. Same millisecond-to-second rounding as `get-to`.
    PstTo,
    // `getx url > R (M t _) t` — like `get` but returns a rich Ok-map with
    // keys `status` (n), `headers` (M t t), `body` (t). Optional 2nd arg is a
    // request-headers map (M t t), same as `get`. Additive — does not change
    // `get`'s shape. Unblocks conditional-request / cache-aware / cookie-
    // following / redirect / pagination-Link workflows that need response
    // metadata. Tree-bridge eligible: no FnRef args, returns Result.
    Getx,
    // `pstx url body > R (M t _) t` — like `pst` but returns the same rich
    // Ok-map shape as `getx`. Optional 3rd arg is a request-headers map.
    // Tree-bridge eligible.
    Pstx,
    // HTTP verb cluster (#5z). Same shape as `pst`/`get`: optional 3rd-arg
    // `M t t` headers map; returns `R t t`. Tree-bridge eligible — no
    // dedicated VM opcodes, the tree interpreter performs the actual minreq
    // call. Verb cluster intentionally limited to the seven safe methods
    // (GET/POST/PUT/PATCH/DELETE/HEAD/OPTIONS); TRACE/CONNECT are excluded
    // (CONNECT is for tunnelling, TRACE is footgun-grade — both unsafe to
    // expose without proxy framing). `del` takes only the URL — DELETE bodies
    // exist in the RFC but no major API honours them, so we keep the surface
    // tight and add the headers variant only.
    Put,
    Pat,
    Del,
    Hed,
    Opt,

    // Process spawn (argv-list only — no shell, no interpolation, no glob).
    // See SPEC.md "Process spawn" section for the security framing.
    Run,
    // `run2 cmd:t args:L t > R RunResult t` — structured process spawn.
    // Like `run` but returns a typed Record instead of a loose Map, giving
    // clean dot-access: r.stdout, r.stderr, r.exit (n, not t). Non-zero
    // exit is NOT an error; Err only on spawn failure. Tree-bridge eligible.
    Run2,

    // Map (associative array)
    Mmap,
    Mget,
    Mset,
    Mhas,
    Mkeys,
    Mvals,
    Mdel,
    // `mget-or m k default > v` — value at key, or default if missing.
    // Same shape as `mget m k > O v` but unwraps with a caller-supplied
    // fallback. Lowered through the tree-bridge so VM and Cranelift inherit
    // semantics without new opcodes.
    MgetOr,
    // `lget-or xs i default > a` — element at index, or default if OOB.
    // Same negative-index semantics as `at xs i`; OOB returns default
    // rather than erroring.
    LgetOr,
    Mpairs,

    // Linear algebra
    Solve,
    Inv,
    Det,
    // `lstsq xm ys > L n` — ordinary least squares via the normal equations:
    // `b = solve (matmul (transpose xm) xm) (matmul (transpose xm) ys)`.
    // Collapses the 5-line OLS recipe into a single call. Same precision
    // tier as `solve`/`inv`/`det` (LU with partial pivoting); ill-conditioned
    // designs surface as `ILO-R009` from the inner `solve`. Tree-bridge
    // eligible — no new opcodes, VM and Cranelift inherit semantics through
    // the bridge. Added in 0.12.1.
    Lstsq,
    // Index-returning aggregates (numpy convention).
    // argmax xs:L n > n — index of max element.
    // argmin xs:L n > n — index of min element.
    // argsort xs:L n > L n — sorted index permutation (ascending).
    Argmax,
    Argmin,
    Argsort,
    // `bisect xs:L n target:n > n` — insertion point in a sorted list.
    // Python `bisect.bisect_left` semantics: returns the leftmost index `i`
    // such that `xs[0..i] < target <= xs[i..]`. Returns `0` for empty list,
    // `len(xs)` when target is greater than every element, and the index of
    // the first equal element when duplicates are present (leftmost wins on
    // ties). Caller is responsible for the sortedness precondition - we do
    // NOT validate it. O(log N) binary search; closes the verbose
    // `flt fn xs` + len pattern that three sorted-array personas reached for
    // (k-sorted-search, schedule-merge, range-bucket). Tree-bridge eligible:
    // pure 2-arg, no FnRef, no Result wrapper. NaN target propagates as NaN.
    Bisect,

    // Path manipulation (pure text ops, Unix forward-slash only).
    // POSIX dirname/basename semantics; pathjoin takes a list to avoid
    // the variadic-arity ILO-P101 trap that `fmt` lives with.
    Dirname,
    Basename,
    Pathjoin,

    // Math constants (zero-arg). Added 0.12.1 so agents stop hardcoding
    // `3.14159...` or reconstructing pi via `* 2 (atan2 0 -1)` — fft-peak
    // rerun12 surfaced both shapes. Tree-bridge-eligible so VM / Cranelift
    // inherit for free; Python codegen emits `math.pi` / `math.tau` / `math.e`.
    Pi,
    Tau,
    Eu,

    // Duration parse / format.
    // `dur-parse s > R n t` — parse a human-readable duration string ("3 weeks
    // 2 days 5 hours", "4h 32m", "1d", "1.5 hours") into seconds (f64). Lenient:
    // accepts unit abbreviations s/m/h/d/w and full names (singular + plural).
    // `dur-fmt n > t` — format seconds as human-readable "4h 32m", "2 days 1
    // hour", "30s". Drops zero parts; always uses the largest applicable unit.
    // Both are tree-bridge eligible — pure text ↔ number ops, no I/O.
    DurParse,
    DurFmt,

    // `default-on-err r d > T` — unwrap `R T E` to `T`, using `d:T` if `Err`.
    // Mirror of `??` for Result: `?? v d` is nil-coalesce for `O T`; this is
    // the Result equivalent. Tree-bridge eligible (2-arg, pure, no FnRef).
    DefaultOnErr,

    // URL + base64url encoding cluster (0.12.1). Both are token-cheap
    // primitives for the OAuth / JWT / webhook-signature workflows that
    // dominated the bearer-token, jwt-signer and webhook-receiver personas.
    // All four are tree-bridge eligible — pure text-in / text-out with no
    // I/O and no FnRef args.
    // `urlenc s > t` — RFC 3986 percent-encode. Unreserved chars (`-._~` and
    //   ALPHA/DIGIT) stay literal; everything else is `%HH`.
    // `urldec s > R t t` — inverse. Err on invalid percent escapes or
    //   non-UTF-8 decoded bytes.
    // `b64u s > t` — base64url-encode the UTF-8 bytes of `s`, no padding
    //   (RFC 4648 §5: `-`/`_` substituted for `+`/`/`).
    // `b64u-dec s > R t t` — inverse. Err on invalid base64 or non-UTF-8
    //   decoded bytes.
    Urlenc,
    Urldec,
    B64u,
    B64uDec,

    // Crypto primitives cluster (0.12.x). All tree-bridge eligible: pure
    // text-in / text-or-bool-out, no FnRef args, no I/O wrap. VM and Cranelift
    // inherit cross-engine parity without new opcodes.
    // `sha256 s > t` — SHA-256 of the UTF-8 bytes of `s`, lowercase hex.
    // `hmac-sha256 key:t msg:t > t` — HMAC-SHA256, lowercase hex.
    // `b64 s > t` — standard base64 encode (RFC 4648 §4, `=` padding).
    // `b64-dec s > R t t` — standard base64 decode; Err on invalid input
    //   or non-UTF-8 decoded bytes.
    // `hex s > t` — lowercase hex encode of UTF-8 bytes of `s`.
    // `ct-eq a:t b:t > b` — constant-time text equality. Use when comparing
    //   secrets (HMAC digests, tokens) to avoid timing leaks.
    // `sha256-hex hex:t > t` — SHA-256 of hex-decoded bytes, returns lowercase
    //   hex digest. Errors (ILO-R009) on odd-length or non-hex input.
    // `sha256d hex:t > t` — double-SHA256 (Bitcoin protocol: sha256(sha256(x)))
    //   of hex-decoded bytes, returns lowercase hex digest. Errors (ILO-R009) on
    //   odd-length or non-hex input. Equivalent to `sha256-hex (sha256-hex h)`
    //   but named for the Bitcoin Merkle tree use-case.
    Sha256,
    HmacSha256,
    B64,
    B64Dec,
    HexEnc,
    CtEq,
    Sha256Hex,
    Sha256d,

    // `where cond xs ys > L a` — parallel-list conditional select.
    // NumPy `np.where` equivalent: for each i, output[i] = xs[i] if cond[i] else ys[i].
    // All three lists must have the same length; mismatch raises ILO-R009.
    // The element type of `xs` and `ys` is preserved in the output. Tree-bridge
    // eligible (no FnRef args, no I/O, no Result wrapper). Saves ~50 tokens
    // over the `map (i:n>_;?h (at cond i) (at xs i) (at ys i)) (range 0 (len xs))`
    // recipe; the manifesto framing for NumPy-familiar agents.
    Where,
    // Calendar arithmetic (0.12.2). All four are tree-bridge eligible: pure
    // epoch ↔ epoch / epoch ↔ number, no FnRef args, no I/O.
    //
    // `add-mo dt:n n:n > n` — add N calendar months to an epoch, snapping
    // end-of-month (Jan 31 + 1 = Feb 28/29). N may be negative.
    // `last-dom dt:n > n`  — epoch of the last day of the month containing dt,
    // at 00:00 UTC.
    // `next-business-day dt:n > n` — next weekday after dt (skip Sat/Sun).
    // `day-of-week dt:n > n` — day of week: 0=Sun, 1=Mon … 6=Sat.
    AddMo,
    LastDom,
    NextBusinessDay,
    DayOfWeek,
    // Rolling-window reducers (0.12.x). Numeric-list inputs, fixed window
    // size `n`. Output length = `len xs - n + 1` (empty when `n > len`).
    //
    // `rsum n:n xs:L n > L n` — rolling sum via running-window: O(n) total,
    // not O(n*w) like the naive `map (i:n>n;sum (slc xs i (+ i n))) ...`
    // recipe.
    // `ravg n:n xs:L n > L n` — rolling mean, same running-window strategy.
    // `rmin n:n xs:L n > L n` — rolling minimum, O(n) amortised via a
    // monotonic-deque idiom.
    //
    // `n=0` errors `ILO-R009`; `n > len xs` returns `[]` (Python/numpy
    // convention). Tree-bridge eligible — pure number-list reducers, no
    // FnRef args, no Result wrapper. Appended last to preserve every
    // existing on-wire tag.
    Rsum,
    Ravg,
    Rmin,

    // Text search (0.13.0). Tree-bridge eligible: pure text-in / Option-out,
    // no FnRef args, no I/O, no Result wrapper.
    // `idxof s sub > O n` — byte index of the first occurrence of `sub` in
    // `s`. Returns nil when `sub` is not found. Index is in Unicode code-point
    // units (same as `at`), not raw bytes, so multi-byte characters count as 1.
    Idxof,
}

impl Builtin {
    /// Resolve a canonical builtin name to its enum variant.
    /// Returns `None` for user-defined functions.
    pub fn from_name(s: &str) -> Option<Builtin> {
        match s {
            "str" => Some(Builtin::Str),
            "num" => Some(Builtin::Num),
            "abs" => Some(Builtin::Abs),
            "flr" => Some(Builtin::Flr),
            "cel" => Some(Builtin::Cel),
            "rou" => Some(Builtin::Rou),
            "min" => Some(Builtin::Min),
            "max" => Some(Builtin::Max),
            "mod" => Some(Builtin::Mod),
            "fmod" => Some(Builtin::Fmod),
            "clamp" => Some(Builtin::Clamp),
            "pow" => Some(Builtin::Pow),
            "sqrt" => Some(Builtin::Sqrt),
            "log" => Some(Builtin::Log),
            "exp" => Some(Builtin::Exp),
            "sin" => Some(Builtin::Sin),
            "cos" => Some(Builtin::Cos),
            "tan" => Some(Builtin::Tan),
            "log10" => Some(Builtin::Log10),
            "log2" => Some(Builtin::Log2),
            "asin" => Some(Builtin::Asin),
            "acos" => Some(Builtin::Acos),
            "atan" => Some(Builtin::Atan),
            "atan2" => Some(Builtin::Atan2),
            "sum" => Some(Builtin::Sum),
            "prod" => Some(Builtin::Prod),
            "cumsum" => Some(Builtin::Cumsum),
            "cprod" => Some(Builtin::Cprod),
            "ewm" => Some(Builtin::Ewm),
            "avg" => Some(Builtin::Avg),
            "median" => Some(Builtin::Median),
            "quantile" => Some(Builtin::Quantile),
            "stdev" => Some(Builtin::Stdev),
            "variance" => Some(Builtin::Variance),
            "fft" => Some(Builtin::Fft),
            "ifft" => Some(Builtin::Ifft),
            "transpose" => Some(Builtin::Transpose),
            "matmul" => Some(Builtin::Matmul),
            "matvec" => Some(Builtin::Matvec),
            "dot" => Some(Builtin::Dot),
            "len" => Some(Builtin::Len),
            "hd" => Some(Builtin::Hd),
            "at" => Some(Builtin::At),
            "tl" => Some(Builtin::Tl),
            "rev" => Some(Builtin::Rev),
            "srt" => Some(Builtin::Srt),
            "rsrt" => Some(Builtin::Rsrt),
            "slc" => Some(Builtin::Slc),
            "lst" => Some(Builtin::Lst),
            "take" => Some(Builtin::Take),
            "drop" => Some(Builtin::Drop),
            "unq" => Some(Builtin::Unq),
            "flat" => Some(Builtin::Flat),
            "has" => Some(Builtin::Has),
            "spl" => Some(Builtin::Spl),
            "cat" => Some(Builtin::Cat),
            "zip" => Some(Builtin::Zip),
            "enumerate" => Some(Builtin::Enumerate),
            "range" => Some(Builtin::Range),
            "linspace" => Some(Builtin::Linspace),
            "ones" => Some(Builtin::Ones),
            "rep" => Some(Builtin::Rep),
            "window" => Some(Builtin::Window),
            "chunks" => Some(Builtin::Chunks),
            "setunion" => Some(Builtin::Setunion),
            "setinter" => Some(Builtin::Setinter),
            "setdiff" => Some(Builtin::Setdiff),
            "map" => Some(Builtin::Map),
            "flt" => Some(Builtin::Flt),
            "fld" => Some(Builtin::Fld),
            "ct" => Some(Builtin::Ct),
            "grp" => Some(Builtin::Grp),
            "uniqby" => Some(Builtin::Uniqby),
            "partition" => Some(Builtin::Partition),
            "frq" => Some(Builtin::Frq),
            "flatmap" => Some(Builtin::Flatmap),
            "mapr" => Some(Builtin::Mapr),
            "rnd" => Some(Builtin::Rnd),
            "rndn" => Some(Builtin::Rndn),
            "rand-bytes" => Some(Builtin::RandBytes),
            "seed" => Some(Builtin::Seed),
            "now" => Some(Builtin::Now),
            "now-ms" => Some(Builtin::NowMs),
            "dtfmt" => Some(Builtin::Dtfmt),
            "dtparse" => Some(Builtin::Dtparse),
            "dtparse-rel" => Some(Builtin::DtparseRel),
            "sleep" => Some(Builtin::Sleep),
            "tz-offset" => Some(Builtin::TzOffset),
            "rd" => Some(Builtin::Rd),
            "rdl" => Some(Builtin::Rdl),
            "rdb" => Some(Builtin::Rdb),
            "rdin" => Some(Builtin::Rdin),
            "rdinl" => Some(Builtin::Rdinl),
            "for-line" => Some(Builtin::ForLine),
            "wr" => Some(Builtin::Wr),
            "wra" => Some(Builtin::Wra),
            "wro" => Some(Builtin::Wro),
            "wrl" => Some(Builtin::Wrl),
            "prnt" => Some(Builtin::Prnt),
            "env" => Some(Builtin::Env),
            "lsd" => Some(Builtin::Ls),
            "walk" => Some(Builtin::Walk),
            "glob" => Some(Builtin::Glob),
            "fsize" => Some(Builtin::Fsize),
            "mtime" => Some(Builtin::Mtime),
            "isfile" => Some(Builtin::Isfile),
            "isdir" => Some(Builtin::Isdir),
            "env-all" => Some(Builtin::EnvAll),
            "trm" => Some(Builtin::Trm),
            "upr" => Some(Builtin::Upr),
            "lwr" => Some(Builtin::Lwr),
            "cap" => Some(Builtin::Cap),
            "padl" => Some(Builtin::Padl),
            "padr" => Some(Builtin::Padr),
            "ord" => Some(Builtin::Ord),
            "chr" => Some(Builtin::Chr),
            "chars" => Some(Builtin::Chars),
            "fmt" => Some(Builtin::Fmt),
            "fmt2" => Some(Builtin::Fmt2),
            "rgx" => Some(Builtin::Rgx),
            "rgxall" => Some(Builtin::Rgxall),
            "rgxall1" => Some(Builtin::Rgxall1),
            "rgxsub" => Some(Builtin::Rgxsub),
            "rgxall-multi" => Some(Builtin::RgxallMulti),
            "jpth" => Some(Builtin::Jpth),
            "jkeys" => Some(Builtin::Jkeys),
            "jdmp" => Some(Builtin::Jdmp),
            "jpar" => Some(Builtin::Jpar),
            "jpar-list" => Some(Builtin::JparList),
            "rdjl" => Some(Builtin::Rdjl),
            "run" => Some(Builtin::Run),
            "run2" => Some(Builtin::Run2),
            "get" => Some(Builtin::Get),
            // 0.12.0 rename: `post` → `pst`. Brings post into line with the
            // I/O compression family (rd, wr, srt, flt, fld, fmt). Clean
            // break — `post` no longer resolves; the verifier surfaces a
            // did-you-mean to `pst` via the standard suggestion path.
            "pst" => Some(Builtin::Post),
            "get-many" => Some(Builtin::GetMany),
            "get-to" => Some(Builtin::GetTo),
            "pst-to" => Some(Builtin::PstTo),
            "getx" => Some(Builtin::Getx),
            "pstx" => Some(Builtin::Pstx),
            "put" => Some(Builtin::Put),
            "pat" => Some(Builtin::Pat),
            "del" => Some(Builtin::Del),
            "hed" => Some(Builtin::Hed),
            "opt" => Some(Builtin::Opt),
            "mmap" => Some(Builtin::Mmap),
            "mget" => Some(Builtin::Mget),
            "mset" => Some(Builtin::Mset),
            "mhas" => Some(Builtin::Mhas),
            "mkeys" => Some(Builtin::Mkeys),
            "mvals" => Some(Builtin::Mvals),
            "mdel" => Some(Builtin::Mdel),
            "mget-or" => Some(Builtin::MgetOr),
            "lget-or" => Some(Builtin::LgetOr),
            "mpairs" => Some(Builtin::Mpairs),
            "solve" => Some(Builtin::Solve),
            "inv" => Some(Builtin::Inv),
            "det" => Some(Builtin::Det),
            "lstsq" => Some(Builtin::Lstsq),
            "argmax" => Some(Builtin::Argmax),
            "argmin" => Some(Builtin::Argmin),
            "argsort" => Some(Builtin::Argsort),
            "bisect" => Some(Builtin::Bisect),
            "dirname" => Some(Builtin::Dirname),
            "basename" => Some(Builtin::Basename),
            "pathjoin" => Some(Builtin::Pathjoin),
            "pi" => Some(Builtin::Pi),
            "tau" => Some(Builtin::Tau),
            "e" => Some(Builtin::Eu),
            "dur-parse" => Some(Builtin::DurParse),
            "dur-fmt" => Some(Builtin::DurFmt),
            "default-on-err" => Some(Builtin::DefaultOnErr),
            "urlenc" => Some(Builtin::Urlenc),
            "urldec" => Some(Builtin::Urldec),
            "b64u" => Some(Builtin::B64u),
            "b64u-dec" => Some(Builtin::B64uDec),
            "sha256" => Some(Builtin::Sha256),
            "hmac-sha256" => Some(Builtin::HmacSha256),
            "b64" => Some(Builtin::B64),
            "b64-dec" => Some(Builtin::B64Dec),
            "hex" => Some(Builtin::HexEnc),
            "ct-eq" => Some(Builtin::CtEq),
            "sha256-hex" => Some(Builtin::Sha256Hex),
            "sha256d" => Some(Builtin::Sha256d),
            "where" => Some(Builtin::Where),
            "add-mo" => Some(Builtin::AddMo),
            "last-dom" => Some(Builtin::LastDom),
            "next-business-day" => Some(Builtin::NextBusinessDay),
            "day-of-week" => Some(Builtin::DayOfWeek),
            "rsum" => Some(Builtin::Rsum),
            "ravg" => Some(Builtin::Ravg),
            "rmin" => Some(Builtin::Rmin),
            "idxof" => Some(Builtin::Idxof),
            _ => None,
        }
    }

    /// Return the canonical short name for this builtin.
    #[allow(dead_code)]
    pub fn name(self) -> &'static str {
        match self {
            Builtin::Str => "str",
            Builtin::Num => "num",
            Builtin::Abs => "abs",
            Builtin::Flr => "flr",
            Builtin::Cel => "cel",
            Builtin::Rou => "rou",
            Builtin::Min => "min",
            Builtin::Max => "max",
            Builtin::Mod => "mod",
            Builtin::Fmod => "fmod",
            Builtin::Clamp => "clamp",
            Builtin::Pow => "pow",
            Builtin::Sqrt => "sqrt",
            Builtin::Log => "log",
            Builtin::Exp => "exp",
            Builtin::Sin => "sin",
            Builtin::Cos => "cos",
            Builtin::Tan => "tan",
            Builtin::Log10 => "log10",
            Builtin::Log2 => "log2",
            Builtin::Asin => "asin",
            Builtin::Acos => "acos",
            Builtin::Atan => "atan",
            Builtin::Atan2 => "atan2",
            Builtin::Sum => "sum",
            Builtin::Prod => "prod",
            Builtin::Cumsum => "cumsum",
            Builtin::Cprod => "cprod",
            Builtin::Ewm => "ewm",
            Builtin::Avg => "avg",
            Builtin::Median => "median",
            Builtin::Quantile => "quantile",
            Builtin::Stdev => "stdev",
            Builtin::Variance => "variance",
            Builtin::Fft => "fft",
            Builtin::Ifft => "ifft",
            Builtin::Transpose => "transpose",
            Builtin::Matmul => "matmul",
            Builtin::Matvec => "matvec",
            Builtin::Dot => "dot",
            Builtin::Len => "len",
            Builtin::Hd => "hd",
            Builtin::At => "at",
            Builtin::Tl => "tl",
            Builtin::Rev => "rev",
            Builtin::Srt => "srt",
            Builtin::Rsrt => "rsrt",
            Builtin::Slc => "slc",
            Builtin::Lst => "lst",
            Builtin::Take => "take",
            Builtin::Drop => "drop",
            Builtin::Unq => "unq",
            Builtin::Flat => "flat",
            Builtin::Has => "has",
            Builtin::Spl => "spl",
            Builtin::Cat => "cat",
            Builtin::Zip => "zip",
            Builtin::Enumerate => "enumerate",
            Builtin::Range => "range",
            Builtin::Linspace => "linspace",
            Builtin::Ones => "ones",
            Builtin::Rep => "rep",
            Builtin::Window => "window",
            Builtin::Chunks => "chunks",
            Builtin::Setunion => "setunion",
            Builtin::Setinter => "setinter",
            Builtin::Setdiff => "setdiff",
            Builtin::Map => "map",
            Builtin::Flt => "flt",
            Builtin::Fld => "fld",
            Builtin::Ct => "ct",
            Builtin::Grp => "grp",
            Builtin::Uniqby => "uniqby",
            Builtin::Partition => "partition",
            Builtin::Frq => "frq",
            Builtin::Flatmap => "flatmap",
            Builtin::Mapr => "mapr",
            Builtin::Rnd => "rnd",
            Builtin::Rndn => "rndn",
            Builtin::RandBytes => "rand-bytes",
            Builtin::Seed => "seed",
            Builtin::Now => "now",
            Builtin::NowMs => "now-ms",
            Builtin::Dtfmt => "dtfmt",
            Builtin::Dtparse => "dtparse",
            Builtin::DtparseRel => "dtparse-rel",
            Builtin::Sleep => "sleep",
            Builtin::TzOffset => "tz-offset",
            Builtin::Rd => "rd",
            Builtin::Rdl => "rdl",
            Builtin::Rdb => "rdb",
            Builtin::Rdin => "rdin",
            Builtin::Rdinl => "rdinl",
            Builtin::ForLine => "for-line",
            Builtin::Wr => "wr",
            Builtin::Wra => "wra",
            Builtin::Wro => "wro",
            Builtin::Wrl => "wrl",
            Builtin::Prnt => "prnt",
            Builtin::Env => "env",
            Builtin::Ls => "lsd",
            Builtin::Walk => "walk",
            Builtin::Glob => "glob",
            Builtin::Fsize => "fsize",
            Builtin::Mtime => "mtime",
            Builtin::Isfile => "isfile",
            Builtin::Isdir => "isdir",
            Builtin::EnvAll => "env-all",
            Builtin::Trm => "trm",
            Builtin::Upr => "upr",
            Builtin::Lwr => "lwr",
            Builtin::Cap => "cap",
            Builtin::Padl => "padl",
            Builtin::Padr => "padr",
            Builtin::Ord => "ord",
            Builtin::Chr => "chr",
            Builtin::Chars => "chars",
            Builtin::Fmt => "fmt",
            Builtin::Fmt2 => "fmt2",
            Builtin::Rgx => "rgx",
            Builtin::Rgxall => "rgxall",
            Builtin::Rgxall1 => "rgxall1",
            Builtin::Rgxsub => "rgxsub",
            Builtin::RgxallMulti => "rgxall-multi",
            Builtin::Jpth => "jpth",
            Builtin::Jkeys => "jkeys",
            Builtin::Jdmp => "jdmp",
            Builtin::Jpar => "jpar",
            Builtin::JparList => "jpar-list",
            Builtin::Rdjl => "rdjl",
            Builtin::Run => "run",
            Builtin::Run2 => "run2",
            Builtin::Get => "get",
            Builtin::Post => "pst",
            Builtin::GetMany => "get-many",
            Builtin::GetTo => "get-to",
            Builtin::PstTo => "pst-to",
            Builtin::Getx => "getx",
            Builtin::Pstx => "pstx",
            Builtin::Put => "put",
            Builtin::Pat => "pat",
            Builtin::Del => "del",
            Builtin::Hed => "hed",
            Builtin::Opt => "opt",
            Builtin::Mmap => "mmap",
            Builtin::Mget => "mget",
            Builtin::Mset => "mset",
            Builtin::Mhas => "mhas",
            Builtin::Mkeys => "mkeys",
            Builtin::Mvals => "mvals",
            Builtin::Mdel => "mdel",
            Builtin::MgetOr => "mget-or",
            Builtin::LgetOr => "lget-or",
            Builtin::Mpairs => "mpairs",
            Builtin::Solve => "solve",
            Builtin::Inv => "inv",
            Builtin::Det => "det",
            Builtin::Lstsq => "lstsq",
            Builtin::Argmax => "argmax",
            Builtin::Argmin => "argmin",
            Builtin::Argsort => "argsort",
            Builtin::Bisect => "bisect",
            Builtin::Dirname => "dirname",
            Builtin::Basename => "basename",
            Builtin::Pathjoin => "pathjoin",
            Builtin::Pi => "pi",
            Builtin::Tau => "tau",
            Builtin::Eu => "e",
            Builtin::DurParse => "dur-parse",
            Builtin::DurFmt => "dur-fmt",
            Builtin::DefaultOnErr => "default-on-err",
            Builtin::Urlenc => "urlenc",
            Builtin::Urldec => "urldec",
            Builtin::B64u => "b64u",
            Builtin::B64uDec => "b64u-dec",
            Builtin::Sha256 => "sha256",
            Builtin::HmacSha256 => "hmac-sha256",
            Builtin::B64 => "b64",
            Builtin::B64Dec => "b64-dec",
            Builtin::HexEnc => "hex",
            Builtin::CtEq => "ct-eq",
            Builtin::Sha256Hex => "sha256-hex",
            Builtin::Sha256d => "sha256d",
            Builtin::Where => "where",
            Builtin::AddMo => "add-mo",
            Builtin::LastDom => "last-dom",
            Builtin::NextBusinessDay => "next-business-day",
            Builtin::DayOfWeek => "day-of-week",
            Builtin::Rsum => "rsum",
            Builtin::Ravg => "ravg",
            Builtin::Rmin => "rmin",
            Builtin::Idxof => "idxof",
        }
    }

    /// Check if a name refers to a builtin function.
    pub fn is_builtin(name: &str) -> bool {
        Self::from_name(name).is_some()
    }

    /// Stable list of every `Builtin` variant, in canonical order.
    ///
    /// The position of each variant in this slice is its on-wire tag
    /// for `OP_CALL_BUILTIN_TREE`. Stability matters: appending is fine,
    /// reordering or removing entries breaks any persisted bytecode.
    pub const ALL: &'static [Builtin] = &[
        Builtin::Str,
        Builtin::Num,
        Builtin::Abs,
        Builtin::Flr,
        Builtin::Cel,
        Builtin::Rou,
        Builtin::Min,
        Builtin::Max,
        Builtin::Mod,
        Builtin::Fmod,
        Builtin::Clamp,
        Builtin::Pow,
        Builtin::Sqrt,
        Builtin::Log,
        Builtin::Exp,
        Builtin::Sin,
        Builtin::Cos,
        Builtin::Tan,
        Builtin::Log10,
        Builtin::Log2,
        Builtin::Asin,
        Builtin::Acos,
        Builtin::Atan,
        Builtin::Atan2,
        Builtin::Sum,
        Builtin::Prod,
        Builtin::Cumsum,
        Builtin::Cprod,
        Builtin::Avg,
        Builtin::Median,
        Builtin::Quantile,
        Builtin::Stdev,
        Builtin::Variance,
        Builtin::Fft,
        Builtin::Ifft,
        Builtin::Transpose,
        Builtin::Matmul,
        Builtin::Dot,
        Builtin::Len,
        Builtin::Hd,
        Builtin::At,
        Builtin::Tl,
        Builtin::Rev,
        Builtin::Srt,
        Builtin::Rsrt,
        Builtin::Slc,
        Builtin::Lst,
        Builtin::Take,
        Builtin::Drop,
        Builtin::Unq,
        Builtin::Flat,
        Builtin::Has,
        Builtin::Spl,
        Builtin::Cat,
        Builtin::Zip,
        Builtin::Enumerate,
        Builtin::Range,
        Builtin::Window,
        Builtin::Chunks,
        Builtin::Setunion,
        Builtin::Setinter,
        Builtin::Setdiff,
        Builtin::Map,
        Builtin::Flt,
        Builtin::Fld,
        Builtin::Grp,
        Builtin::Uniqby,
        Builtin::Partition,
        Builtin::Frq,
        Builtin::Flatmap,
        Builtin::Mapr,
        Builtin::Rnd,
        Builtin::Rndn,
        Builtin::Seed,
        Builtin::Now,
        Builtin::Dtfmt,
        Builtin::Dtparse,
        Builtin::DtparseRel,
        Builtin::Rd,
        Builtin::Rdl,
        Builtin::Rdb,
        Builtin::Wr,
        Builtin::Wra,
        Builtin::Wro,
        Builtin::Wrl,
        Builtin::Prnt,
        Builtin::Env,
        Builtin::Trm,
        Builtin::Upr,
        Builtin::Lwr,
        Builtin::Cap,
        Builtin::Padl,
        Builtin::Padr,
        Builtin::Ord,
        Builtin::Chr,
        Builtin::Chars,
        Builtin::Fmt,
        Builtin::Fmt2,
        Builtin::Rgx,
        Builtin::Rgxall,
        Builtin::Rgxsub,
        Builtin::Jpth,
        Builtin::Jkeys,
        Builtin::Jdmp,
        Builtin::Jpar,
        Builtin::JparList,
        Builtin::Rdjl,
        Builtin::Get,
        Builtin::Post,
        Builtin::GetMany,
        Builtin::Mmap,
        Builtin::Mget,
        Builtin::Mset,
        Builtin::Mhas,
        Builtin::Mkeys,
        Builtin::Mvals,
        Builtin::Mdel,
        Builtin::Solve,
        Builtin::Inv,
        Builtin::Det,
        Builtin::Sleep,
        // Appended after Sleep to preserve every existing tag. Rgxall1 is a
        // convenience over Rgxall (flat first-capture-group list); see the
        // tree-bridge entry in src/vm/mod.rs for cross-engine dispatch.
        Builtin::Rgxall1,
        // Ct fn xs -> n: count-by-predicate. Tree-bridge eligible alongside
        // its HOF peers (Flt, Map, Grp); see is_tree_bridge_eligible.
        // Named `ct` (not `cnt`) because `cnt` is reserved as the loop
        // continue keyword — see src/parser/mod.rs:3507.
        Builtin::Ct,
        // Appended last to preserve existing on-wire tags. now-ms returns
        // the current Unix epoch in milliseconds as f64 — paired with `now`
        // (seconds) so per-phase timing has no rounding loss in agent
        // perf-bisection workloads.
        Builtin::NowMs,
        // Filesystem enumeration. Closes the categorical gap vs Python rglob
        // and shell `find`: single-file `rd` exists but no directory listing
        // until now. All three return Result so missing-dir / permission-denied
        // are typed at the boundary. Tree-bridge eligible; no native opcodes.
        Builtin::Ls,
        Builtin::Walk,
        Builtin::Glob,
        // env-all -> R M t t: full process environment as Map[Text, Text]
        // wrapped in Result. Tree-bridge eligible (zero args, no FnRef);
        // see is_tree_bridge_eligible in src/vm/mod.rs.
        Builtin::EnvAll,
        // `run cmd:t args:L t > R (M t) t` — argv-list process spawn.
        // No shell, no interpolation, no glob — the principled defence
        // against shell injection in agent orchestration. See SPEC.md
        // "Process spawn" + the tree-bridge entry in src/vm/mod.rs.
        Builtin::Run,
        // 0.12.1: defaulted lookups for Map and List. Both lower through the
        // tree-bridge (OP_CALL_BUILTIN_TREE), so no new opcodes; appending
        // here keeps every existing on-wire tag stable.
        Builtin::MgetOr,
        Builtin::LgetOr,
        // Index-returning aggregates (numpy convention). Pure list ops:
        // tree-bridge eligible, no FnRef args, no I/O. Closes the
        // `srt fn (enumerate xs)` + extract-first pattern that three
        // rerun12 personas converged on. Appended to preserve tags.
        Builtin::Argmax,
        Builtin::Argmin,
        Builtin::Argsort,
        // Path manipulation builtins. Pure text ops, POSIX semantics,
        // forward-slash only (Windows paths are a 0.13.0 concern).
        // Tree-bridge eligible like the lsd/walk/glob trio, but with no
        // Result wrapper — see is_tree_bridge_eligible in src/vm/mod.rs.
        Builtin::Dirname,
        Builtin::Basename,
        Builtin::Pathjoin,
        // 0.12.1: stdin read primitives. Unblocks the Unix-pipeline persona
        // class — `rd` is file-only; child stdin was Stdio::null() previously.
        // `rdin > R t t` reads all of stdin; `rdinl > R (L t) t` is line-mode.
        // Both are 0-arg and tree-bridge eligible. Appended to preserve tags.
        Builtin::Rdin,
        Builtin::Rdinl,
        // `mpairs m > L (L _)` — sorted-by-key list of [k, v] 2-element lists.
        // Invariant: `mpairs m == zip (mkeys m) (mvals m)`. Kills the common
        // `map (fn k > [k (mget m k)]) (mkeys m)` cascade — one builtin call
        // instead of lambda + mkeys + mget per iteration. Added in 0.12.1.
        Builtin::Mpairs,
        // `rgxall-multi pats:L t line:t > L t` — multi-pattern flat-match.
        // Equivalent to `flat (map (p:t>L t;rgxall1 p line) pats)` but saves
        // ~20 tokens per call site. cron-explainer and historical-archeologist
        // both wanted this: apply several patterns to one line and get a single
        // flat list of all hits in pattern order. Tree-bridge eligible alongside
        // rgxall1 — same dispatch path, no new opcodes.
        Builtin::RgxallMulti,
        // Math constants (0.12.1). Appended last to preserve on-wire tag
        // stability for every prior builtin. Zero-arg, tree-bridge-eligible.
        Builtin::Pi,
        Builtin::Tau,
        Builtin::Eu,
        // Duration parse / format. Tree-bridge eligible: pure text↔number, no
        // I/O, no FnRef args. DurParse returns R n t so that malformed input
        // surfaces as a typed error at the boundary. DurFmt is total (always
        // returns a text string). Appended here to preserve every existing tag.
        Builtin::DurParse,
        Builtin::DurFmt,
        // `default-on-err r d > T` — Result mirror of `??`. Unwraps `R T E`
        // to `T`, returning `d` on `Err`. Kills the common `?r{~v:v ^_:default}`
        // pattern. Tree-bridge eligible (2-arg, pure). Added in 0.12.1.
        Builtin::DefaultOnErr,
        // Calendar arithmetic (0.12.2). Pure epoch↔epoch/n ops, no FnRef, no I/O.
        // Tree-bridge eligible: VM + Cranelift inherit for free without new opcodes.
        // Appended to preserve all existing on-wire tags.
        Builtin::AddMo,
        Builtin::LastDom,
        Builtin::NextBusinessDay,
        Builtin::DayOfWeek,
        // 0.12.1 filesystem metadata primitives. Atomic singletons rather
        // than a fat `stat path > R (M t t) t`: agents that want size pay
        // size cost, agents that want a predicate pay predicate cost. Size
        // / mtime return Result (open-and-stat can fail); predicates return
        // bool (Python convention - `false` collapses missing / perm-denied
        // / wrong-kind into the natural branch). All four are tree-bridge
        // eligible; no native opcodes.
        Builtin::Fsize,
        Builtin::Mtime,
        Builtin::Isfile,
        Builtin::Isdir,
        // 0.12.1: HTTP builtins with explicit per-request timeout. Appended
        // last to preserve every existing on-wire tag. Tree-bridge eligible
        // so VM and Cranelift JIT/AOT inherit them without new opcodes.
        // minreq `with_timeout` takes whole seconds; millisecond values are
        // rounded up (ceil(ms / 1000)) so 1 ms => 1 s, 1001 ms => 2 s.
        Builtin::GetTo,
        Builtin::PstTo,
        // `matvec xm ys > L n` — native matrix-vector multiply. Tree-bridge
        // eligible: composes the same row/vector helpers as `matmul`, so VM
        // and Cranelift inherit through the bridge without new opcodes.
        // Appended to preserve every existing tag.
        Builtin::Matvec,
        // `lstsq xm ys > L n` — ordinary least squares via the normal
        // equations. Tree-bridge eligible: composes existing transpose /
        // matmul / solve so VM and Cranelift inherit through the bridge
        // without new opcodes. Appended to preserve every existing tag.
        Builtin::Lstsq,
        // `rand-bytes n > t` — cryptographically random bytes, base64url-no-pad encoded.
        // Distinct from `rnd` (uniform float) and `rndn` (Normal float): this is the
        // CSPRNG path agents need for jti / CSRF tokens / session IDs / nonces. Output
        // is base64url-no-pad so it drops straight into headers, cookies, query strings
        // without further encoding. Tree-bridge eligible (arity 1, no FnRef, no I/O
        // wrap). Appended last to preserve on-wire tags. Backed by `getrandom`, not
        // `fastrand` — cryptographic randomness must never be seeded.
        Builtin::RandBytes,
        // 0.12.1: URL + base64url encoding cluster. Appended last to preserve
        // every existing on-wire tag. Tree-bridge eligible — pure text-in /
        // text-out, no FnRef args, no I/O. Backed by the `percent-encoding`
        // and `base64` crates. The two decoders return Result so malformed
        // input surfaces as a typed error at the boundary; the two encoders
        // are total (always produce Text).
        Builtin::Urlenc,
        Builtin::Urldec,
        Builtin::B64u,
        Builtin::B64uDec,
        // ewm xs a > L n — exponential moving average with smoothing factor a
        // in [0, 1]. Pure number-list reducer; tree-bridge eligible alongside
        // the cumsum/cprod aggregate family. Appended last to preserve every
        // existing on-wire tag.
        Builtin::Ewm,
        // where cond xs ys > L a — parallel-list conditional select (NumPy
        // np.where equivalent). Tree-bridge eligible (3-arg, no FnRef, no I/O,
        // no Result wrapper). Appended last to preserve every existing on-wire
        // tag.
        Builtin::Where,
        // `tz-offset tz:t epoch:n > R n t` — UTC offset in seconds for the
        // given IANA timezone at the given Unix epoch. DST-aware via chrono-tz.
        // Returns Err on unknown timezone name. Tree-bridge eligible (2-arg,
        // no FnRef). Appended to preserve all prior on-wire tags.
        Builtin::TzOffset,
        // `run2 cmd:t args:L t > R RunResult t` - structured process spawn.
        // Returns a typed Record{stdout:t; stderr:t; exit:n} rather than the
        // loose M t t that `run` returns, giving clean dot-access. Appended
        // last to preserve every existing on-wire tag; tree-bridge eligible.
        Builtin::Run2,
        // Numeric prelude (0.12.1). Three list constructors hit repeatedly by
        // linear-regression (linspace for evenly-spaced sample points),
        // distance-matrix (ones for a design-matrix column), and monte-carlo
        // (rep for seeding accumulators). Tree-bridge eligible — pure, no
        // FnRef args, no I/O, no Result wrapper. Appended last to preserve
        // every existing on-wire tag.
        Builtin::Linspace,
        Builtin::Ones,
        Builtin::Rep,
        // HTTP verb cluster (#5z). Tree-bridge eligible — same shape as `pst`
        // (optional headers, R t t return). Appended last to preserve every
        // existing on-wire tag.
        Builtin::Put,
        Builtin::Pat,
        Builtin::Del,
        Builtin::Hed,
        Builtin::Opt,
        // Crypto primitives cluster (0.12.x). All tree-bridge eligible — pure
        // text-in / text-or-bool-out, no FnRef args, no I/O. VM and Cranelift
        // JIT inherit through the existing bridge at zero opcode cost. Order
        // here is the on-wire dispatch order; appended last to preserve every
        // existing tag.
        //
        // sha256 / hmac-sha256: lowercase hex digest output. Backed by `sha2`
        // and `hmac` crates from the RustCrypto suite.
        // b64 / b64-dec: standard base64 with `=` padding (RFC 4648 §4),
        // distinct from b64u / b64u-dec which use the URL-safe alphabet.
        // hex: lowercase hex encode of the UTF-8 bytes of the input text.
        // ct-eq: constant-time text equality. Use when comparing secrets
        // (HMAC digests, tokens) so a short-circuit `=` doesn't leak timing.
        Builtin::Sha256,
        Builtin::HmacSha256,
        Builtin::B64,
        Builtin::B64Dec,
        Builtin::HexEnc,
        Builtin::CtEq,
        // getx / pstx — HTTP variants that surface response status, headers,
        // and body as a Map[Text, _] wrapped in Ok. Additive — the existing
        // `get` / `pst` body-only signatures stay intact for token-cheap GETs
        // that don't care about metadata. Tree-bridge eligible (returns
        // Result, no FnRef args), so VM and Cranelift inherit without new
        // opcodes. Appended last to preserve every prior on-wire tag.
        Builtin::Getx,
        Builtin::Pstx,
        // Rolling-window reducers (#5bq). Pure number-list reducers with a
        // fixed window size; output length = `len xs - n + 1`. O(n)
        // amortised via running-sum (rsum/ravg) and monotonic-deque (rmin),
        // not O(n*w) like the naive `slc + sum` recipe. Tree-bridge eligible:
        // VM and Cranelift inherit through OP_CALL_BUILTIN_TREE at zero
        // opcode cost. Appended last to preserve every existing on-wire tag.
        Builtin::Rsum,
        Builtin::Ravg,
        Builtin::Rmin,
        // `bisect xs target > n` — O(log N) insertion point in a sorted
        // numeric list (Python `bisect_left` semantics). Tree-bridge
        // eligible: pure 2-arg, no FnRef, no Result wrapper. Appended last
        // to preserve every existing on-wire tag.
        Builtin::Bisect,
        // `for-line stdin > LazyStdinLines` — lazy stdin line iterator (ILO-70).
        // Appended last to preserve every existing on-wire tag.
        // Returns a LazyStdinLines handle that ForEach drains one line at a time,
        // enabling processing of unbounded piped input without buffering.
        Builtin::ForLine,
    ];

    /// Stability tier for this builtin, sourced from `STABILITY.md`.
    ///
    /// - `"experimental"` — unreleased (above `0.12.1` in `CHANGELOG.md`).
    ///   May be removed or changed without notice.
    /// - `"provisional"` — shipped in a released version (0.12.1 or earlier).
    ///   Signature may change pre-1.0; canonical short name is stable-ish.
    ///
    /// Used by `ilo spec --json ai` to emit per-item stability annotations.
    pub fn stability(self) -> &'static str {
        match self {
            // Unreleased additions (above 0.12.1 in CHANGELOG.md → experimental).
            Builtin::Matvec
            | Builtin::Lstsq
            | Builtin::JparList
            | Builtin::GetTo
            | Builtin::PstTo
            | Builtin::TzOffset
            | Builtin::Run2
            | Builtin::RgxallMulti
            | Builtin::Fmod
            | Builtin::DtparseRel
            | Builtin::DurParse
            | Builtin::DurFmt
            | Builtin::Idxof => "experimental",

            // Everything else shipped in 0.12.1 or earlier → provisional.
            _ => "provisional",
        }
    }

    /// On-wire 8-bit tag for cross-engine builtin dispatch. See `ALL`.
    pub fn tag(self) -> u8 {
        // Linear search over a small dense table; this is only called
        // at compile time by the bytecode emitter, not on the hot path.
        Self::ALL
            .iter()
            .position(|b| *b == self)
            .expect("Builtin::ALL must include every variant") as u8
    }

    /// Inverse of `tag`. Returns `None` for unknown tags so the VM/JIT
    /// can surface a clean runtime error rather than panicking on a
    /// malformed instruction.
    pub fn from_tag(tag: u8) -> Option<Builtin> {
        Self::ALL.get(tag as usize).copied()
    }
}

/// Result of a char-by-signed-index lookup on a `&str`.
pub(crate) enum CharAtResult {
    /// The codepoint at the requested index.
    Found(char),
    /// Index was out of range; carries the total char count for error messages.
    OutOfRange { len: usize },
}

/// Fetch the i-th codepoint of `s`, supporting negative indices (`-1` = last).
///
/// Allocation-free in every path: positive indices walk `s.chars().nth(idx)`
/// (O(idx)); negative indices pay one O(n) `chars().count()` to adjust, then
/// the same `chars().nth`. Prior implementations did
/// `s.chars().collect::<Vec<char>>()` on every call, making per-char loops
/// like `@i 0..len s{c=at s i}` O(n²) AND allocating a fresh Vec per
/// iteration. The Vec allocator pressure was the observable trigger behind
/// the 222k-token "OOM" cluster in NLP workloads.
///
/// We deliberately do not branch on `s.is_ascii()` for a constant-time ASCII
/// path here: `is_ascii` itself walks the full string, so the guard would be
/// O(n) per call, more expensive than the `chars().nth(idx)` it replaces.
/// True O(1) ASCII indexing needs a cached `is_ascii` flag on the string
/// value; that's deferred with the RC-aware accumulator work.
pub(crate) fn char_at_signed(s: &str, raw_idx: i64) -> CharAtResult {
    if raw_idx >= 0 {
        let idx = raw_idx as usize;
        if let Some(c) = s.chars().nth(idx) {
            return CharAtResult::Found(c);
        }
        // Out of range: pay one O(n) pass for the count, only on error.
        return CharAtResult::OutOfRange {
            len: s.chars().count(),
        };
    }
    // Negative index: count chars to adjust, then walk again to the target.
    let len = s.chars().count();
    let adjusted = raw_idx + len as i64;
    if adjusted < 0 {
        return CharAtResult::OutOfRange { len };
    }
    match s.chars().nth(adjusted as usize) {
        Some(c) => CharAtResult::Found(c),
        None => CharAtResult::OutOfRange { len },
    }
}

/// Resolve a Python-style signed slice bound against `len`.
///
/// - `raw >= 0`: clamp to `[0, len]`.
/// - `raw < 0`: treat as `len + raw`, then clamp to `[0, len]`. So `-1` on a
///   length-5 list becomes index `4` (the last element); `-5` becomes `0`;
///   `-99` clamps to `0`.
///
/// Returned index is always in `[0, len]`, so callers can use it directly
/// as a slice bound without further checks. Matches the semantics already
/// applied to `at`'s negative index handling (`adjusted = if i < 0 { i + len }
/// else { i }`) — see `Builtin::At` in the tree-walker and `OP_AT` in the VM.
#[inline]
pub(crate) fn resolve_slice_bound(raw: i64, len: usize) -> usize {
    let len_i = len as i64;
    let adjusted = if raw < 0 { raw + len_i } else { raw };
    adjusted.clamp(0, len_i) as usize
}

/// Resolve `slc`'s `end` bound with the `-1 = to end` sugar.
///
/// `slc` historically treated negative end indices as Python-style relative
/// offsets, so `slc s 0 -1` dropped the last element. Agents trained on
/// Python/JS keep reaching for `-1` to mean "to end of string/list" instead,
/// so we add a narrow ergonomic exception: when `start_raw >= 0` and
/// `end_raw == -1`, treat the end as `len`. All other shapes (negative
/// start, or end < -1) keep the Python-style relative-offset behaviour via
/// [`resolve_slice_bound`].
///
/// This is intentionally `-1` only, not "any negative end". Treating every
/// negative end as "to end" would silently break the existing
/// `slc xs -3 -1` / `slc "hello" -99 -1` shapes that already rely on the
/// Python semantics.
#[inline]
pub(crate) fn resolve_slc_end(start_raw: i64, end_raw: i64, len: usize) -> usize {
    if start_raw >= 0 && end_raw == -1 {
        return len;
    }
    resolve_slice_bound(end_raw, len)
}

/// Resolve `take n xs` against `len`, returning the prefix length to retain.
///
/// - `n >= 0`: take the first `min(n, len)` elements.
/// - `n < 0`: take all but the last `|n|`. Equivalent to Python's `xs[:n]`.
///   `take -1 [1,2,3]` returns `[1,2]`; `take -len xs` returns `[]`; `n` more
///   negative than `-len` clamps to `0` (empty).
#[inline]
pub(crate) fn resolve_take_count(n: i64, len: usize) -> usize {
    if n >= 0 {
        (n as usize).min(len)
    } else {
        let adjusted = (len as i64) + n;
        adjusted.max(0) as usize
    }
}

/// Resolve `drop n xs` against `len`, returning the prefix length to skip.
///
/// - `n >= 0`: skip the first `min(n, len)` elements.
/// - `n < 0`: keep only the last `|n|`, i.e. skip the leading `len - |n|`.
///   Equivalent to Python's `xs[n:]`. `drop -1 [1,2,3]` returns `[3]`;
///   `drop -len xs` returns the full list; `n` more negative than `-len`
///   clamps to `0` (returns the full list).
#[inline]
pub(crate) fn resolve_drop_count(n: i64, len: usize) -> usize {
    if n >= 0 {
        (n as usize).min(len)
    } else {
        let adjusted = (len as i64) + n;
        adjusted.max(0) as usize
    }
}

/// Detects when a `jpth` path argument looks like JSONPath rather than ilo's
/// dot-path. ilo's `jpth` uses `a.b.0.c` style segments — leading `$`, `*`
/// wildcards, and `[...]` bracket selectors all signal that the caller reached
/// for JSONPath syntax. Returns a descriptive error message in that case so the
/// agent doesn't waste retries decoding a generic "key not found" diagnostic.
///
/// The first sentence is kept short so it fits cleanly in JSON error output
/// and matches the diagnostic shape used elsewhere in the runtime.
pub fn jpth_jsonpath_diagnostic(path: &str) -> Option<String> {
    // Leading `$` — classic JSONPath root selector.
    if let Some(rest) = path.strip_prefix('$') {
        // Tolerate a path that happens to be the literal key "$" — `key not found: $`
        // is still the right behaviour for that case. The diagnostic only fires when
        // `$` is followed by `.`, `[`, or `*`, all of which are JSONPath operators.
        if rest.starts_with('.') || rest.starts_with('[') || rest.starts_with('*') {
            return Some(format!(
                "jpth is dot-path only (e.g. \"a.b.0.c\"), not JSONPath. Got: \"{}\". Drop the leading `$` and use dot-separated keys / indices; for wildcards, iterate with `@i` or `map`.",
                path
            ));
        }
    }
    // Wildcard segment — JSONPath `*`, not valid in dot-path.
    if path.contains('*') {
        return Some(format!(
            "jpth is dot-path only (e.g. \"a.b.0.c\"), not JSONPath. Got: \"{}\". `*` wildcards are not supported; iterate the array yourself with `@i` after extracting it, or use `jpar` and walk the parsed value.",
            path
        ));
    }
    // Bracket selector — JSONPath `[0]` / `[*]` / `['key']`, not valid in dot-path.
    if path.contains('[') {
        return Some(format!(
            "jpth is dot-path only (e.g. \"a.b.0.c\"), not JSONPath. Got: \"{}\". Use a dot before array indices (e.g. \"items.0.name\") instead of bracket selectors.",
            path
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_all_builtins() {
        let all = [
            "range",
            "str",
            "num",
            "abs",
            "flr",
            "cel",
            "rou",
            "min",
            "max",
            "mod",
            "fmod",
            "clamp",
            "pow",
            "sqrt",
            "log",
            "exp",
            "sin",
            "cos",
            "tan",
            "log10",
            "log2",
            "asin",
            "acos",
            "atan",
            "atan2",
            "sum",
            "prod",
            "cumsum",
            "cprod",
            "ewm",
            "avg",
            "median",
            "quantile",
            "stdev",
            "variance",
            "len",
            "hd",
            "at",
            "tl",
            "rev",
            "srt",
            "rsrt",
            "slc",
            "lst",
            "take",
            "drop",
            "unq",
            "flat",
            "has",
            "spl",
            "cat",
            "zip",
            "enumerate",
            "setunion",
            "setinter",
            "setdiff",
            "map",
            "flt",
            "fld",
            "ct",
            "grp",
            "uniqby",
            "partition",
            "frq",
            "flatmap",
            "mapr",
            "rnd",
            "seed",
            "now",
            "now-ms",
            "rd",
            "rdl",
            "rdb",
            "wr",
            "wra",
            "wro",
            "wrl",
            "prnt",
            "env",
            "trm",
            "upr",
            "lwr",
            "cap",
            "padl",
            "padr",
            "ord",
            "chr",
            "chars",
            "fmt",
            "fmt2",
            "rgx",
            "rgxall",
            "rgxall1",
            "rgxsub",
            "rgxall-multi",
            "jpth",
            "jkeys",
            "jdmp",
            "jpar",
            "jpar-list",
            "get",
            "pst",
            "get-to",
            "pst-to",
            "getx",
            "pstx",
            "put",
            "pat",
            "del",
            "hed",
            "opt",
            "mmap",
            "mget",
            "mset",
            "mhas",
            "mkeys",
            "mvals",
            "mdel",
            "mget-or",
            "lget-or",
            "mpairs",
            "fft",
            "ifft",
            "window",
            "chunks",
            "transpose",
            "matmul",
            "matvec",
            "dot",
            "rndn",
            "get-many",
            "solve",
            "inv",
            "det",
            "lstsq",
            "rdjl",
            "dtfmt",
            "dtparse",
            "dtparse-rel",
            "sleep",
            "run",
            "lsd",
            "walk",
            "glob",
            "argmax",
            "argmin",
            "argsort",
            "dirname",
            "basename",
            "pathjoin",
            "rdin",
            "rdinl",
            "pi",
            "tau",
            "e",
            "dur-parse",
            "dur-fmt",
            "rand-bytes",
            "where",
            "add-mo",
            "last-dom",
            "next-business-day",
            "day-of-week",
            "linspace",
            "ones",
            "rep",
            "sha256",
            "hmac-sha256",
            "b64",
            "b64-dec",
            "hex",
            "ct-eq",
            "rsum",
            "ravg",
            "rmin",
            "bisect",
        ];
        for name in &all {
            let b = Builtin::from_name(name).unwrap_or_else(|| panic!("missing builtin: {name}"));
            assert_eq!(b.name(), *name, "round-trip failed for {name}");
        }
    }

    #[test]
    fn non_builtin_returns_none() {
        assert_eq!(Builtin::from_name("foo"), None);
        assert_eq!(Builtin::from_name(""), None);
    }

    #[test]
    fn resolve_slice_bound_positive_and_clamps() {
        // Within range: returned as-is.
        assert_eq!(resolve_slice_bound(0, 5), 0);
        assert_eq!(resolve_slice_bound(3, 5), 3);
        assert_eq!(resolve_slice_bound(5, 5), 5);
        // Past len: clamps up to len (matches existing slc behaviour).
        assert_eq!(resolve_slice_bound(99, 5), 5);
    }

    #[test]
    fn resolve_slice_bound_negative_python_style() {
        // -1 is the last index; -len is 0; beyond -len clamps to 0.
        assert_eq!(resolve_slice_bound(-1, 5), 4);
        assert_eq!(resolve_slice_bound(-5, 5), 0);
        assert_eq!(resolve_slice_bound(-99, 5), 0);
    }

    #[test]
    fn resolve_slice_bound_empty_list() {
        // len=0 makes every bound clamp to 0 — slc of an empty list always
        // returns empty, never errors. The fencepost-trap case in the
        // quant-trader run.
        assert_eq!(resolve_slice_bound(0, 0), 0);
        assert_eq!(resolve_slice_bound(-1, 0), 0);
        assert_eq!(resolve_slice_bound(99, 0), 0);
    }

    #[test]
    fn resolve_take_count_positive() {
        assert_eq!(resolve_take_count(0, 5), 0);
        assert_eq!(resolve_take_count(3, 5), 3);
        assert_eq!(resolve_take_count(5, 5), 5);
        assert_eq!(resolve_take_count(99, 5), 5);
    }

    #[test]
    fn resolve_take_count_negative_drops_tail() {
        // `take -k xs` == `xs[:-k]` — keep all but the last |k|.
        assert_eq!(resolve_take_count(-1, 5), 4);
        assert_eq!(resolve_take_count(-4, 5), 1);
        assert_eq!(resolve_take_count(-5, 5), 0);
        // Beyond -len clamps to 0 (empty), matching Python's `xs[:-99]`.
        assert_eq!(resolve_take_count(-99, 5), 0);
    }

    #[test]
    fn resolve_drop_count_positive() {
        assert_eq!(resolve_drop_count(0, 5), 0);
        assert_eq!(resolve_drop_count(3, 5), 3);
        assert_eq!(resolve_drop_count(5, 5), 5);
        assert_eq!(resolve_drop_count(99, 5), 5);
    }

    #[test]
    fn resolve_drop_count_negative_keeps_tail() {
        // `drop -k xs` == `xs[-k:]` — discard all but the last |k|.
        // Returned value is the *prefix length to skip*.
        assert_eq!(resolve_drop_count(-1, 5), 4); // skip 4, keep last 1
        assert_eq!(resolve_drop_count(-4, 5), 1); // skip 1, keep last 4
        assert_eq!(resolve_drop_count(-5, 5), 0); // skip 0, keep all
        // Beyond -len clamps to 0 (keep everything), matching Python `xs[-99:]`.
        assert_eq!(resolve_drop_count(-99, 5), 0);
    }

    #[test]
    fn resolve_take_drop_empty_list() {
        // Every count against len=0 must clamp to 0 — take/drop of empty
        // never errors, irrespective of sign.
        assert_eq!(resolve_take_count(0, 0), 0);
        assert_eq!(resolve_take_count(-3, 0), 0);
        assert_eq!(resolve_take_count(3, 0), 0);
        assert_eq!(resolve_drop_count(0, 0), 0);
        assert_eq!(resolve_drop_count(-3, 0), 0);
        assert_eq!(resolve_drop_count(3, 0), 0);
    }

    #[test]
    fn tag_round_trips_for_every_builtin() {
        // Anchor for the OP_CALL_BUILTIN_TREE bridge: every builtin must
        // tag↔from_tag cleanly, and Builtin::ALL must list every variant
        // covered by from_name (no silent drift).
        for name in &[
            "str",
            "num",
            "abs",
            "flr",
            "cel",
            "rou",
            "min",
            "max",
            "mod",
            "fmod",
            "clamp",
            "pow",
            "sqrt",
            "log",
            "exp",
            "sin",
            "cos",
            "tan",
            "log10",
            "log2",
            "asin",
            "acos",
            "atan",
            "atan2",
            "sum",
            "prod",
            "cumsum",
            "cprod",
            "ewm",
            "avg",
            "median",
            "quantile",
            "stdev",
            "variance",
            "fft",
            "ifft",
            "transpose",
            "matmul",
            "matvec",
            "dot",
            "len",
            "hd",
            "at",
            "tl",
            "rev",
            "srt",
            "rsrt",
            "slc",
            "lst",
            "take",
            "drop",
            "unq",
            "flat",
            "has",
            "spl",
            "cat",
            "zip",
            "enumerate",
            "range",
            "window",
            "chunks",
            "setunion",
            "setinter",
            "setdiff",
            "map",
            "flt",
            "fld",
            "ct",
            "grp",
            "uniqby",
            "partition",
            "frq",
            "flatmap",
            "mapr",
            "rnd",
            "rndn",
            "now",
            "now-ms",
            "dtfmt",
            "dtparse",
            "dtparse-rel",
            "rd",
            "rdl",
            "rdb",
            "wr",
            "wra",
            "wro",
            "wrl",
            "prnt",
            "env",
            "trm",
            "upr",
            "lwr",
            "cap",
            "padl",
            "padr",
            "ord",
            "chr",
            "chars",
            "fmt",
            "fmt2",
            "rgx",
            "rgxall",
            "rgxall1",
            "rgxsub",
            "rgxall-multi",
            "jpth",
            "jkeys",
            "jdmp",
            "jpar",
            "jpar-list",
            "rdjl",
            "get",
            "pst",
            "get-many",
            "get-to",
            "pst-to",
            "getx",
            "pstx",
            "put",
            "pat",
            "del",
            "hed",
            "opt",
            "mmap",
            "mget",
            "mset",
            "mhas",
            "mkeys",
            "mvals",
            "mdel",
            "mget-or",
            "lget-or",
            "mpairs",
            "solve",
            "inv",
            "det",
            "lstsq",
            "run",
            "lsd",
            "walk",
            "glob",
            "dirname",
            "basename",
            "pathjoin",
            "rdin",
            "rdinl",
            "pi",
            "tau",
            "e",
            "dur-parse",
            "dur-fmt",
            "rand-bytes",
            "where",
            "add-mo",
            "last-dom",
            "next-business-day",
            "day-of-week",
            "linspace",
            "ones",
            "rep",
            "sha256",
            "hmac-sha256",
            "b64",
            "b64-dec",
            "hex",
            "ct-eq",
            "rsum",
            "ravg",
            "rmin",
            "bisect",
            "for-line",
        ] {
            let b = Builtin::from_name(name).unwrap_or_else(|| panic!("no builtin: {name}"));
            let t = b.tag();
            let round = Builtin::from_tag(t).unwrap_or_else(|| panic!("no from_tag for {name}"));
            assert_eq!(b, round, "tag round-trip failed for {name}");
        }
        // No tag collisions.
        let tags: Vec<u8> = Builtin::ALL.iter().map(|b| b.tag()).collect();
        let mut sorted = tags.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), tags.len(), "tag collision in Builtin::ALL");
    }

    fn unwrap_found(r: CharAtResult) -> char {
        match r {
            CharAtResult::Found(c) => c,
            CharAtResult::OutOfRange { len } => panic!("expected Found, got OutOfRange len={len}"),
        }
    }

    fn unwrap_oor(r: CharAtResult) -> usize {
        match r {
            CharAtResult::OutOfRange { len } => len,
            CharAtResult::Found(c) => panic!("expected OutOfRange, got Found({c:?})"),
        }
    }

    #[test]
    fn char_at_signed_ascii_positive() {
        assert_eq!(unwrap_found(char_at_signed("hello", 0)), 'h');
        assert_eq!(unwrap_found(char_at_signed("hello", 4)), 'o');
        assert_eq!(unwrap_oor(char_at_signed("hello", 5)), 5);
        assert_eq!(unwrap_oor(char_at_signed("", 0)), 0);
    }

    #[test]
    fn char_at_signed_ascii_negative() {
        assert_eq!(unwrap_found(char_at_signed("hello", -1)), 'o');
        assert_eq!(unwrap_found(char_at_signed("hello", -5)), 'h');
        assert_eq!(unwrap_oor(char_at_signed("hello", -6)), 5);
    }

    #[test]
    fn char_at_signed_unicode_positive() {
        // "naïve" — 5 codepoints, 6 bytes
        assert_eq!(unwrap_found(char_at_signed("naïve", 0)), 'n');
        assert_eq!(unwrap_found(char_at_signed("naïve", 2)), 'ï');
        assert_eq!(unwrap_found(char_at_signed("naïve", 4)), 'e');
        assert_eq!(unwrap_oor(char_at_signed("naïve", 5)), 5);
    }

    #[test]
    fn char_at_signed_unicode_negative() {
        assert_eq!(unwrap_found(char_at_signed("naïve", -1)), 'e');
        assert_eq!(unwrap_found(char_at_signed("naïve", -3)), 'ï');
        assert_eq!(unwrap_oor(char_at_signed("naïve", -6)), 5);
    }
}
