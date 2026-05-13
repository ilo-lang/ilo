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
