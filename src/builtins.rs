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
    Avg,

    // Collections
    Len,
    Hd,
    At,
    Tl,
    Rev,
    Srt,
    Slc,
    Unq,
    Flat,
    Has,
    Spl,
    Cat,

    // Higher-order
    Map,
    Flt,
    Fld,
    Grp,

    // Random / time
    Rnd,
    Now,

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
    Fmt,
    Rgx,

    // JSON
    Jpth,
    Jdmp,
    Jpar,

    // HTTP
    Get,
    Post,

    // Map (associative array)
    Mmap,
    Mget,
    Mset,
    Mhas,
    Mkeys,
    Mvals,
    Mdel,
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
            "avg" => Some(Builtin::Avg),
            "len" => Some(Builtin::Len),
            "hd" => Some(Builtin::Hd),
            "at" => Some(Builtin::At),
            "tl" => Some(Builtin::Tl),
            "rev" => Some(Builtin::Rev),
            "srt" => Some(Builtin::Srt),
            "slc" => Some(Builtin::Slc),
            "unq" => Some(Builtin::Unq),
            "flat" => Some(Builtin::Flat),
            "has" => Some(Builtin::Has),
            "spl" => Some(Builtin::Spl),
            "cat" => Some(Builtin::Cat),
            "map" => Some(Builtin::Map),
            "flt" => Some(Builtin::Flt),
            "fld" => Some(Builtin::Fld),
            "grp" => Some(Builtin::Grp),
            "rnd" => Some(Builtin::Rnd),
            "now" => Some(Builtin::Now),
            "rd" => Some(Builtin::Rd),
            "rdl" => Some(Builtin::Rdl),
            "rdb" => Some(Builtin::Rdb),
            "wr" => Some(Builtin::Wr),
            "wrl" => Some(Builtin::Wrl),
            "prnt" => Some(Builtin::Prnt),
            "env" => Some(Builtin::Env),
            "trm" => Some(Builtin::Trm),
            "fmt" => Some(Builtin::Fmt),
            "rgx" => Some(Builtin::Rgx),
            "jpth" => Some(Builtin::Jpth),
            "jdmp" => Some(Builtin::Jdmp),
            "jpar" => Some(Builtin::Jpar),
            "get" => Some(Builtin::Get),
            "post" => Some(Builtin::Post),
            "mmap" => Some(Builtin::Mmap),
            "mget" => Some(Builtin::Mget),
            "mset" => Some(Builtin::Mset),
            "mhas" => Some(Builtin::Mhas),
            "mkeys" => Some(Builtin::Mkeys),
            "mvals" => Some(Builtin::Mvals),
            "mdel" => Some(Builtin::Mdel),
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
            Builtin::Avg => "avg",
            Builtin::Len => "len",
            Builtin::Hd => "hd",
            Builtin::At => "at",
            Builtin::Tl => "tl",
            Builtin::Rev => "rev",
            Builtin::Srt => "srt",
            Builtin::Slc => "slc",
            Builtin::Unq => "unq",
            Builtin::Flat => "flat",
            Builtin::Has => "has",
            Builtin::Spl => "spl",
            Builtin::Cat => "cat",
            Builtin::Map => "map",
            Builtin::Flt => "flt",
            Builtin::Fld => "fld",
            Builtin::Grp => "grp",
            Builtin::Rnd => "rnd",
            Builtin::Now => "now",
            Builtin::Rd => "rd",
            Builtin::Rdl => "rdl",
            Builtin::Rdb => "rdb",
            Builtin::Wr => "wr",
            Builtin::Wrl => "wrl",
            Builtin::Prnt => "prnt",
            Builtin::Env => "env",
            Builtin::Trm => "trm",
            Builtin::Fmt => "fmt",
            Builtin::Rgx => "rgx",
            Builtin::Jpth => "jpth",
            Builtin::Jdmp => "jdmp",
            Builtin::Jpar => "jpar",
            Builtin::Get => "get",
            Builtin::Post => "post",
            Builtin::Mmap => "mmap",
            Builtin::Mget => "mget",
            Builtin::Mset => "mset",
            Builtin::Mhas => "mhas",
            Builtin::Mkeys => "mkeys",
            Builtin::Mvals => "mvals",
            Builtin::Mdel => "mdel",
        }
    }

    /// Check if a name refers to a builtin function.
    pub fn is_builtin(name: &str) -> bool {
        Self::from_name(name).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_all_builtins() {
        let all = [
            "str", "num", "abs", "flr", "cel", "rou", "min", "max", "mod", "pow", "sqrt", "log",
            "exp", "sin", "cos", "tan", "log10", "log2", "atan2", "sum", "avg", "len", "hd", "at",
            "tl", "rev", "srt", "slc", "unq", "flat", "has", "spl", "cat", "map", "flt", "fld",
            "grp", "rnd", "now", "rd", "rdl", "rdb", "wr", "wrl", "prnt", "env", "trm", "fmt",
            "rgx", "jpth", "jdmp", "jpar", "get", "post", "mmap", "mget", "mset", "mhas", "mkeys",
            "mvals", "mdel",
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
}
