//! Semantic analysis — name resolution, light type analysis, ownership checks.

use crate::ast::*;
use crate::diagnostics::{Diagnostic, Diagnostics};
use indexmap::IndexMap;
use std::collections::HashSet;

/// Analyzed program ready for IR lowering.
#[derive(Debug, Clone)]
pub struct AnalyzedProgram {
    pub program: Program,
    pub structs: IndexMap<String, StructDef>,
    pub enums: IndexMap<String, EnumDef>,
    pub functions: IndexMap<String, Function>,
    pub traits: IndexMap<String, TraitDef>,
    pub impls: Vec<ImplBlock>,
}

/// Run semantic analysis.
pub fn analyze(program: &Program) -> anyhow::Result<(AnalyzedProgram, Diagnostics)> {
    let mut diags = Diagnostics::new();
    let mut structs = IndexMap::new();
    let mut enums = IndexMap::new();
    let mut functions = IndexMap::new();
    let mut traits = IndexMap::new();
    let mut impls = Vec::new();

    for item in &program.items {
        match item {
            Item::Struct(s) => {
                if structs.contains_key(&s.name) {
                    diags.push(
                        Diagnostic::error("E0401", format!("duplicate struct `{}`", s.name))
                            .at(&program.filename, s.span.line, s.span.column),
                    );
                }
                structs.insert(s.name.clone(), s.clone());
            }
            Item::Enum(e) => {
                if enums.contains_key(&e.name) {
                    diags.push(
                        Diagnostic::error("E0401", format!("duplicate enum `{}`", e.name))
                            .at(&program.filename, e.span.line, e.span.column),
                    );
                }
                enums.insert(e.name.clone(), e.clone());
            }
            Item::Function(f) => {
                if functions.contains_key(&f.name) {
                    diags.push(
                        Diagnostic::error("E0401", format!("duplicate function `{}`", f.name))
                            .at(&program.filename, f.span.line, f.span.column),
                    );
                }
                functions.insert(f.name.clone(), f.clone());
            }
            Item::Trait(t) => {
                if traits.contains_key(&t.name) {
                    diags.push(
                        Diagnostic::error("E0401", format!("duplicate trait `{}`", t.name))
                            .at(&program.filename, t.span.line, t.span.column),
                    );
                }
                traits.insert(t.name.clone(), t.clone());
            }
            Item::Impl(i) => {
                impls.push(i.clone());
            }
            Item::Const(_) | Item::Use(_) => {}
        }
    }

    // Expand inherent impl methods into free functions with explicit Self type.
    for impl_block in &impls {
        if impl_block.trait_name.is_some() {
            continue; // trait impls handled in IR as instances
        }
        let self_ty = impl_block.for_type.clone();
        for method in &impl_block.methods {
            let mut f = method.clone();
            f.params = rewrite_self_params(&f.params, &self_ty);
            f.return_type = f
                .return_type
                .as_ref()
                .map(|t| rewrite_self_type(t, &self_ty));
            // Avoid clobbering free functions; qualify if needed
            if functions.contains_key(&f.name) {
                let type_name = type_name_hint(&self_ty);
                f.name = format!("{}_{}", type_name, f.name);
            }
            functions.insert(f.name.clone(), f);
        }
    }

    for f in functions.values() {
        check_function(f, &program.filename, &structs, &enums, &functions, &mut diags);
    }

    for item in &program.items {
        match item {
            Item::Function(f) => {
                scan_unsupported_in_block(&f.body, &program.filename, &mut diags);
                scan_unsupported_types_in_fn(f, &program.filename, &mut diags);
            }
            Item::Impl(i) => {
                for m in &i.methods {
                    scan_unsupported_in_block(&m.body, &program.filename, &mut diags);
                    scan_unsupported_types_in_fn(m, &program.filename, &mut diags);
                }
            }
            Item::Trait(t) => {
                for m in &t.methods {
                    if let Some(body) = &m.default_body {
                        scan_unsupported_in_block(body, &program.filename, &mut diags);
                    }
                }
            }
            Item::Use(u) => {
                if is_unsupported_path(&u.path) {
                    diags.push(
                        Diagnostic::unsupported(
                            &format!("stdlib import `{}`", u.path),
                            &program.filename,
                            u.span.line,
                            u.span.column,
                        )
                        .note("Interior mutability, reference counting, and threading are outside the current semantic model."),
                    );
                }
            }
            _ => {}
        }
    }

    Ok((
        AnalyzedProgram {
            program: program.clone(),
            structs,
            enums,
            functions,
            traits,
            impls,
        },
        diags,
    ))
}

fn type_name_hint(ty: &Type) -> String {
    match ty {
        Type::Named(n, _) => n.clone(),
        Type::Generic(n) => n.clone(),
        Type::SelfType => "Self".into(),
        _ => "T".into(),
    }
}

fn rewrite_self_params(params: &[Param], self_ty: &Type) -> Vec<Param> {
    params
        .iter()
        .map(|p| {
            if p.is_self {
                let ty = rewrite_self_type(&p.ty, self_ty);
                Param {
                    name: "self_".into(),
                    ty,
                    is_mut: p.is_mut,
                    by_ref: p.by_ref,
                    is_self: false,
                    span: p.span,
                }
            } else {
                let mut p = p.clone();
                p.ty = rewrite_self_type(&p.ty, self_ty);
                p
            }
        })
        .collect()
}

fn rewrite_self_type(ty: &Type, self_ty: &Type) -> Type {
    match ty {
        Type::SelfType => self_ty.clone(),
        Type::Ref { is_mut, inner } => Type::Ref {
            is_mut: *is_mut,
            inner: Box::new(rewrite_self_type(inner, self_ty)),
        },
        Type::Vec(t) => Type::Vec(Box::new(rewrite_self_type(t, self_ty))),
        Type::Option(t) => Type::Option(Box::new(rewrite_self_type(t, self_ty))),
        Type::Result(a, b) => Type::Result(
            Box::new(rewrite_self_type(a, self_ty)),
            Box::new(rewrite_self_type(b, self_ty)),
        ),
        Type::Tuple(ts) => Type::Tuple(ts.iter().map(|t| rewrite_self_type(t, self_ty)).collect()),
        Type::Named(n, args) => Type::Named(
            n.clone(),
            args.iter().map(|t| rewrite_self_type(t, self_ty)).collect(),
        ),
        Type::Array(t) => Type::Array(Box::new(rewrite_self_type(t, self_ty))),
        Type::Fun { params, ret } => Type::Fun {
            params: params
                .iter()
                .map(|t| rewrite_self_type(t, self_ty))
                .collect(),
            ret: Box::new(rewrite_self_type(ret, self_ty)),
        },
        other => other.clone(),
    }
}

fn check_function(
    f: &Function,
    filename: &str,
    _structs: &IndexMap<String, StructDef>,
    _enums: &IndexMap<String, EnumDef>,
    functions: &IndexMap<String, Function>,
    diags: &mut Diagnostics,
) {
    let mut scope: HashSet<String> = HashSet::new();
    for p in &f.params {
        scope.insert(p.name.clone());
    }
    for name in ["Some", "None", "Ok", "Err", "true", "false", "Vec", "String"] {
        scope.insert(name.into());
    }
    for name in functions.keys() {
        scope.insert(name.clone());
    }
    check_block(&f.body, filename, &mut scope, diags);
}

fn check_block(block: &Block, filename: &str, scope: &mut HashSet<String>, diags: &mut Diagnostics) {
    let mut local = scope.clone();
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let { name, value, .. } => {
                if let Some(v) = value {
                    check_expr(v, filename, &local, diags);
                }
                local.insert(name.clone());
            }
            Stmt::Expr(e) | Stmt::Return(Some(e), _) => check_expr(e, filename, &local, diags),
            Stmt::Return(None, _) | Stmt::Break(_) | Stmt::Continue(_) => {}
        }
    }
    if let Some(e) = &block.expr {
        check_expr(e, filename, &local, diags);
    }
}

fn check_expr(expr: &Expr, filename: &str, scope: &HashSet<String>, diags: &mut Diagnostics) {
    match expr {
        Expr::Unsupported { description, span } => {
            diags.push(Diagnostic::unsupported(
                description,
                filename,
                span.line,
                span.column,
            ));
        }
        Expr::Path(_, _) | Expr::Lit(_, _) => {}
        Expr::Field { base, .. } => check_expr(base, filename, scope, diags),
        Expr::Index { base, index, .. } => {
            check_expr(base, filename, scope, diags);
            check_expr(index, filename, scope, diags);
        }
        Expr::Call { func, args, .. } => {
            check_expr(func, filename, scope, diags);
            for a in args {
                check_expr(a, filename, scope, diags);
            }
        }
        Expr::MethodCall {
            receiver, args, ..
        } => {
            check_expr(receiver, filename, scope, diags);
            for a in args {
                check_expr(a, filename, scope, diags);
            }
        }
        Expr::Binary { left, right, .. } => {
            check_expr(left, filename, scope, diags);
            check_expr(right, filename, scope, diags);
        }
        Expr::Unary { expr, .. } | Expr::Deref { expr, .. } | Expr::Reference { expr, .. } => {
            check_expr(expr, filename, scope, diags)
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => {
            check_expr(cond, filename, scope, diags);
            let mut s = scope.clone();
            check_block(then_branch, filename, &mut s, diags);
            if let Some(e) = else_branch {
                check_expr(e, filename, scope, diags);
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            check_expr(scrutinee, filename, scope, diags);
            for arm in arms {
                let mut s = scope.clone();
                bind_pattern(&arm.pattern, &mut s);
                if let Some(g) = &arm.guard {
                    check_expr(g, filename, &s, diags);
                }
                check_expr(&arm.body, filename, &s, diags);
            }
        }
        Expr::Block(b) => {
            let mut s = scope.clone();
            check_block(b, filename, &mut s, diags);
        }
        Expr::Closure { params, body, .. } => {
            let mut s = scope.clone();
            for p in params {
                s.insert(p.clone());
            }
            check_expr(body, filename, &s, diags);
        }
        Expr::Tuple(es, _) | Expr::Array(es, _) => {
            for e in es {
                check_expr(e, filename, scope, diags);
            }
        }
        Expr::Struct { fields, .. } => {
            for (_, e) in fields {
                check_expr(e, filename, scope, diags);
            }
        }
        Expr::Assign { target, value, .. } | Expr::AssignOp { target, value, .. } => {
            check_expr(target, filename, scope, diags);
            check_expr(value, filename, scope, diags);
        }
        Expr::For { iter, body, pat, .. } => {
            check_expr(iter, filename, scope, diags);
            let mut s = scope.clone();
            s.insert(pat.clone());
            check_block(body, filename, &mut s, diags);
        }
        Expr::While { cond, body, .. } => {
            check_expr(cond, filename, scope, diags);
            let mut s = scope.clone();
            check_block(body, filename, &mut s, diags);
        }
        Expr::Loop { body, .. } => {
            let mut s = scope.clone();
            check_block(body, filename, &mut s, diags);
        }
        Expr::Return(Some(e), _) | Expr::Try(e, _) => check_expr(e, filename, scope, diags),
        Expr::Return(None, _) => {}
        Expr::Cast { expr, .. } => check_expr(expr, filename, scope, diags),
    }
}

fn bind_pattern(pat: &Pattern, scope: &mut HashSet<String>) {
    match pat {
        Pattern::Ident(n) => {
            scope.insert(n.clone());
        }
        Pattern::Tuple(ps) | Pattern::TupleStruct { elems: ps, .. } => {
            for p in ps {
                bind_pattern(p, scope);
            }
        }
        Pattern::Variant { elems, .. } => {
            for p in elems {
                bind_pattern(p, scope);
            }
        }
        Pattern::Struct { fields, .. } => {
            for (_, p) in fields {
                bind_pattern(p, scope);
            }
        }
        Pattern::Ref { inner, .. } => bind_pattern(inner, scope),
        Pattern::Wildcard | Pattern::Lit(_) | Pattern::Path(_) => {}
    }
}

fn scan_unsupported_in_block(block: &Block, filename: &str, diags: &mut Diagnostics) {
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let { value: Some(e), .. } | Stmt::Expr(e) | Stmt::Return(Some(e), _) => {
                scan_unsupported_expr(e, filename, diags);
            }
            _ => {}
        }
    }
    if let Some(e) = &block.expr {
        scan_unsupported_expr(e, filename, diags);
    }
}

fn scan_unsupported_expr(expr: &Expr, filename: &str, diags: &mut Diagnostics) {
    let scope = HashSet::new();
    check_expr(expr, filename, &scope, diags);
    walk_paths(expr, &mut |path, span| {
        if is_unsupported_path(path) {
            diags.push(
                Diagnostic::unsupported(
                    &format!("`{path}`"),
                    filename,
                    span.line,
                    span.column,
                )
                .note("Interior mutability, reference counting, and threading lack a sound erasure today."),
            );
        }
    });
}

fn scan_unsupported_types_in_fn(f: &Function, filename: &str, diags: &mut Diagnostics) {
    for p in &f.params {
        scan_unsupported_type(&p.ty, filename, f.span, diags);
    }
    if let Some(ty) = &f.return_type {
        scan_unsupported_type(ty, filename, f.span, diags);
    }
}

fn scan_unsupported_type(ty: &Type, filename: &str, span: Span, diags: &mut Diagnostics) {
    match ty {
        Type::Named(name, args) => {
            if is_unsupported_path(name)
                || matches!(
                    name.as_str(),
                    "RefCell" | "Rc" | "Arc" | "Cell" | "Mutex" | "RwLock"
                )
            {
                diags.push(
                    Diagnostic::unsupported(
                        &format!("type `{name}`"),
                        filename,
                        span.line,
                        span.column,
                    )
                    .note("Smart pointers and interior mutability are not erased by the current model."),
                );
            }
            if name.contains("dyn ") || name.starts_with("dyn") {
                diags.push(
                    Diagnostic::unsupported(
                        "trait objects (`dyn Trait`)",
                        filename,
                        span.line,
                        span.column,
                    )
                    .note("Dynamic dispatch needs a different representation; deferred."),
                );
            }
            for a in args {
                scan_unsupported_type(a, filename, span, diags);
            }
        }
        Type::Tuple(ts) => {
            for t in ts {
                scan_unsupported_type(t, filename, span, diags);
            }
        }
        Type::Ref { inner, .. } | Type::Array(inner) | Type::Vec(inner) | Type::Option(inner) => {
            scan_unsupported_type(inner, filename, span, diags);
        }
        Type::Result(a, b) => {
            scan_unsupported_type(a, filename, span, diags);
            scan_unsupported_type(b, filename, span, diags);
        }
        Type::Fun { params, ret } => {
            for t in params {
                scan_unsupported_type(t, filename, span, diags);
            }
            scan_unsupported_type(ret, filename, span, diags);
        }
        Type::Path(parts) => {
            let name = parts.join("::");
            if is_unsupported_path(&name) {
                diags.push(
                    Diagnostic::unsupported(
                        &format!("type `{name}`"),
                        filename,
                        span.line,
                        span.column,
                    )
                    .note("Smart pointers and interior mutability are not erased by the current model."),
                );
            }
        }
        _ => {}
    }
}

fn is_unsupported_path(path: &str) -> bool {
    let p = path.replace(' ', "");
    p.contains("RefCell")
        || p.contains("std::rc::Rc")
        || p.ends_with("::Rc")
        || p == "Rc"
        || p.contains("std::sync::Arc")
        || p.ends_with("::Arc")
        || p == "Arc"
        || p.contains("std::cell::Cell")
        || p.contains("std::thread")
        || p.contains("thread::spawn")
        || p.contains("std::sync::Mutex")
        || p.contains("std::sync::RwLock")
}

fn walk_paths(expr: &Expr, f: &mut impl FnMut(&str, Span)) {
    match expr {
        Expr::Path(p, span) => f(p, *span),
        Expr::Field { base, .. } | Expr::Deref { expr: base, .. } | Expr::Reference { expr: base, .. }
        | Expr::Unary { expr: base, .. } | Expr::Try(base, _) | Expr::Cast { expr: base, .. }
        | Expr::Return(Some(base), _) => walk_paths(base, f),
        Expr::Index { base, index, .. } | Expr::Binary { left: base, right: index, .. }
        | Expr::Assign { target: base, value: index, .. }
        | Expr::AssignOp { target: base, value: index, .. } => {
            walk_paths(base, f);
            walk_paths(index, f);
        }
        Expr::Call { func, args, .. } => {
            walk_paths(func, f);
            for a in args {
                walk_paths(a, f);
            }
        }
        Expr::MethodCall { receiver, args, .. } => {
            walk_paths(receiver, f);
            for a in args {
                walk_paths(a, f);
            }
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => {
            walk_paths(cond, f);
            for s in &then_branch.stmts {
                if let Stmt::Expr(e) | Stmt::Let { value: Some(e), .. } | Stmt::Return(Some(e), _) = s {
                    walk_paths(e, f);
                }
            }
            if let Some(e) = &then_branch.expr {
                walk_paths(e, f);
            }
            if let Some(e) = else_branch {
                walk_paths(e, f);
            }
        }
        Expr::Match { scrutinee, arms, .. } => {
            walk_paths(scrutinee, f);
            for arm in arms {
                walk_paths(&arm.body, f);
            }
        }
        Expr::Block(b) => {
            for s in &b.stmts {
                if let Stmt::Expr(e) | Stmt::Let { value: Some(e), .. } | Stmt::Return(Some(e), _) = s {
                    walk_paths(e, f);
                }
            }
            if let Some(e) = &b.expr {
                walk_paths(e, f);
            }
        }
        Expr::Closure { body, .. } => walk_paths(body, f),
        Expr::Tuple(es, _) | Expr::Array(es, _) => {
            for e in es {
                walk_paths(e, f);
            }
        }
        Expr::Struct { fields, .. } => {
            for (_, e) in fields {
                walk_paths(e, f);
            }
        }
        Expr::For { iter, body, .. } => {
            walk_paths(iter, f);
            for s in &body.stmts {
                if let Stmt::Expr(e) | Stmt::Let { value: Some(e), .. } | Stmt::Return(Some(e), _) = s {
                    walk_paths(e, f);
                }
            }
        }
        Expr::While { cond, body, .. } => {
            walk_paths(cond, f);
            for s in &body.stmts {
                if let Stmt::Expr(e) | Stmt::Let { value: Some(e), .. } | Stmt::Return(Some(e), _) = s {
                    walk_paths(e, f);
                }
            }
        }
        _ => {}
    }
}
