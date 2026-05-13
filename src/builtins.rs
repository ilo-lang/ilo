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
    Atan2,
    Sum,
    Cumsum,
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
    Window,
    Chunks,
    Setunion,
    Setinter,
    Setdiff,

    // Higher-order
    Map,
    Flt,
    Fld,
    Grp,
    Uniqby,
    Partition,
    Frq,
    Flatmap,

    // Random / time
    Rnd,
    Rndn,
    Now,
    Dtfmt,
    Dtparse,
    Sleep,

    // I/O
    Rd,
    Rdl,
    Rdb,
    Wr,
    Wrl,
    Prnt,
    Env,

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
    Rgxsub,

    // JSON
    Jpth,
    Jdmp,
    Jpar,
    Rdjl,

    // HTTP
    Get,
    Post,
    GetMany,

    // Map (associative array)
    Mmap,
    Mget,
    Mset,
    Mhas,
    Mkeys,
    Mvals,
    Mdel,

    // Linear algebra
    Solve,
    Inv,
    Det,
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
            "atan2" => Some(Builtin::Atan2),
            "sum" => Some(Builtin::Sum),
            "cumsum" => Some(Builtin::Cumsum),
            "avg" => Some(Builtin::Avg),
            "median" => Some(Builtin::Median),
            "quantile" => Some(Builtin::Quantile),
            "stdev" => Some(Builtin::Stdev),
            "variance" => Some(Builtin::Variance),
            "fft" => Some(Builtin::Fft),
            "ifft" => Some(Builtin::Ifft),
            "transpose" => Some(Builtin::Transpose),
            "matmul" => Some(Builtin::Matmul),
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
            "window" => Some(Builtin::Window),
            "chunks" => Some(Builtin::Chunks),
            "setunion" => Some(Builtin::Setunion),
            "setinter" => Some(Builtin::Setinter),
            "setdiff" => Some(Builtin::Setdiff),
            "map" => Some(Builtin::Map),
            "flt" => Some(Builtin::Flt),
            "fld" => Some(Builtin::Fld),
            "grp" => Some(Builtin::Grp),
            "uniqby" => Some(Builtin::Uniqby),
            "partition" => Some(Builtin::Partition),
            "frq" => Some(Builtin::Frq),
            "flatmap" => Some(Builtin::Flatmap),
            "rnd" => Some(Builtin::Rnd),
            "rndn" => Some(Builtin::Rndn),
            "now" => Some(Builtin::Now),
            "dtfmt" => Some(Builtin::Dtfmt),
            "dtparse" => Some(Builtin::Dtparse),
            "sleep" => Some(Builtin::Sleep),
            "rd" => Some(Builtin::Rd),
            "rdl" => Some(Builtin::Rdl),
            "rdb" => Some(Builtin::Rdb),
            "wr" => Some(Builtin::Wr),
            "wrl" => Some(Builtin::Wrl),
            "prnt" => Some(Builtin::Prnt),
            "env" => Some(Builtin::Env),
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
            "rgxsub" => Some(Builtin::Rgxsub),
            "jpth" => Some(Builtin::Jpth),
            "jdmp" => Some(Builtin::Jdmp),
            "jpar" => Some(Builtin::Jpar),
            "rdjl" => Some(Builtin::Rdjl),
            "get" => Some(Builtin::Get),
            "post" => Some(Builtin::Post),
            "get-many" => Some(Builtin::GetMany),
            "mmap" => Some(Builtin::Mmap),
            "mget" => Some(Builtin::Mget),
            "mset" => Some(Builtin::Mset),
            "mhas" => Some(Builtin::Mhas),
            "mkeys" => Some(Builtin::Mkeys),
            "mvals" => Some(Builtin::Mvals),
            "mdel" => Some(Builtin::Mdel),
            "solve" => Some(Builtin::Solve),
            "inv" => Some(Builtin::Inv),
            "det" => Some(Builtin::Det),
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
            Builtin::Atan2 => "atan2",
            Builtin::Sum => "sum",
            Builtin::Cumsum => "cumsum",
            Builtin::Avg => "avg",
            Builtin::Median => "median",
            Builtin::Quantile => "quantile",
            Builtin::Stdev => "stdev",
            Builtin::Variance => "variance",
            Builtin::Fft => "fft",
            Builtin::Ifft => "ifft",
            Builtin::Transpose => "transpose",
            Builtin::Matmul => "matmul",
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
            Builtin::Window => "window",
            Builtin::Chunks => "chunks",
            Builtin::Setunion => "setunion",
            Builtin::Setinter => "setinter",
            Builtin::Setdiff => "setdiff",
            Builtin::Map => "map",
            Builtin::Flt => "flt",
            Builtin::Fld => "fld",
            Builtin::Grp => "grp",
            Builtin::Uniqby => "uniqby",
            Builtin::Partition => "partition",
            Builtin::Frq => "frq",
            Builtin::Flatmap => "flatmap",
            Builtin::Rnd => "rnd",
            Builtin::Rndn => "rndn",
            Builtin::Now => "now",
            Builtin::Dtfmt => "dtfmt",
            Builtin::Dtparse => "dtparse",
            Builtin::Sleep => "sleep",
            Builtin::Rd => "rd",
            Builtin::Rdl => "rdl",
            Builtin::Rdb => "rdb",
            Builtin::Wr => "wr",
            Builtin::Wrl => "wrl",
            Builtin::Prnt => "prnt",
            Builtin::Env => "env",
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
            Builtin::Rgxsub => "rgxsub",
            Builtin::Jpth => "jpth",
            Builtin::Jdmp => "jdmp",
            Builtin::Jpar => "jpar",
            Builtin::Rdjl => "rdjl",
            Builtin::Get => "get",
            Builtin::Post => "post",
            Builtin::GetMany => "get-many",
            Builtin::Mmap => "mmap",
            Builtin::Mget => "mget",
            Builtin::Mset => "mset",
            Builtin::Mhas => "mhas",
            Builtin::Mkeys => "mkeys",
            Builtin::Mvals => "mvals",
            Builtin::Mdel => "mdel",
            Builtin::Solve => "solve",
            Builtin::Inv => "inv",
            Builtin::Det => "det",
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
        Builtin::Atan2,
        Builtin::Sum,
        Builtin::Cumsum,
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
        Builtin::Rnd,
        Builtin::Rndn,
        Builtin::Now,
        Builtin::Dtfmt,
        Builtin::Dtparse,
        Builtin::Rd,
        Builtin::Rdl,
        Builtin::Rdb,
        Builtin::Wr,
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
        Builtin::Jdmp,
        Builtin::Jpar,
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
    ];

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
            "atan2",
            "sum",
            "cumsum",
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
            "grp",
            "uniqby",
            "partition",
            "frq",
            "flatmap",
            "rnd",
            "now",
            "rd",
            "rdl",
            "rdb",
            "wr",
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
            "rgxsub",
            "jpth",
            "jdmp",
            "jpar",
            "get",
            "post",
            "mmap",
            "mget",
            "mset",
            "mhas",
            "mkeys",
            "mvals",
            "mdel",
            "fft",
            "ifft",
            "window",
            "chunks",
            "transpose",
            "matmul",
            "dot",
            "rndn",
            "get-many",
            "solve",
            "inv",
            "det",
            "rdjl",
            "dtfmt",
            "dtparse",
            "sleep",
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
            "atan2",
            "sum",
            "cumsum",
            "avg",
            "median",
            "quantile",
            "stdev",
            "variance",
            "fft",
            "ifft",
            "transpose",
            "matmul",
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
            "grp",
            "uniqby",
            "partition",
            "frq",
            "flatmap",
            "rnd",
            "rndn",
            "now",
            "dtfmt",
            "dtparse",
            "rd",
            "rdl",
            "rdb",
            "wr",
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
            "rgxsub",
            "jpth",
            "jdmp",
            "jpar",
            "rdjl",
            "get",
            "post",
            "get-many",
            "mmap",
            "mget",
            "mset",
            "mhas",
            "mkeys",
            "mvals",
            "mdel",
            "solve",
            "inv",
            "det",
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
