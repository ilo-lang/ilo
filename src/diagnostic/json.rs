use super::{Diagnostic, FixPlan, Severity};
use crate::ast::SourceMap;

fn serialize_fix_plan(plan: &FixPlan) -> serde_json::Value {
    let edits: Vec<serde_json::Value> = plan
        .edits
        .iter()
        .map(|e| {
            serde_json::json!({
                "line_range": [e.line_start, e.line_end],
                "before": e.before,
                "after": e.after,
            })
        })
        .collect();

    let mut obj = serde_json::json!({ "edits": edits });
    if let Some(p) = &plan.path {
        obj["path"] = serde_json::Value::String(p.clone());
    }
    obj
}

pub fn render(d: &Diagnostic) -> String {
    let severity = match d.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };

    // Build SourceMap once (not per-label) if source is available
    let source_map = d.source.as_deref().map(SourceMap::new);

    let labels: Vec<serde_json::Value> = d
        .labels
        .iter()
        .map(|l| {
            let mut obj = serde_json::json!({
                "start": l.span.start,
                "end": l.span.end,
                "message": l.message,
                "primary": l.is_primary,
            });
            if let Some(map) = &source_map {
                let (line, col) = map.lookup(l.span.start);
                obj["line"] = serde_json::Value::from(line);
                obj["col"] = serde_json::Value::from(col);
            }
            obj
        })
        .collect();

    let mut obj = serde_json::json!({
        "severity": severity,
        "message": d.message,
        "labels": labels,
        "notes": d.notes,
    });

    if let Some(code) = d.code {
        obj["code"] = serde_json::Value::String(code.to_string());
    }

    if let Some(s) = &d.suggestion {
        obj["suggestion"] = serde_json::Value::String(s.clone());
    }

    if let Some(plan) = &d.fix_plan {
        obj["fix_plan"] = serialize_fix_plan(plan);
    }

    serde_json::to_string(&obj).unwrap_or_else(|_| {
        r#"{"severity":"error","message":"internal error serializing diagnostic"}"#.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Span;

    fn parse_json(s: &str) -> serde_json::Value {
        serde_json::from_str(s).expect("valid JSON")
    }

    #[test]
    fn render_basic_error() {
        let d = Diagnostic::error("type mismatch");
        let out = render(&d);
        let v = parse_json(&out);
        assert_eq!(v["severity"], "error");
        assert_eq!(v["message"], "type mismatch");
        assert!(v["labels"].as_array().unwrap().is_empty());
    }

    #[test]
    fn render_with_span_and_source() {
        let d = Diagnostic::error("bad token")
            .with_span(Span { start: 2, end: 5 }, "here")
            .with_source("f x:n>n;x".to_string());
        let out = render(&d);
        let v = parse_json(&out);
        let label = &v["labels"][0];
        assert_eq!(label["start"], 2);
        assert_eq!(label["end"], 5);
        assert_eq!(label["primary"], true);
        assert_eq!(label["line"], 1);
        assert_eq!(label["col"], 3);
    }

    #[test]
    fn render_with_suggestion() {
        let d = Diagnostic::error("bad").with_suggestion("try this instead");
        let out = render(&d);
        let v = parse_json(&out);
        assert_eq!(v["suggestion"], "try this instead");
    }

    #[test]
    fn render_with_notes() {
        let d = Diagnostic::error("bad")
            .with_note("in function 'f'")
            .with_note("called from 'g'");
        let out = render(&d);
        let v = parse_json(&out);
        let notes = v["notes"].as_array().unwrap();
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0], "in function 'f'");
    }

    #[test]
    fn render_no_suggestion_key_absent() {
        let d = Diagnostic::error("bad");
        let out = render(&d);
        let v = parse_json(&out);
        // suggestion key should be absent when None
        assert!(v.get("suggestion").is_none() || v["suggestion"].is_null());
    }

    #[test]
    fn render_label_without_source_no_line_col() {
        let d = Diagnostic::error("bad").with_span(Span { start: 5, end: 8 }, "here");
        let out = render(&d);
        let v = parse_json(&out);
        let label = &v["labels"][0];
        // No source → no line/col fields
        assert!(label.get("line").is_none());
        assert!(label.get("col").is_none());
    }

    #[test]
    fn render_is_valid_json() {
        let d = Diagnostic::error("complex error")
            .with_span(Span { start: 0, end: 5 }, "primary")
            .with_secondary_span(Span { start: 10, end: 12 }, "secondary")
            .with_note("some note")
            .with_suggestion("fix it")
            .with_source("hello world test".to_string());
        let out = render(&d);
        // Must be parseable JSON
        parse_json(&out);
    }

    #[test]
    fn render_warning_severity() {
        let mut d = Diagnostic::error("unused variable");
        d.severity = Severity::Warning;
        let out = render(&d);
        let v = parse_json(&out);
        assert_eq!(v["severity"], "warning");
    }
}
