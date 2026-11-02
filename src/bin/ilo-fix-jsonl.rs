//! Batch adapter for the authoritative language-service fixer.
//!
//! Input: one JSON object per line, `{"source":"...", "max_iters":10}`.
//! Output: one `lang_svc::repair` JSON result per line. This is intentionally
//! a separate helper binary so dataset repair never replaces the trusted
//! `ilovalid`/`ilorun` executables.

use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

fn process_line(line: &str) -> Value {
    let request: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) => return json!({"ok": false, "error": format!("invalid JSON: {error}")}),
    };
    let Some(source) = request.get("source").and_then(Value::as_str) else {
        return json!({"ok": false, "error": "missing string field `source`"});
    };
    let max_iters = request
        .get("max_iters")
        .and_then(Value::as_u64)
        .unwrap_or(10)
        .min(100) as usize;
    if request.get("safe_only").and_then(Value::as_bool).unwrap_or(false) {
        ilo::lang_svc::repair_safe(source, None, max_iters)
    } else {
        ilo::lang_svc::repair(source, None, max_iters)
    }
}

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in stdin.lock().lines() {
        let value = match line {
            Ok(line) => process_line(&line),
            Err(error) => json!({"ok": false, "error": format!("input error: {error}")}),
        };
        if writeln!(stdout, "{value}").is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repairs_type_proven_at_swap() {
        let result = process_line(r#"{"source":"f xs:L n>n;at 0 xs"}"#);
        assert_eq!(result["ok"], true, "{result}");
        assert_eq!(result["fixed_source"], "f xs:L n>n;at xs 0");
    }

    #[test]
    fn safe_mode_applies_only_type_proven_fixes() {
        let result = process_line(r#"{"source":"f xs:L n>n;at 0 xs", "safe_only":true}"#);
        assert_eq!(result["ok"], true, "{result}");
        assert_eq!(result["fixed_source"], "f xs:L n>n;at xs 0");

        let boolean = process_line(r#"{"source":"f flag:b>b;not flag", "safe_only":true}"#);
        assert_eq!(boolean["ok"], true, "{boolean}");
        assert_eq!(boolean["fixed_source"], "f flag:b>b;!flag");

        let generic = process_line(r#"{"source":"f x:n>n;unknow x", "safe_only":true}"#);
        assert_eq!(generic["applied"], 0, "{generic}");
        assert!(generic["fixed_source"].is_null());
    }

    #[test]
    fn rejects_missing_source_cleanly() {
        let result = process_line("{}");
        assert_eq!(result["ok"], false);
        assert!(result["error"].as_str().unwrap().contains("source"));
    }
}
