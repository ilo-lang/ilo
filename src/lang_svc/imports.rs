//! Import resolution shared with the CLI check pipeline.
use crate::{ast, lexer, parser};
use crate::diagnostic::Diagnostic;
/// Return the declared name of a `Decl`, if it has one.
pub fn decl_name(decl: &ast::Decl) -> Option<&str> {
    match decl {
        ast::Decl::Function { name, .. } => Some(name),
        ast::Decl::Tool { name, .. } => Some(name),
        ast::Decl::TypeDef { name, .. } => Some(name),
        ast::Decl::Alias { name, .. } => Some(name),
        ast::Decl::SumType { name, .. } => Some(name),
        ast::Decl::Use { .. } | ast::Decl::VersionPragma { .. } | ast::Decl::Error { .. } => None,
    }
}

/// Build target used to evaluate conditional `use ?<pred>` predicates.
///
/// Passed to `resolve_imports` so that `use ?wasm "a" : "b"` picks the
/// right branch at compile time without touching the runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BuildTarget {
    /// Native host binary (default).
    #[default]
    Native,
    /// WebAssembly (wasm32) target.
    Wasm,
    /// Test run (`ilo test`).
    Test,
}

impl BuildTarget {
    /// Evaluate a `UsePredicate` against this target: returns `true` when
    /// the predicate matches (i.e. the "true" branch should be imported).
    pub fn eval(self, pred: ast::UsePredicate) -> bool {
        matches!(
            (self, pred),
            (BuildTarget::Wasm, ast::UsePredicate::Wasm)
                | (BuildTarget::Native, ast::UsePredicate::Native)
                | (BuildTarget::Test, ast::UsePredicate::Test)
        )
    }
}

/// Apply the `only [name1 name2]` filter from a `use` statement.
/// Pushes `ILO-P019` diagnostics for names not found.
fn apply_only_filter(
    decls: Vec<ast::Decl>,
    only: &Option<Vec<String>>,
    path: &str,
    span: ast::Span,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<ast::Decl> {
    let Some(names) = only else {
        return decls;
    };
    for name in names {
        let found = decls.iter().any(|d| decl_name(d) == Some(name.as_str()));
        if !found {
            diagnostics.push(
                Diagnostic::error(format!(
                    "use \"{}\": name '{}' not found in imported file",
                    path, name
                ))
                .with_code("ILO-P019")
                .with_span(span, "imported here"),
            );
        }
    }
    decls
        .into_iter()
        .filter(|d| {
            decl_name(d)
                .map(|n| names.iter().any(|s| s == n))
                .unwrap_or(false)
        })
        .collect()
}

/// Resolve all `Decl::Use` nodes in `decls` recursively, returning a flat
/// merged list with imported declarations prepended and `Use` nodes stripped.
///
/// - `base_dir`: directory of the importing file (used to resolve relative paths).
///   `None` means inline code — `use` is not supported without a file context.
/// - `visited`: canonical paths already in the import chain; circular imports are errors.
/// - `diagnostics`: errors are pushed here (file-not-found, circular, parse failures).
/// - `build_target`: evaluates `use ?<pred>` predicates at compile time.
///
/// Privacy rule: declarations whose name starts with `_` are module-private and
/// are never exported. They are stripped during import regardless of `only` or `alias`.
///
/// Lazy imports (`use lazy:"./path"`) are deferred: the module file is only
/// opened if at least one `<stem>-*` symbol is referenced in the non-lazy
/// declarations. This lets large optional modules be listed without paying
/// any parse/IO cost when unused.
pub fn resolve_imports(
    decls: Vec<ast::Decl>,
    base_dir: Option<&std::path::Path>,
    visited: &mut std::collections::HashSet<std::path::PathBuf>,
    diagnostics: &mut Vec<Diagnostic>,
    build_target: BuildTarget,
) -> Vec<ast::Decl> {
    // Separate lazy Use nodes from everything else for a two-pass approach.
    let mut eager_decls: Vec<ast::Decl> = Vec::new();
    let mut lazy_pending: Vec<ast::Decl> = Vec::new();
    for decl in decls {
        if matches!(&decl, ast::Decl::Use { lazy: true, .. }) {
            lazy_pending.push(decl);
        } else {
            eager_decls.push(decl);
        }
    }

    // Pass 1: resolve all eager (non-lazy) declarations.
    let mut result = resolve_imports_inner(
        eager_decls,
        base_dir,
        visited,
        diagnostics,
        build_target,
        false,
    )
    .0;

    // Pass 2: for each lazy pending import, check if any `<alias>-*` symbol
    // is referenced in the already-resolved declarations. Only load if so.
    for decl in lazy_pending {
        if let ast::Decl::Use { ref alias, .. } = decl {
            let prefix = format!("{}-", alias.as_deref().unwrap_or(""));
            if decls_reference_prefix(&result, &prefix) {
                // Symbol is used — load the module via the eager path.
                let loaded = resolve_imports_inner(
                    vec![decl],
                    base_dir,
                    visited,
                    diagnostics,
                    build_target,
                    false,
                )
                .0;
                // Prepend so lazy module decls precede the importer's own decls.
                let mut combined = loaded;
                combined.extend(result);
                result = combined;
            }
            // else: no reference to this module — skip loading entirely.
        }
    }

    result
}

/// Resolves imports and returns `(all_decls, exported_names)`.
///
/// `all_decls` contains every declaration that is in scope for the current module
/// (own declarations + all transitively-imported declarations). This is what the
/// module uses internally.
///
/// `exported_names` is the *public surface* of this module — the names that
/// consumers of this module can import. It includes:
///   - Every own declaration whose name does not start with `_`
///   - Any declaration brought in via `use re:"path" [names]` (re-exports)
///
/// When `for_export` is `false` (entry-point / top-level call), the second return
/// value is not meaningful and the caller should ignore it.
fn resolve_imports_inner(
    decls: Vec<ast::Decl>,
    base_dir: Option<&std::path::Path>,
    visited: &mut std::collections::HashSet<std::path::PathBuf>,
    diagnostics: &mut Vec<Diagnostic>,
    build_target: BuildTarget,
    for_export: bool,
) -> (Vec<ast::Decl>, Vec<String>) {
    let _ = for_export; // used by callers; kept for future use
    let mut result: Vec<ast::Decl> = Vec::new();
    // Track which names in `result` are part of the public export surface.
    let mut exported_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    for decl in decls {
        if let ast::Decl::Use {
            path,
            only,
            alias,
            predicate,
            alt_path,
            reexport,
            lazy: _,
            span,
        } = decl
        {
            // For conditional imports, select the right branch now.
            let path = if let Some(pred) = predicate {
                if build_target.eval(pred) {
                    path
                } else {
                    // alt_path is guaranteed to be Some when predicate is Some
                    // (enforced by the parser).
                    alt_path.unwrap_or(path)
                }
            } else {
                path
            };

            // ── Package registry resolution ───────────────────────────────────
            // `use "owner/repo"` (first component has no `.`) resolves through
            // the local package cache at ~/.ilo/pkgs/<owner>/<repo>/index.ilo.
            if crate::pkg::is_pkg_path(&path) {
                let resolved = crate::pkg::resolve_pkg_path(&path);
                match resolved {
                    Err(msg) => {
                        diagnostics.push(
                            Diagnostic::error(format!("use \"{path}\": {msg}"))
                                .with_code("ILO-P017")
                                .with_span(span, "imported here"),
                        );
                        continue;
                    }
                    Ok(pkg_file) => {
                        // Synthesise a Use decl for the resolved file path and
                        // re-use the same local-file resolution path below by
                        // substituting the resolved path into a local Use node.
                        let abs = pkg_file.to_string_lossy().into_owned();
                        let synthetic = ast::Decl::Use {
                            path: abs,
                            only: only.clone(),
                            alias: alias.clone(),
                            predicate: None,
                            alt_path: None,
                            reexport: false,
                            lazy: false,
                            span,
                        };
                        let mut sub = resolve_imports(
                            vec![synthetic],
                            None, // abs path, base_dir not needed
                            visited,
                            diagnostics,
                            build_target,
                        );
                        result.append(&mut sub);
                        continue;
                    }
                }
            }

            // ── Local file resolution ─────────────────────────────────────────
            let Some(dir) = base_dir else {
                // Package paths are handled above; inline code cannot use local files.
                // Absolute paths (synthesised by package resolution) are also handled above.
                if path.starts_with('/') {
                    // Absolute path synthesised by package resolution — resolve directly.
                    let canonical = match std::path::PathBuf::from(&path).canonicalize() {
                        Ok(c) => c,
                        Err(_) => {
                            diagnostics.push(
                                Diagnostic::error(format!("use \"{}\": file not found", path))
                                    .with_code("ILO-P017")
                                    .with_span(span, "imported here"),
                            );
                            continue;
                        }
                    };
                    // Proceed with the canonical path as if base_dir were its parent.
                    let imported_dir = canonical.parent().map(|p| p.to_path_buf());
                    let source = match std::fs::read_to_string(&canonical) {
                        Ok(s) => s,
                        Err(e) => {
                            diagnostics.push(
                                Diagnostic::error(format!("use \"{}\": {}", path, e))
                                    .with_code("ILO-P017")
                                    .with_span(span, "imported here"),
                            );
                            continue;
                        }
                    };
                    if visited.contains(&canonical) {
                        diagnostics.push(
                            Diagnostic::error(format!("use \"{}\": circular import", path))
                                .with_code("ILO-P018")
                                .with_span(span, "imported here"),
                        );
                        continue;
                    }
                    let tokens = match lexer::lex(&source) {
                        Ok(t) => t,
                        Err(e) => {
                            diagnostics.push(Diagnostic::from(&e));
                            continue;
                        }
                    };
                    let token_spans: Vec<(lexer::Token, ast::Span)> = tokens
                        .into_iter()
                        .map(|(t, r)| {
                            (
                                t,
                                ast::Span {
                                    start: r.start,
                                    end: r.end,
                                },
                            )
                        })
                        .collect();
                    let (mut imported_prog, parse_errors) = parser::parse(token_spans);
                    ast::resolve_aliases(&mut imported_prog);
                    ast::desugar_dot_var_index(&mut imported_prog);
                    for e in &parse_errors {
                        diagnostics.push(Diagnostic::from(e));
                    }
                    visited.insert(canonical.clone());
                    let imported_decls = resolve_imports(
                        imported_prog.declarations,
                        imported_dir.as_deref(),
                        visited,
                        diagnostics,
                        build_target,
                    );
                    visited.remove(&canonical);
                    let filtered =
                        apply_only_filter(imported_decls, &only, &path, span, diagnostics);
                    result.extend(filtered);
                    continue;
                }
                diagnostics.push(
                    Diagnostic::error(
                        "`use` requires a file path context — not supported in inline code",
                    )
                    .with_code("ILO-P017")
                    .with_span(span, "here"),
                );
                continue;
            };

            let file_path = dir.join(&path);
            let canonical = match file_path.canonicalize() {
                Ok(c) => c,
                Err(_) => {
                    diagnostics.push(
                        Diagnostic::error(format!("use \"{}\": file not found", path))
                            .with_code("ILO-P017")
                            .with_span(span, "imported here"),
                    );
                    continue;
                }
            };

            if visited.contains(&canonical) {
                diagnostics.push(
                    Diagnostic::error(format!("use \"{}\": circular import", path))
                        .with_code("ILO-P018")
                        .with_span(span, "imported here"),
                );
                continue;
            }

            let source = match std::fs::read_to_string(&canonical) {
                Ok(s) => s,
                Err(e) => {
                    diagnostics.push(
                        Diagnostic::error(format!("use \"{}\": {}", path, e))
                            .with_code("ILO-P017")
                            .with_span(span, "imported here"),
                    );
                    continue;
                }
            };

            let tokens = match lexer::lex(&source) {
                Ok(t) => t,
                Err(e) => {
                    diagnostics.push(Diagnostic::from(&e));
                    continue;
                }
            };

            let token_spans: Vec<(lexer::Token, ast::Span)> = tokens
                .into_iter()
                .map(|(t, r)| {
                    (
                        t,
                        ast::Span {
                            start: r.start,
                            end: r.end,
                        },
                    )
                })
                .collect();

            let (mut imported_prog, parse_errors) = parser::parse(token_spans);
            ast::resolve_aliases(&mut imported_prog);
            ast::desugar_dot_var_index(&mut imported_prog);
            for e in &parse_errors {
                diagnostics.push(Diagnostic::from(e));
            }

            visited.insert(canonical.clone());
            let imported_dir = canonical.parent();
            let (imported_decls, imported_exported) = resolve_imports_inner(
                imported_prog.declarations,
                imported_dir,
                visited,
                diagnostics,
                build_target,
                true,
            );
            visited.remove(&canonical);

            // Apply import filter based on form:
            //
            // - Flat (`use "path"`): public declarations (those in `imported_exported`) come
            //   through. Private (`_`-prefixed) names and non-re-exported transitive imports
            //   are excluded. (Legacy behaviour for pre-re-export files: if no re-export
            //   annotations exist in the imported file, `imported_exported` equals all own
            //   non-`_` decls, which is the same as before.)
            //
            // - Selective (`use "path" [name1 name2]`): only the listed names are imported,
            //   subject to the same export-surface constraint — each requested name must be
            //   in the imported file's public surface. `_`-prefixed names are always blocked.
            //
            // - Named-module (`use alias:"path"`): all public (non-`_`) names in the export
            //   surface are imported and prefixed with `alias-`.
            //
            // - Re-export (`use re:"path" [name1 name2]`): same as selective import, but the
            //   imported names are added to *this* module's export surface so consumers can
            //   further import them.
            let filtered = if let Some(ref names) = only {
                // Selective / re-export form — block private names and enforce export surface
                for name in names {
                    if name.starts_with('_') {
                        diagnostics.push(
                            Diagnostic::error(format!(
                                "use \"{}\": '{}' is module-private (names starting with `_` \
                                 are not exported)",
                                path, name
                            ))
                            .with_code("ILO-P019")
                            .with_span(span, "imported here"),
                        );
                        continue;
                    }
                    // Check the name is in the imported module's export surface
                    let found = imported_exported.iter().any(|e| e == name)
                        || imported_decls
                            .iter()
                            .any(|d| decl_name(d) == Some(name.as_str()));
                    if !found {
                        diagnostics.push(
                            Diagnostic::error(format!(
                                "use \"{}\": name '{}' not found in imported file",
                                path, name
                            ))
                            .with_code("ILO-P019")
                            .with_span(span, "imported here"),
                        );
                    }
                }
                let selected = imported_decls
                    .into_iter()
                    .filter(|d| {
                        decl_name(d)
                            .map(|n| !n.starts_with('_') && names.iter().any(|s| s == n))
                            .unwrap_or(false)
                    })
                    .collect::<Vec<_>>();
                // If this is a re-export, add selected names to this module's export surface
                if reexport {
                    for name in names {
                        if !name.starts_with('_') {
                            exported_names.insert(name.clone());
                        }
                    }
                }
                selected
            } else if let Some(ref pfx) = alias {
                // Named-module form: strip private, then rename public to `alias-name`.
                let public_decls: Vec<ast::Decl> = imported_decls
                    .into_iter()
                    .filter(|d| decl_name(d).map(|n| !n.starts_with('_')).unwrap_or(true))
                    .collect();
                apply_module_alias(public_decls, pfx)
            } else {
                // Flat import: everything (including private helpers) comes through.
                // Preserves backward-compatible behaviour — all transitively imported
                // declarations are visible in the importing file's scope.
                imported_decls
            };

            // Prepend imported declarations (so they appear before the importer's own decls)
            result.extend(filtered);
        } else {
            // Own declaration: add to result and mark as exported (if public)
            if let Some(name) = decl_name(&decl) {
                if !name.starts_with('_') {
                    exported_names.insert(name.to_string());
                }
            }
            result.push(decl);
        }
    }

    (result, exported_names.into_iter().collect())
}

/// Return `true` if any `Expr::Call` or `Expr::Ref` in `decls` has a name
/// that starts with `prefix`. Used by the lazy-import gate to decide whether
/// a deferred module is actually needed.
fn decls_reference_prefix(decls: &[ast::Decl], prefix: &str) -> bool {
    fn expr_refs(expr: &ast::Expr, prefix: &str) -> bool {
        match expr {
            ast::Expr::Call { function, args, .. } => {
                if function.starts_with(prefix) {
                    return true;
                }
                args.iter().any(|a| expr_refs(a, prefix))
            }
            ast::Expr::Ref(name) => name.starts_with(prefix),
            ast::Expr::BinOp { left, right, .. } => {
                expr_refs(left, prefix) || expr_refs(right, prefix)
            }
            ast::Expr::UnaryOp { operand, .. } => expr_refs(operand, prefix),
            ast::Expr::Ok(inner) | ast::Expr::Err(inner) => expr_refs(inner, prefix),
            ast::Expr::List(items) => items.iter().any(|e| expr_refs(e, prefix)),
            ast::Expr::Record { fields, .. } => fields.iter().any(|(_, e)| expr_refs(e, prefix)),
            ast::Expr::Field { object, .. } => expr_refs(object, prefix),
            ast::Expr::Index { object, .. } => expr_refs(object, prefix),
            ast::Expr::Match { subject, arms } => {
                subject
                    .as_deref()
                    .map(|s| expr_refs(s, prefix))
                    .unwrap_or(false)
                    || arms
                        .iter()
                        .any(|arm| arm.body.iter().any(|s| stmt_refs(&s.node, prefix)))
            }
            ast::Expr::NilCoalesce { value, default } => {
                expr_refs(value, prefix) || expr_refs(default, prefix)
            }
            ast::Expr::With { object, updates } => {
                expr_refs(object, prefix) || updates.iter().any(|(_, e)| expr_refs(e, prefix))
            }
            ast::Expr::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                expr_refs(condition, prefix)
                    || expr_refs(then_expr, prefix)
                    || expr_refs(else_expr, prefix)
            }
            ast::Expr::MakeClosure { captures, .. } => {
                captures.iter().any(|e| expr_refs(e, prefix))
            }
            ast::Expr::AnonRecord { fields, .. } => {
                fields.iter().any(|(_, e)| expr_refs(e, prefix))
            }
            ast::Expr::Literal(_) | ast::Expr::Todo(_) | ast::Expr::Panic(_) => false,
        }
    }

    fn stmt_refs(stmt: &ast::Stmt, prefix: &str) -> bool {
        match stmt {
            ast::Stmt::Let { value, .. } | ast::Stmt::Destructure { value, .. } => {
                expr_refs(value, prefix)
            }
            ast::Stmt::Expr(e) | ast::Stmt::Return(e) => expr_refs(e, prefix),
            ast::Stmt::Break(Some(e)) => expr_refs(e, prefix),
            ast::Stmt::Break(None) | ast::Stmt::Continue => false,
            ast::Stmt::Guard {
                condition,
                body,
                else_body,
                ..
            } => {
                expr_refs(condition, prefix)
                    || body.iter().any(|s| stmt_refs(&s.node, prefix))
                    || else_body
                        .as_deref()
                        .map(|b| b.iter().any(|s| stmt_refs(&s.node, prefix)))
                        .unwrap_or(false)
            }
            ast::Stmt::Match { subject, arms } => {
                subject
                    .as_ref()
                    .map(|s| expr_refs(s, prefix))
                    .unwrap_or(false)
                    || arms
                        .iter()
                        .any(|arm| arm.body.iter().any(|s| stmt_refs(&s.node, prefix)))
            }
            ast::Stmt::ForEach {
                collection, body, ..
            } => expr_refs(collection, prefix) || body.iter().any(|s| stmt_refs(&s.node, prefix)),
            ast::Stmt::ForRange {
                start, end, body, ..
            } => {
                expr_refs(start, prefix)
                    || expr_refs(end, prefix)
                    || body.iter().any(|s| stmt_refs(&s.node, prefix))
            }
            ast::Stmt::While { condition, body } => {
                expr_refs(condition, prefix) || body.iter().any(|s| stmt_refs(&s.node, prefix))
            }
            ast::Stmt::Defer { expr, .. } => expr_refs(expr, prefix),
        }
    }

    for decl in decls {
        let referenced = match decl {
            ast::Decl::Function { body, .. } => body.iter().any(|s| stmt_refs(&s.node, prefix)),
            ast::Decl::Tool { .. }
            | ast::Decl::TypeDef { .. }
            | ast::Decl::SumType { .. }
            | ast::Decl::Alias { .. }
            | ast::Decl::Use { .. }
            | ast::Decl::VersionPragma { .. }
            | ast::Decl::Error { .. } => false,
        };
        if referenced {
            return true;
        }
    }
    false
}

/// Rename all named declarations in `decls` by prepending `alias-` to their name.
/// Used by the `use alias:"path"` named-module import form.
fn apply_module_alias(decls: Vec<ast::Decl>, alias: &str) -> Vec<ast::Decl> {
    decls
        .into_iter()
        .map(|d| rename_decl_with_alias(d, alias))
        .collect()
}

/// Return a copy of `decl` with its name prefixed by `alias-`.
/// Declarations without a name (errors, `Use` nodes) pass through unchanged.
fn rename_decl_with_alias(decl: ast::Decl, alias: &str) -> ast::Decl {
    match decl {
        ast::Decl::Function {
            name,
            params,
            return_type,
            body,
            span,
            type_params,
            effect_set,
        } => ast::Decl::Function {
            name: format!("{}-{}", alias, name),
            params,
            return_type,
            body,
            span,
            type_params,
            effect_set,
        },
        ast::Decl::Tool {
            name,
            description,
            params,
            return_type,
            timeout,
            retry,
            span,
        } => ast::Decl::Tool {
            name: format!("{}-{}", alias, name),
            description,
            params,
            return_type,
            timeout,
            retry,
            span,
        },
        ast::Decl::TypeDef { name, fields, span } => ast::Decl::TypeDef {
            name: format!("{}-{}", alias, name),
            fields,
            span,
        },
        ast::Decl::Alias { name, target, span } => ast::Decl::Alias {
            name: format!("{}-{}", alias, name),
            target,
            span,
        },
        // Use and Error nodes have no name — pass through unchanged
        other => other,
    }
}
