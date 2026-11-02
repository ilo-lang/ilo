#[cfg(test)]
mod probe {
    #[test]
    fn range_fix() {
        let e = crate::verify::VerifyError {
            code: "ILO-T006",
            function: "f".to_string(),
            message: "arity mismatch: 'range' expects 2 args, got 1".to_string(),
            hint: Some(crate::diagnostic::FIX_HINT_RANGE_ZERO_START.to_string()),
            span: None,
            is_warning: false,
        };
        let d = crate::diagnostic::Diagnostic::from(&e);
        eprintln!("fix={:?} suggestion={:?}", d.fix, d.suggestion);
    }
}
