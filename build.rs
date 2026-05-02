// build.rs — regenerates the compact spec at `ai.txt` from SPEC.md at compile time.
// `ilo help ai` / `ilo -ai` embeds the same file directly via `include_str!("../ai.txt")`,
// so `ai.txt` is the single source of truth for the compact spec — git-tracked, stable raw
// URL on GitHub, and embedded in the binary unchanged.
//
// CI runs `cargo build` then `git diff --exit-code ai.txt`. If SPEC.md was edited without
// regenerating, the diff is non-empty and CI fails.

fn main() {
    println!("cargo:rerun-if-changed=SPEC.md");
    let spec = std::fs::read_to_string("SPEC.md").expect("SPEC.md not found");
    let compact = compact_spec(&spec);

    // Only write when the content changed, so unchanged builds don't dirty the working tree.
    let tracked_path = std::path::Path::new("ai.txt");
    let needs_write = match std::fs::read_to_string(tracked_path) {
        Ok(existing) => existing != compact,
        Err(_) => true,
    };
    if needs_write {
        std::fs::write(tracked_path, &compact).expect("failed to write ai.txt");
    }
}

/// Compress the spec into one line per `## Section`.
/// - Table headers + separator rows are dropped; data rows become `key=value` tokens.
/// - Bullet points are joined with `;`.
/// - `### Subsection` becomes an inline `[Subsection]` label.
/// - Code fence markers, blank lines, and `---` dividers are stripped.
/// - Everything within a section is joined with ` ` and emitted as `SECTION: content`.
fn compact_spec(src: &str) -> String {
    // Split into (heading, content_lines) sections.
    // The preamble (before first ## heading) gets an empty heading.
    let mut sections: Vec<(String, Vec<String>)> = vec![("".into(), vec![])];

    for line in src.lines() {
        let trimmed = line.trim();
        if let Some(h) = trimmed.strip_prefix("## ") {
            sections.push((h.to_uppercase(), vec![]));
        } else {
            sections
                .last_mut()
                .expect("sections always non-empty")
                .1
                .push(trimmed.to_string());
        }
    }

    let mut out = String::new();

    for (heading, raw_lines) in sections {
        let tokens = compress_section(&raw_lines);
        if tokens.is_empty() {
            continue;
        }
        if heading.is_empty() {
            out.push_str(&tokens);
        } else {
            out.push_str(&heading);
            out.push_str(": ");
            out.push_str(&tokens);
        }
        out.push('\n');
    }

    out
}

/// Compress a section's lines into a single string.
fn compress_section(lines: &[String]) -> String {
    #[derive(PartialEq)]
    enum TableState {
        NotInTable,
        InHeader, // first data row seen, separator not yet seen
        InData,   // past the separator row — real data rows
    }

    let mut items: Vec<String> = Vec::new();
    let mut table_state = TableState::NotInTable;

    for line in lines {
        let t = line.as_str();

        // Blank lines, horizontal rules, and code-fence markers are noise.
        if t.is_empty() || t == "---" || t.starts_with("```") {
            continue;
        }

        if let Some(sub) = t.strip_prefix("### ") {
            // Subsection heading inline.
            table_state = TableState::NotInTable;
            items.push(format!("[{sub}]"));
            continue;
        }

        if t.starts_with('|') {
            let is_sep = t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '));
            if is_sep {
                // Separator row: marks end of header, start of data.
                table_state = TableState::InData;
                continue;
            }
            match table_state {
                TableState::NotInTable => {
                    // First row of a new table = the header row — skip it.
                    table_state = TableState::InHeader;
                }
                TableState::InHeader => {
                    // Still before the separator (unusual: two header rows?) — skip.
                }
                TableState::InData => {
                    // Real data row: extract cells.
                    let cells: Vec<&str> = t
                        .split('|')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .collect();
                    items.push(cells.join("="));
                }
            }
            continue;
        }

        // Non-table line — reset table state.
        table_state = TableState::NotInTable;

        if let Some(bullet) = t.strip_prefix("- ") {
            items.push(bullet.to_string());
        } else {
            items.push(t.to_string());
        }
    }

    items.join(" ")
}
