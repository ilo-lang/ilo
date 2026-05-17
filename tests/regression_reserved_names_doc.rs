// Drift-guard: the enumerated short-name reserve list in SPEC.md
// (### Reserved namespaces) must match the actual `Builtin::ALL` registry.
//
// Why this test matters: SPEC.md publishes a forward-compatibility forecast
// to agents — "2-char names not on this list are user-safe forever". If a
// new builtin lands without updating the enumeration, the forecast becomes
// silently wrong and the next persona run hits ILO-P011 with no warning in
// the spec. This is the exact pattern that broke marketing-analyst rerun7
// on the v0.11.6 `ct` addition; the test makes the failure mode loud at
// CI time instead of weeks later at user time.
//
// The test scrapes the SPEC's reserved-namespaces enumeration and compares
// it to `Builtin::ALL`. On drift it fails with a precise message that
// names the offending builtin and quotes the AGENTS.md "Adding builtins"
// rule the maintainer should follow.

use ilo::builtins::Builtin;
use std::collections::BTreeSet;
use std::fs;

/// Parse the short-name enumeration block out of SPEC.md.
/// The block sits inside a fenced code block under the
/// `### Reserved namespaces` subsection and looks like:
///
///     ```
///     2-char  at hd tl rd wr ct
///     3-char  abs acos asin ...
///     ```
///
/// Returns the union of every whitespace-separated token after the
/// `N-char` label (which is itself stripped).
fn parse_spec_short_names() -> BTreeSet<String> {
    let spec = fs::read_to_string("SPEC.md").expect("SPEC.md not found");

    let header = "### Reserved namespaces";
    let start = spec.find(header).expect(
        "SPEC.md missing `### Reserved namespaces` subsection (see AGENTS.md > Adding builtins)",
    );

    let after_header = &spec[start..];
    // The enumerated short-name list is the first fenced block after the header.
    let fence_open = after_header
        .find("```")
        .expect("Reserved namespaces section is missing its enumerated code block");
    let block_start = fence_open + 3;
    let block_rest = &after_header[block_start..];
    // Skip the optional language tag line up to the first newline.
    let body_start = block_rest.find('\n').map(|i| i + 1).unwrap_or(0);
    let body_rest = &block_rest[body_start..];
    let fence_close = body_rest
        .find("```")
        .expect("Reserved namespaces code block is not terminated");
    let body = &body_rest[..fence_close];

    let mut names = BTreeSet::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Strip the leading `N-char` label if present; otherwise treat the
        // whole line as continuation tokens (the 3-char block wraps).
        let payload = match line.split_once(char::is_whitespace) {
            Some((head, tail)) if head.ends_with("-char") => tail,
            _ => line,
        };
        for tok in payload.split_whitespace() {
            names.insert(tok.to_string());
        }
    }
    names
}

#[test]
fn spec_reserved_short_names_match_builtin_registry() {
    let spec_names = parse_spec_short_names();

    let mut registry_short: BTreeSet<String> = BTreeSet::new();
    for b in Builtin::ALL {
        let n = b.name();
        // Aliases like `get-many` are not user-name-shaped; the
        // ILO-P011 collision check ignores them. The reserved-namespaces
        // enumeration mirrors that scope: identifiers only.
        if n.chars()
            .any(|c| !c.is_ascii_lowercase() && !c.is_ascii_digit())
        {
            continue;
        }
        if n.len() <= 3 {
            registry_short.insert(n.to_string());
        }
    }

    let missing_in_spec: Vec<&String> = registry_short.difference(&spec_names).collect();
    let extra_in_spec: Vec<&String> = spec_names.difference(&registry_short).collect();

    let mut failures = Vec::new();
    if !missing_in_spec.is_empty() {
        failures.push(format!(
            "Builtins missing from SPEC.md `### Reserved namespaces`: {:?}\n\
             Add them to the enumerated list and update the changelog. Per\n\
             AGENTS.md > Adding builtins, new builtins should land under 4+\n\
             char names — if you needed a 2- or 3-char form, that's a\n\
             reservation that must be published.",
            missing_in_spec
        ));
    }
    if !extra_in_spec.is_empty() {
        failures.push(format!(
            "SPEC.md `### Reserved namespaces` lists names that are not in\n\
             `Builtin::ALL`: {:?}\n\
             Either remove them from the doc or restore the builtin. The\n\
             forward-compat promise to agents is that listed names stay\n\
             reserved, so removal also needs a changelog entry.",
            extra_in_spec
        ));
    }

    assert!(
        failures.is_empty(),
        "Reserved-name doc drift detected:\n\n{}",
        failures.join("\n\n")
    );
}

#[test]
fn spec_reserved_namespaces_section_exists() {
    let spec = fs::read_to_string("SPEC.md").expect("SPEC.md not found");
    assert!(
        spec.contains("### Reserved namespaces"),
        "SPEC.md must contain a `### Reserved namespaces` subsection \
         documenting the short-name reserve list and the forward-compat \
         forecast. See AGENTS.md > Adding builtins."
    );
    assert!(
        spec.contains("4 characters or longer"),
        "SPEC.md `### Reserved namespaces` must publish the forward-compat \
         rule (new builtins land under 4+ char names). Without it, agents \
         have no basis for the 'unreserved 2-char names are safe forever' \
         strategy."
    );
}
