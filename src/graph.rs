use std::collections::{HashMap, HashSet, BTreeSet, VecDeque};
use serde::Serialize;
use crate::ast::{self, Decl, Stmt, Expr, Type, Program};
use crate::builtins::Builtin;
use crate::codegen::fmt::{self, FmtMode};

// ── Output types ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ProgramGraph {
    pub functions: HashMap<String, FuncNode>,
    pub types: HashMap<String, TypeNode>,
}

#[derive(Debug, Serialize)]
pub struct FuncNode {
    pub sig: String,
    pub calls: BTreeSet<String>,
    pub called_by: BTreeSet<String>,
    pub types_used: BTreeSet<String>,
}

#[derive(Debug, Serialize)]
pub struct TypeNode {
    pub fields: Vec<(String, String)>,
    pub refs: BTreeSet<String>,
}

/// Subgraph output for --fn X queries.
#[derive(Debug, Serialize)]
pub struct FuncQuery {
    pub root: String,
    pub source: String,
    pub deps: HashMap<String, DepInfo>,
    pub types: HashMap<String, TypeInfo>,
}

#[derive(Debug, Serialize)]
pub struct DepInfo {
    pub sig: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TypeInfo {
    pub source: String,
}

/// Reverse query output.
#[derive(Debug, Serialize)]
pub struct ReverseQuery {
    pub function: String,
    pub sig: String,
    pub callers: Vec<CallerInfo>,
}

#[derive(Debug, Serialize)]
pub struct CallerInfo {
    pub name: String,
    pub sig: String,
}

/// Budget-aware subgraph.
#[derive(Debug, Serialize)]
pub struct BudgetQuery {
    pub root: String,
    pub source: String,
    pub deps: HashMap<String, DepInfo>,
    pub types: HashMap<String, TypeInfo>,
    pub budget: BudgetInfo,
}

#[derive(Debug, Serialize)]
pub struct BudgetInfo {
    pub used: usize,
    pub limit: usize,
    pub truncated: Vec<String>,
}

// ── AST walking helpers ─────────────────────────────────────────────────────

/// Walk an expression tree, collecting function calls and type references.
fn collect_calls(expr: &Expr, calls: &mut BTreeSet<String>, types: &mut BTreeSet<String>) {
    match expr {
        Expr::Call { function, args, .. } => {
            calls.insert(function.clone());
            for arg in args {
                collect_calls(arg, calls, types);
            }
        }
        Expr::Record { type_name, fields, .. } => {
            types.insert(type_name.clone());
            for (_, val) in fields {
                collect_calls(val, calls, types);
            }
        }
        Expr::Field { object, .. } => collect_calls(object, calls, types),
        Expr::Index { object, .. } => collect_calls(object, calls, types),
        Expr::BinOp { left, right, .. } => {
            collect_calls(left, calls, types);
            collect_calls(right, calls, types);
        }
        Expr::UnaryOp { operand, .. } => collect_calls(operand, calls, types),
        Expr::Ok(inner) | Expr::Err(inner) => collect_calls(inner, calls, types),
        Expr::List(items) => {
            for item in items {
                collect_calls(item, calls, types);
            }
        }
        Expr::NilCoalesce { value, default } => {
            collect_calls(value, calls, types);
            collect_calls(default, calls, types);
        }
        Expr::With { object, updates } => {
            collect_calls(object, calls, types);
            for (_, val) in updates {
                collect_calls(val, calls, types);
            }
        }
        Expr::Ternary { condition, then_expr, else_expr } => {
            collect_calls(condition, calls, types);
            collect_calls(then_expr, calls, types);
            collect_calls(else_expr, calls, types);
        }
        Expr::Match { subject, arms } => {
            if let Some(subj) = subject {
                collect_calls(subj, calls, types);
            }
            for arm in arms {
                for stmt in &arm.body {
                    collect_stmts(std::slice::from_ref(stmt), calls, types);
                }
            }
        }
        Expr::Literal(_) | Expr::Ref(_) => {}
    }
}

/// Walk statements, collecting function calls and type references.
fn collect_stmts(stmts: &[ast::Spanned<Stmt>], calls: &mut BTreeSet<String>, types: &mut BTreeSet<String>) {
    for spanned in stmts {
        match &spanned.node {
            Stmt::Let { value, .. } => collect_calls(value, calls, types),
            Stmt::Guard { condition, body, else_body, .. } => {
                collect_calls(condition, calls, types);
                collect_stmts(body, calls, types);
                if let Some(eb) = else_body {
                    collect_stmts(eb, calls, types);
                }
            }
            Stmt::Match { subject, arms } => {
                if let Some(subj) = subject {
                    collect_calls(subj, calls, types);
                }
                for arm in arms {
                    for stmt in &arm.body {
                        collect_stmts(std::slice::from_ref(stmt), calls, types);
                    }
                }
            }
            Stmt::ForEach { collection, body, .. } => {
                collect_calls(collection, calls, types);
                collect_stmts(body, calls, types);
            }
            Stmt::ForRange { start, end, body, .. } => {
                collect_calls(start, calls, types);
                collect_calls(end, calls, types);
                collect_stmts(body, calls, types);
            }
            Stmt::While { condition, body } => {
                collect_calls(condition, calls, types);
                collect_stmts(body, calls, types);
            }
            Stmt::Return(expr) => collect_calls(expr, calls, types),
            Stmt::Break(Some(expr)) => collect_calls(expr, calls, types),
            Stmt::Expr(expr) => collect_calls(expr, calls, types),
            Stmt::Destructure { value, .. } => collect_calls(value, calls, types),
            Stmt::Break(None) | Stmt::Continue => {}
        }
    }
}

/// Collect named type references from a Type node.
fn collect_type_refs(ty: &Type, refs: &mut BTreeSet<String>) {
    match ty {
        Type::Named(name) => {
            refs.insert(name.clone());
        }
        Type::List(inner) | Type::Optional(inner) => collect_type_refs(inner, refs),
        Type::Map(k, v) | Type::Result(k, v) => {
            collect_type_refs(k, refs);
            collect_type_refs(v, refs);
        }
        Type::Fn(params, ret) => {
            for p in params {
                collect_type_refs(p, refs);
            }
            collect_type_refs(ret, refs);
        }
        Type::Sum(_) | Type::Number | Type::Text | Type::Bool | Type::Any => {}
    }
}

// ── Signature formatting ────────────────────────────────────────────────────

/// Format a function signature string: `name param:type param:type>return_type`
fn format_sig(name: &str, params: &[ast::Param], return_type: &Type) -> String {
    let mut sig = name.to_string();
    for p in params {
        sig.push(' ');
        sig.push_str(&p.name);
        sig.push(':');
        sig.push_str(&fmt::type_str(&p.ty));
    }
    sig.push('>');
    sig.push_str(&fmt::type_str(return_type));
    sig
}

/// Rough token estimate: count whitespace-separated words.
fn estimate_tokens(text: &str) -> usize {
    text.split_whitespace().count()
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Build the full program graph from a parsed program.
pub fn build_graph(program: &Program) -> ProgramGraph {
    let mut functions: HashMap<String, FuncNode> = HashMap::new();
    let mut types: HashMap<String, TypeNode> = HashMap::new();

    // Collect all user-defined function names and type names for filtering.
    let user_fns: HashSet<String> = program
        .declarations
        .iter()
        .filter_map(|d| match d {
            Decl::Function { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();

    let user_types: HashSet<String> = program
        .declarations
        .iter()
        .filter_map(|d| match d {
            Decl::TypeDef { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();

    // 1. Process functions: collect forward edges.
    for decl in &program.declarations {
        if let Decl::Function { name, params, return_type, body, .. } = decl {
            let sig = format_sig(name, params, return_type);

            let mut raw_calls = BTreeSet::new();
            let mut raw_types = BTreeSet::new();

            // Collect calls and type refs from the body.
            collect_stmts(body, &mut raw_calls, &mut raw_types);

            // Also collect type refs from params and return type.
            for p in params {
                collect_type_refs(&p.ty, &mut raw_types);
            }
            collect_type_refs(return_type, &mut raw_types);

            // Filter to user-defined functions only (exclude builtins).
            let calls: BTreeSet<String> = raw_calls
                .into_iter()
                .filter(|c| user_fns.contains(c) && !Builtin::is_builtin(c))
                .collect();

            // Filter to user-defined types only.
            let types_used: BTreeSet<String> = raw_types
                .into_iter()
                .filter(|t| user_types.contains(t))
                .collect();

            functions.insert(name.clone(), FuncNode {
                sig,
                calls,
                called_by: BTreeSet::new(),
                types_used,
            });
        }
    }

    // 2. Process types: collect field references.
    for decl in &program.declarations {
        if let Decl::TypeDef { name, fields, .. } = decl {
            let mut refs = BTreeSet::new();
            let field_list: Vec<(String, String)> = fields
                .iter()
                .map(|f| {
                    collect_type_refs(&f.ty, &mut refs);
                    (f.name.clone(), fmt::type_str(&f.ty))
                })
                .collect();

            // Filter refs to user-defined types only.
            let refs: BTreeSet<String> = refs
                .into_iter()
                .filter(|r| user_types.contains(r))
                .collect();

            types.insert(name.clone(), TypeNode { fields: field_list, refs });
        }
    }

    // 3. Compute reverse edges (called_by).
    let forward: Vec<(String, BTreeSet<String>)> = functions
        .iter()
        .map(|(name, node)| (name.clone(), node.calls.clone()))
        .collect();

    for (caller, callees) in &forward {
        for callee in callees {
            if let Some(node) = functions.get_mut(callee) {
                node.called_by.insert(caller.clone());
            }
        }
    }

    ProgramGraph { functions, types }
}

/// Find a declaration by name.
fn find_decl<'a>(program: &'a Program, name: &str) -> Option<&'a Decl> {
    program.declarations.iter().find(|d| match d {
        Decl::Function { name: n, .. } | Decl::TypeDef { name: n, .. } => n == name,
        _ => false,
    })
}

/// Query: function + forward deps (signatures only, no source for deps).
pub fn query_fn(program: &Program, graph: &ProgramGraph, fn_name: &str) -> Option<FuncQuery> {
    let node = graph.functions.get(fn_name)?;
    let decl = find_decl(program, fn_name)?;
    let source = fmt::format_decl(decl, FmtMode::Dense);

    let mut deps = HashMap::new();
    for dep_name in &node.calls {
        if let Some(dep_node) = graph.functions.get(dep_name) {
            deps.insert(dep_name.clone(), DepInfo {
                sig: dep_node.sig.clone(),
                source: None,
            });
        }
    }

    // Collect types used by this function.
    let mut type_infos = HashMap::new();
    for type_name in &node.types_used {
        if let Some(decl) = find_decl(program, type_name) {
            type_infos.insert(type_name.clone(), TypeInfo {
                source: fmt::format_decl(decl, FmtMode::Dense),
            });
        }
    }

    Some(FuncQuery {
        root: fn_name.to_string(),
        source,
        deps,
        types: type_infos,
    })
}

/// Query: reverse callers of a function.
pub fn query_reverse(_program: &Program, graph: &ProgramGraph, fn_name: &str) -> Option<ReverseQuery> {
    let node = graph.functions.get(fn_name)?;

    let callers: Vec<CallerInfo> = node
        .called_by
        .iter()
        .filter_map(|caller_name| {
            graph.functions.get(caller_name).map(|caller_node| CallerInfo {
                name: caller_name.clone(),
                sig: caller_node.sig.clone(),
            })
        })
        .collect();

    Some(ReverseQuery {
        function: fn_name.to_string(),
        sig: node.sig.clone(),
        callers,
    })
}

/// Query: full subgraph (transitive deps, full source).
pub fn query_subgraph(program: &Program, graph: &ProgramGraph, fn_name: &str) -> Option<FuncQuery> {
    let _node = graph.functions.get(fn_name)?;
    let decl = find_decl(program, fn_name)?;
    let source = fmt::format_decl(decl, FmtMode::Dense);

    // BFS to collect all transitive deps.
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    visited.insert(fn_name.to_string());

    // Seed with direct calls.
    if let Some(node) = graph.functions.get(fn_name) {
        for dep in &node.calls {
            if visited.insert(dep.clone()) {
                queue.push_back(dep.clone());
            }
        }
    }

    while let Some(current) = queue.pop_front() {
        if let Some(node) = graph.functions.get(&current) {
            for dep in &node.calls {
                if visited.insert(dep.clone()) {
                    queue.push_back(dep.clone());
                }
            }
        }
    }

    // Build deps map (everything except root).
    let mut deps = HashMap::new();
    for name in &visited {
        if name == fn_name {
            continue;
        }
        if let Some(dep_node) = graph.functions.get(name) {
            let dep_source = find_decl(program, name)
                .map(|d| fmt::format_decl(d, FmtMode::Dense));
            deps.insert(name.clone(), DepInfo {
                sig: dep_node.sig.clone(),
                source: dep_source,
            });
        }
    }

    // Collect all types used by the root and all deps.
    let mut all_types = BTreeSet::new();
    for name in &visited {
        if let Some(node) = graph.functions.get(name) {
            for t in &node.types_used {
                all_types.insert(t.clone());
            }
        }
    }

    let mut type_infos = HashMap::new();
    for type_name in &all_types {
        if let Some(decl) = find_decl(program, type_name) {
            type_infos.insert(type_name.clone(), TypeInfo {
                source: fmt::format_decl(decl, FmtMode::Dense),
            });
        }
    }

    Some(FuncQuery {
        root: fn_name.to_string(),
        source,
        deps,
        types: type_infos,
    })
}

/// Query: budget-aware subgraph. Includes deps up to a token budget.
pub fn query_budget(
    program: &Program,
    graph: &ProgramGraph,
    fn_name: &str,
    budget: usize,
) -> Option<BudgetQuery> {
    let _node = graph.functions.get(fn_name)?;
    let decl = find_decl(program, fn_name)?;
    let source = fmt::format_decl(decl, FmtMode::Dense);
    let mut used = estimate_tokens(&source);

    // BFS, adding deps until budget is exhausted.
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    visited.insert(fn_name.to_string());

    if let Some(node) = graph.functions.get(fn_name) {
        for dep in &node.calls {
            if visited.insert(dep.clone()) {
                queue.push_back(dep.clone());
            }
        }
    }

    let mut deps = HashMap::new();
    let mut truncated = Vec::new();
    let mut all_types = BTreeSet::new();

    // Collect types from root.
    if let Some(node) = graph.functions.get(fn_name) {
        for t in &node.types_used {
            all_types.insert(t.clone());
        }
    }

    while let Some(current) = queue.pop_front() {
        let dep_source = find_decl(program, &current)
            .map(|d| fmt::format_decl(d, FmtMode::Dense));
        let cost = dep_source.as_ref().map(|s| estimate_tokens(s)).unwrap_or(0);

        if used + cost > budget {
            truncated.push(current);
            continue;
        }

        used += cost;

        if let Some(dep_node) = graph.functions.get(&current) {
            deps.insert(current.clone(), DepInfo {
                sig: dep_node.sig.clone(),
                source: dep_source,
            });

            for t in &dep_node.types_used {
                all_types.insert(t.clone());
            }

            for dep in &dep_node.calls {
                if visited.insert(dep.clone()) {
                    queue.push_back(dep.clone());
                }
            }
        }
    }

    // Add type sources (counted against budget too).
    let mut type_infos = HashMap::new();
    for type_name in &all_types {
        if let Some(td) = find_decl(program, type_name) {
            let ts = fmt::format_decl(td, FmtMode::Dense);
            let cost = estimate_tokens(&ts);
            if used + cost <= budget {
                used += cost;
                type_infos.insert(type_name.clone(), TypeInfo { source: ts });
            } else {
                truncated.push(type_name.clone());
            }
        }
    }

    Some(BudgetQuery {
        root: fn_name.to_string(),
        source,
        deps,
        types: type_infos,
        budget: BudgetInfo {
            used,
            limit: budget,
            truncated,
        },
    })
}

/// Emit DOT (graphviz) format for the program graph.
pub fn to_dot(graph: &ProgramGraph) -> String {
    let mut out = String::from("digraph ilo {\n  rankdir=LR;\n  node [shape=box];\n");

    // Sort for deterministic output.
    let mut func_names: Vec<&String> = graph.functions.keys().collect();
    func_names.sort();

    for name in &func_names {
        if let Some(node) = graph.functions.get(*name) {
            let mut callees: Vec<&String> = node.calls.iter().collect();
            callees.sort();
            for callee in callees {
                out.push_str(&format!("  \"{}\" -> \"{}\";\n", name, callee));
            }
            let mut type_refs: Vec<&String> = node.types_used.iter().collect();
            type_refs.sort();
            for tr in type_refs {
                out.push_str(&format!("  \"{}\" -> \"{}\" [style=dashed];\n", name, tr));
            }
        }
    }

    // Type nodes with a different shape.
    let mut type_names: Vec<&String> = graph.types.keys().collect();
    type_names.sort();
    for name in &type_names {
        out.push_str(&format!("  \"{}\" [shape=record];\n", name));
    }

    out.push_str("}\n");
    out
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;
    use crate::lexer;

    fn parse(src: &str) -> Program {
        let tokens = lexer::lex(src).unwrap();
        let token_spans: Vec<_> = tokens
            .into_iter()
            .map(|(t, r)| (t, ast::Span { start: r.start, end: r.end }))
            .collect();
        let (mut prog, _) = parser::parse(token_spans);
        ast::resolve_aliases(&mut prog);
        prog
    }

    #[test]
    fn test_basic_call_graph() {
        let prog = parse("add a:n b:n>n;+a b\ndbl x:n>n;add x x");
        let graph = build_graph(&prog);
        assert!(graph.functions["dbl"].calls.contains("add"));
        assert!(graph.functions["add"].called_by.contains("dbl"));
    }

    #[test]
    fn test_type_refs() {
        let prog = parse("type pt{x:n;y:n}\ndist p:pt>n;+p.x p.y");
        let graph = build_graph(&prog);
        assert!(graph.functions["dist"].types_used.contains("pt"));
    }

    #[test]
    fn test_query_fn() {
        let prog = parse("add a:n b:n>n;+a b\ndbl x:n>n;add x x\nquad x:n>n;dbl dbl x");
        let graph = build_graph(&prog);
        let q = query_fn(&prog, &graph, "quad").unwrap();
        assert_eq!(q.root, "quad");
        assert!(q.deps.contains_key("dbl"));
    }

    #[test]
    fn test_subgraph_transitive() {
        let prog = parse("add a:n b:n>n;+a b\ndbl x:n>n;add x x\nquad x:n>n;dbl dbl x");
        let graph = build_graph(&prog);
        let q = query_subgraph(&prog, &graph, "quad").unwrap();
        assert!(q.deps.contains_key("dbl"));
        assert!(q.deps.contains_key("add")); // transitive dep
    }

    #[test]
    fn test_reverse_query() {
        let prog = parse("add a:n b:n>n;+a b\ndbl x:n>n;add x x");
        let graph = build_graph(&prog);
        let r = query_reverse(&prog, &graph, "add").unwrap();
        assert_eq!(r.callers.len(), 1);
        assert_eq!(r.callers[0].name, "dbl");
    }

    #[test]
    fn test_dot_output() {
        let prog = parse("add a:n b:n>n;+a b\ndbl x:n>n;add x x");
        let graph = build_graph(&prog);
        let dot = to_dot(&graph);
        assert!(dot.contains("digraph"));
        assert!(dot.contains("dbl -> add") || dot.contains("\"dbl\" -> \"add\""));
    }

    #[test]
    fn test_type_node_fields() {
        let prog = parse("type pt{x:n;y:n}");
        let graph = build_graph(&prog);
        let tn = &graph.types["pt"];
        assert_eq!(tn.fields.len(), 2);
        assert_eq!(tn.fields[0], ("x".to_string(), "n".to_string()));
        assert_eq!(tn.fields[1], ("y".to_string(), "n".to_string()));
    }

    #[test]
    fn test_type_refs_between_types() {
        let prog = parse("type pt{x:n;y:n}\ntype line{start:pt;end:pt}");
        let graph = build_graph(&prog);
        assert!(graph.types["line"].refs.contains("pt"));
    }

    #[test]
    fn test_builtin_calls_excluded() {
        let prog = parse("f xs:L n>n;len xs");
        let graph = build_graph(&prog);
        // `len` is a builtin, should not appear in calls
        assert!(graph.functions["f"].calls.is_empty());
    }

    #[test]
    fn test_query_nonexistent() {
        let prog = parse("f x:n>n;x");
        let graph = build_graph(&prog);
        assert!(query_fn(&prog, &graph, "nope").is_none());
        assert!(query_reverse(&prog, &graph, "nope").is_none());
        assert!(query_subgraph(&prog, &graph, "nope").is_none());
        assert!(query_budget(&prog, &graph, "nope", 100).is_none());
    }

    #[test]
    fn test_budget_query() {
        let prog = parse("add a:n b:n>n;+a b\ndbl x:n>n;add x x\nquad x:n>n;dbl dbl x");
        let graph = build_graph(&prog);
        // Large budget should include everything.
        let q = query_budget(&prog, &graph, "quad", 10000).unwrap();
        assert!(q.deps.contains_key("dbl"));
        assert!(q.deps.contains_key("add"));
        assert_eq!(q.budget.limit, 10000);
        assert!(q.budget.truncated.is_empty());
    }

    #[test]
    fn test_budget_truncation() {
        let prog = parse("add a:n b:n>n;+a b\ndbl x:n>n;add x x\nquad x:n>n;dbl dbl x");
        let graph = build_graph(&prog);
        // Tiny budget: only root fits, deps get truncated.
        let q = query_budget(&prog, &graph, "quad", 3).unwrap();
        assert!(!q.budget.truncated.is_empty());
    }

    #[test]
    fn test_sig_format() {
        let prog = parse("prc ord:order>R order t;ord");
        let graph = build_graph(&prog);
        let sig = &graph.functions["prc"].sig;
        assert!(sig.starts_with("prc ord:order>R order t"));
    }

    #[test]
    fn test_subgraph_includes_types() {
        let prog = parse("type pt{x:n;y:n}\nmk>pt;pt x:1 y:2");
        let graph = build_graph(&prog);
        let q = query_subgraph(&prog, &graph, "mk").unwrap();
        assert!(q.types.contains_key("pt"));
    }

    #[test]
    fn test_graph_json_serializable() {
        let prog = parse("add a:n b:n>n;+a b\ndbl x:n>n;add x x");
        let graph = build_graph(&prog);
        let json = serde_json::to_string(&graph);
        assert!(json.is_ok());
    }
}
