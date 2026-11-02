//! Builtin catalogue for agent-facing semantic queries, generated from the verifier's signature table.
//!
//! The single source of truth is `verify::BUILTINS`: `(name, param_types,
//! return_type)`. Nothing is hardcoded here, so the catalogue cannot drift
//! from the language — when a builtin is added, its signature row lands in
//! the same commit (enforced by `all_names_have_signatures` below and the
//! existing SPEC reserve-list guard).
//!
//! Alias spellings (`floor`/`flr`, `rng`/`range`) are grouped via
//! `ast::all_builtin_aliases` so lookup works under either spelling.

use crate::ast;
use crate::verify;

/// One overload of a builtin's signature.
pub struct BuiltinSig {
    pub params: &'static [&'static str],
    pub ret: &'static str,
}

/// All spellings of `name`: itself plus any alias-pair mate
/// (`floor` ↔ `flr`, `rng` ↔ `range`).
pub fn spellings(name: &str) -> Vec<&'static str> {
    let mut out = vec![];
    // `name` itself, if it has any signature row.
    if verify::BUILTINS.iter().any(|(n, _, _)| *n == name) {
        out.push(name_as_static(name));
    }
    for (a, b) in ast::all_builtin_aliases() {
        if a == name && !out.contains(&b) {
            out.push(b);
        } else if b == name && !out.contains(&a) {
            out.push(a);
        }
    }
    out
}

/// All overloads for `name` under any of its spellings, in table order.
pub fn signatures(name: &str) -> Vec<BuiltinSig> {
    let spells = spellings(name);
    verify::BUILTINS
        .iter()
        .filter(|(n, _, _)| spells.contains(n))
        .map(|(_, params, ret)| BuiltinSig { params, ret })
        .collect()
}

/// Short signature label for completion details / signature help.
/// Overloads are joined: `min n n → n | min list → n`.
pub fn signature_label(name: &str) -> Option<String> {
    let sigs = signatures(name);
    if sigs.is_empty() {
        return None;
    }
    let parts: Vec<String> = sigs
        .iter()
        .map(|s| {
            if s.params.is_empty() {
                format!("{} → {}", name, s.ret)
            } else {
                format!("{} {} → {}", name, s.params.join(" "), s.ret)
            }
        })
        .collect();
    Some(parts.join("  |  "))
}

/// Hover markdown for a builtin, including alternate spellings.
pub fn hover_markdown(name: &str) -> Option<String> {
    let label = signature_label(name)?;
    let mut spells = spellings(name);
    spells.retain(|s| *s != name);
    let also = if spells.is_empty() {
        String::new()
    } else {
        let list: Vec<String> = spells.iter().map(|s| format!("`{}`", s)).collect();
        format!("\n\nAlso spelled {}.", list.join(", "))
    };
    Some(format!("**builtin** `{}`\n\n`{}`{}", name, label, also))
}

/// Reborrow a `&str` known to be `&'static` (table or alias entry).
fn name_as_static(name: &str) -> &'static str {
    if let Some(hit) = verify::BUILTINS
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|(n, _, _)| *n)
    {
        return hit;
    }
    // Must be an alias-table entry then (caller checked `spellings`
    // non-empty); find its static spelling.
    for (a, b) in ast::all_builtin_aliases() {
        if a == name {
            return a;
        }
        if b == name {
            return b;
        }
    }
    unreachable!("spellings() was non-empty for {}", name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins::Builtin;

    /// Drift guard: every builtin must have a signature row, otherwise
    /// semantic lookup silently omits it.
    #[test]
    fn all_names_have_signatures() {
        let mut missing = Vec::new();
        for b in Builtin::ALL {
            let name = b.name();
            if signatures(name).is_empty() {
                missing.push(name.to_string());
            }
        }
        assert!(
            missing.is_empty(),
            "builtins missing verify::BUILTINS rows: {:?}",
            missing
        );
    }

    #[test]
    fn overloads_spellings_reserved() {
        assert!(signatures("min").len() >= 2);
        assert!(spellings("flr").contains(&"floor"));
        assert!(spellings("floor").contains(&"flr"));
        assert!(spellings("rng").contains(&"range"));
        assert!(!spellings("sum").is_empty());
        assert!(spellings("myvar").is_empty());
    }
}
