//! Lower analyzed Rust programs into translation IR.

use crate::ast::*;
use crate::diagnostics::{Diagnostic, Diagnostics};
use crate::ir::{self, Module, Ty};
use crate::semantic::AnalyzedProgram;
use crate::translate::patterns;
use heck::{ToLowerCamelCase, ToPascalCase};
use indexmap::IndexMap;

pub fn to_ir(analyzed: &AnalyzedProgram) -> anyhow::Result<(Module, Diagnostics)> {
    let mut diags = Diagnostics::new();
    let module_name = module_name_from_file(&analyzed.program.filename);
    let mut module = Module::new(&module_name);

    // Data types first
    for (name, s) in &analyzed.structs {
        module.decls.push(ir::Decl::Data(struct_to_data(s)));
        let _ = name;
    }
    for (name, e) in &analyzed.enums {
        module.decls.push(ir::Decl::Data(enum_to_data(e)));
        let _ = name;
    }

    // Constants
    for item in &analyzed.program.items {
        if let Item::Const(c) = item {
            let mut ctx = LowerCtx::new(&analyzed.program.filename);
            let value = lower_expr(&c.value, &mut ctx);
            module.decls.push(ir::Decl::Const {
                name: camel(&c.name),
                ty: lower_type(&c.ty),
                value,
            });
            diags.append(ctx.diags);
        }
    }

    // Functions
    for f in analyzed.functions.values() {
            let (func, notes, fdiags) = lower_function(f, &analyzed.program.filename);
        diags.append(fdiags);
        module.explanations.push(ir::TransNote {
            function: f.name.clone(),
            detected: notes.detected.clone(),
            translation: notes.translation.clone(),
            generated_summary: notes.summary.clone(),
        });
        module.decls.push(ir::Decl::Func(func));
    }

    Ok((module, diags))
}

fn module_name_from_file(filename: &str) -> String {
    let stem = std::path::Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Main");
    // Haskell module names: PascalCase, no hyphens/underscores
    stem.to_pascal_case().replace('-', "")
}

/// Convert snake_case Rust names to camelCase Haskell names.
pub fn camel(name: &str) -> String {
    if name == "_" {
        return "_".into();
    }
    // Keep type/constructor names PascalCase
    if name.chars().next().is_some_and(|c| c.is_uppercase()) {
        return name.to_string();
    }
    name.to_lower_camel_case()
}

fn struct_to_data(s: &StructDef) -> ir::DataType {
    ir::DataType {
        name: s.name.clone(),
        generics: s.generics.clone(),
        constructors: vec![ir::Constructor {
            name: s.name.clone(),
            fields: s
                .fields
                .iter()
                .map(|f| ir::DataField {
                    name: Some(camel(&f.name)),
                    ty: lower_type(&f.ty),
                })
                .collect(),
        }],
        is_record: true,
    }
}

fn enum_to_data(e: &EnumDef) -> ir::DataType {
    ir::DataType {
        name: e.name.clone(),
        generics: e.generics.clone(),
        constructors: e
            .variants
            .iter()
            .map(|v| ir::Constructor {
                name: v.name.clone(),
                fields: match &v.fields {
                    VariantFields::Unit => vec![],
                    VariantFields::Tuple(tys) => tys
                        .iter()
                        .map(|t| ir::DataField {
                            name: None,
                            ty: lower_type(t),
                        })
                        .collect(),
                    VariantFields::Struct(fs) => fs
                        .iter()
                        .map(|f| ir::DataField {
                            name: Some(camel(&f.name)),
                            ty: lower_type(&f.ty),
                        })
                        .collect(),
                },
            })
            .collect(),
        is_record: e
            .variants
            .iter()
            .any(|v| matches!(v.fields, VariantFields::Struct(_))),
    }
}

pub fn lower_type(ty: &Type) -> Ty {
    match ty {
        Type::Unit => Ty::Unit,
        Type::Named(n) => match n.as_str() {
            "i8" | "i16" | "i32" | "i64" | "i128" | "isize" => Ty::Int,
            "u8" | "u16" | "u32" => Ty::Word32,
            "u64" | "u128" | "usize" => Ty::Word64,
            "f32" => Ty::Float,
            "f64" => Ty::Double,
            "bool" => Ty::Bool,
            "char" => Ty::Char,
            "String" | "str" => Ty::String,
            other => Ty::Named(other.to_string(), vec![]),
        },
        Type::Path(parts) => {
            let n = parts.last().map(|s| s.as_str()).unwrap_or("()");
            lower_type(&Type::Named(n.to_string()))
        }
        Type::Ref { inner, .. } => lower_type(inner), // refs erased
        Type::Tuple(ts) => Ty::Tuple(ts.iter().map(lower_type).collect()),
        Type::Array(t) | Type::Vec(t) => Ty::List(Box::new(lower_type(t))),
        Type::Option(t) => Ty::Maybe(Box::new(lower_type(t))),
        Type::Result(ok, err) => {
            // Rust Result<T,E> → Haskell Either e a
            Ty::Either(Box::new(lower_type(err)), Box::new(lower_type(ok)))
        }
        Type::Fun { params, ret } => Ty::Fun(
            params.iter().map(lower_type).collect(),
            Box::new(lower_type(ret)),
        ),
        Type::Generic(g) => Ty::Var(g.clone()),
        Type::Infer => Ty::Var("a".into()),
    }
}

#[derive(Debug, Clone, Default)]
pub struct TransNotes {
    pub detected: Vec<String>,
    pub translation: Vec<String>,
    pub summary: String,
}

struct LowerCtx {
    filename: String,
    diags: Diagnostics,
    /// Local mutable bindings we're tracking as SSA-ish renames
    #[allow(dead_code)]
    mut_versions: IndexMap<String, usize>,
}

impl LowerCtx {
    fn new(filename: &str) -> Self {
        Self {
            filename: filename.to_string(),
            diags: Diagnostics::new(),
            mut_versions: IndexMap::new(),
        }
    }
}

fn lower_function(f: &Function, filename: &str) -> (ir::Func, TransNotes, Diagnostics) {
    let mut ctx = LowerCtx::new(filename);
    let mut notes = TransNotes::default();

    let params: Vec<(String, Ty)> = f
        .params
        .iter()
        .map(|p| {
            // Strip refs from param types for Haskell
            let ty = lower_type(&p.ty);
            (camel(&p.name), ty)
        })
        .collect();

    let return_ty = f
        .return_type
        .as_ref()
        .map(lower_type)
        .unwrap_or(Ty::Unit);

    // Try pattern recognition on the whole body first
    if let Some(body) = try_pattern_body(&f.body, &mut notes, &mut ctx) {
        let func = ir::Func {
            name: camel(&f.name),
            params,
            return_ty,
            body,
            notes: notes.detected.clone(),
        };
        let diags = std::mem::take(&mut ctx.diags);
        return (func, notes, diags);
    }

    let body = lower_block(&f.body, &mut ctx, &mut notes);
    // Infer IO () for main and functions that produce IO actions
    let return_ty = if f.name == "main" || is_io_exp(&body) {
        Ty::Named("IO".into(), vec![Ty::Unit])
    } else {
        return_ty
    };
    // Wrap pure main in `return (...)`. Leave applications alone — they may be IO.
    let body = if f.name == "main" && !is_io_exp(&body) {
        match &body {
            ir::Exp::App(_, _) => body,
            _ => ir::Exp::Pure(Box::new(body)),
        }
    } else {
        body
    };
    notes.summary = format!("{} ...", camel(&f.name));
    let diags = ctx.diags;
    (
        ir::Func {
            name: camel(&f.name),
            params,
            return_ty,
            body,
            notes: notes.detected.clone(),
        },
        notes,
        diags,
    )
}

fn try_pattern_body(
    block: &Block,
    notes: &mut TransNotes,
    ctx: &mut LowerCtx,
) -> Option<ir::Exp> {
    // Conditional sum / filter+sum
    if let Some((_acc, collection, _op, cond)) = patterns::detect_conditional_sum(block) {
        notes.detected.push("mutable accumulator".into());
        notes.detected.push("conditional accumulation".into());
        notes.detected.push("Vec iteration".into());

        // Build predicate from cond — typically `value > 0`
        let pred = lower_expr(&cond, ctx);
        // For `value > 0` where value is the loop var, we want filter (> 0)
        let filter_pred = match &cond {
            Expr::Binary {
                op,
                left,
                right,
                ..
            } if matches!(left.as_ref(), Expr::Path(_, _)) => {
                let op = lower_binop(*op);
                let rhs = lower_expr(right, ctx);
                // section: (op rhs) or \x -> x op rhs
                ir::Exp::Lam(
                    vec!["x".into()],
                    Box::new(ir::Exp::BinOp(
                        op,
                        Box::new(ir::Exp::var("x")),
                        Box::new(rhs),
                    )),
                )
            }
            _ => ir::Exp::Lam(vec!["x".into()], Box::new(pred)),
        };

        notes.translation.push("filter (...)".into());
        notes.translation.push("sum".into());
        let col = camel(&collection);
        notes.summary = format!(
            "{} values = sum (filter (...) {})",
            "fn", col
        );
        // Fix summary with actual function context later
        notes.summary = format!("sum (filter (...) {col})");

        return Some(ir::Exp::Sum(Box::new(ir::Exp::Filter(
            Box::new(filter_pred),
            Box::new(ir::Exp::var(col)),
        ))));
    }

    if let Some(collection) = patterns::detect_plain_sum(block) {
        notes.detected.push("mutable accumulator".into());
        notes.detected.push("Vec iteration".into());
        notes.translation.push("sum".into());
        let col = camel(&collection);
        notes.summary = format!("sum {col}");
        return Some(ir::Exp::Sum(Box::new(ir::Exp::var(col))));
    }

    if let Some((_res, collection, mapped)) = patterns::detect_map_push(block) {
        notes.detected.push("mutable Vec construction".into());
        notes.detected.push("for-loop push".into());
        notes.translation.push("map".into());
        // Infer loop variable from mapped expression — use "x" and substitute
        // Find the loop pat from the for
        let pat = match &block.stmts[1] {
            Stmt::Expr(Expr::For { pat, .. }) => pat.clone(),
            _ => "x".into(),
        };
        let mut mapped_l = lower_expr(&mapped, ctx);
        mapped_l = rename_var(&mapped_l, &camel(&pat), "x");
        let f = ir::Exp::Lam(vec!["x".into()], Box::new(mapped_l));
        let col = camel(&collection);
        notes.summary = format!("map (...) {col}");
        return Some(ir::Exp::Map(Box::new(f), Box::new(ir::Exp::var(col))));
    }

    if let Some((_name, init, ops)) = patterns::detect_scalar_mutation(block) {
        notes.detected.push("scalar mutation sequence".into());
        notes.translation.push("nested arithmetic expression".into());
        let mut e = lower_expr(&init, ctx);
        for (op, val) in ops {
            let v = lower_expr(&val, ctx);
            e = ir::Exp::BinOp(lower_binop(op), Box::new(e), Box::new(v));
        }
        notes.summary = "nested arithmetic".into();
        return Some(e);
    }

    None
}

fn rename_var(exp: &ir::Exp, from: &str, to: &str) -> ir::Exp {
    match exp {
        ir::Exp::Var(v) if v == from => ir::Exp::Var(to.into()),
        ir::Exp::Var(v) => ir::Exp::Var(v.clone()),
        ir::Exp::Lit(l) => ir::Exp::Lit(l.clone()),
        ir::Exp::App(f, args) => ir::Exp::App(
            Box::new(rename_var(f, from, to)),
            args.iter().map(|a| rename_var(a, from, to)).collect(),
        ),
        ir::Exp::Lam(ps, body) => {
            if ps.iter().any(|p| p == from) {
                ir::Exp::Lam(ps.clone(), body.clone())
            } else {
                ir::Exp::Lam(ps.clone(), Box::new(rename_var(body, from, to)))
            }
        }
        ir::Exp::BinOp(op, l, r) => ir::Exp::BinOp(
            *op,
            Box::new(rename_var(l, from, to)),
            Box::new(rename_var(r, from, to)),
        ),
        ir::Exp::UnOp(op, e) => ir::Exp::UnOp(*op, Box::new(rename_var(e, from, to))),
        ir::Exp::If(c, t, e) => ir::Exp::If(
            Box::new(rename_var(c, from, to)),
            Box::new(rename_var(t, from, to)),
            Box::new(rename_var(e, from, to)),
        ),
        ir::Exp::Tuple(es) => ir::Exp::Tuple(es.iter().map(|e| rename_var(e, from, to)).collect()),
        ir::Exp::List(es) => ir::Exp::List(es.iter().map(|e| rename_var(e, from, to)).collect()),
        ir::Exp::Field(e, f) => ir::Exp::Field(Box::new(rename_var(e, from, to)), f.clone()),
        ir::Exp::Map(f, xs) => ir::Exp::Map(
            Box::new(rename_var(f, from, to)),
            Box::new(rename_var(xs, from, to)),
        ),
        ir::Exp::Filter(f, xs) => ir::Exp::Filter(
            Box::new(rename_var(f, from, to)),
            Box::new(rename_var(xs, from, to)),
        ),
        other => other.clone(),
    }
}

fn lower_block(block: &Block, ctx: &mut LowerCtx, notes: &mut TransNotes) -> ir::Exp {
    // Convert statements into nested lets
    lower_stmts(&block.stmts, block.expr.as_deref(), ctx, notes)
}

fn lower_stmts(
    stmts: &[Stmt],
    trailing: Option<&Expr>,
    ctx: &mut LowerCtx,
    notes: &mut TransNotes,
) -> ir::Exp {
    if stmts.is_empty() {
        return match trailing {
            Some(e) => lower_expr(e, ctx),
            None => ir::Exp::Lit(ir::Lit::Unit),
        };
    }

    let (first, rest) = stmts.split_first().unwrap();
    match first {
        Stmt::Let {
            name,
            is_mut: _,
            value,
            ..
        } => {
            let val = match value {
                Some(v) => lower_expr(v, ctx),
                None => ir::Exp::Error("uninitialized let".into()),
            };
            let body = lower_stmts(rest, trailing, ctx, notes);
            ir::Exp::Let(
                vec![ir::Binding {
                    name: camel(name),
                    value: val,
                }],
                Box::new(body),
            )
        }
        Stmt::Expr(Expr::For {
            pat,
            iter,
            body,
            span,
        }) => {
            // Generic for → fold / map depending on body
            notes.detected.push("for-loop".into());
            if let Some(exp) = lower_for_as_foldl(pat, iter, body, rest, trailing, ctx, notes) {
                return exp;
            }
            ctx.diags.push(
                Diagnostic::unsupported(
                    "this for-loop pattern",
                    &ctx.filename,
                    span.line,
                    span.column,
                )
                .note("Raskell currently recognises map/filter/sum-style loops."),
            );
            ir::Exp::Error("unsupported for".into())
        }
        Stmt::Expr(e) => {
            // Expression statement: if it's assign, thread state; else sequence
            match e {
                Expr::Assign { target, value, .. } => {
                    if let Expr::Path(name, _) = target.as_ref() {
                        let val = lower_expr(value, ctx);
                        let body = lower_stmts(rest, trailing, ctx, notes);
                        return ir::Exp::Let(
                            vec![ir::Binding {
                                name: camel(name),
                                value: val,
                            }],
                            Box::new(body),
                        );
                    }
                }
                Expr::AssignOp {
                    op,
                    target,
                    value,
                    ..
                } => {
                    if let Expr::Path(name, _) = target.as_ref() {
                        let rhs = lower_expr(value, ctx);
                        let val = ir::Exp::BinOp(
                            lower_binop(*op),
                            Box::new(ir::Exp::var(camel(name))),
                            Box::new(rhs),
                        );
                        let body = lower_stmts(rest, trailing, ctx, notes);
                        return ir::Exp::Let(
                            vec![ir::Binding {
                                name: camel(name),
                                value: val,
                            }],
                            Box::new(body),
                        );
                    }
                }
                Expr::While { span, .. } => {
                    ctx.diags.push(Diagnostic::unsupported(
                        "while loops (generic)",
                        &ctx.filename,
                        span.line,
                        span.column,
                    ).note("While loops need a clear functional interpretation; use recursion or for where possible."));
                    return ir::Exp::Error("unsupported while".into());
                }
                Expr::Loop { span, .. } => {
                    ctx.diags.push(Diagnostic::unsupported(
                        "loop",
                        &ctx.filename,
                        span.line,
                        span.column,
                    ));
                    return ir::Exp::Error("unsupported loop".into());
                }
                _ => {}
            }
            // Last statement with no trailing expr → keep its value (e.g. putStrLn)
            if rest.is_empty() && trailing.is_none() {
                return lower_expr(e, ctx);
            }
            let head = lower_expr(e, ctx);
            let body = lower_stmts(rest, trailing, ctx, notes);
            // Sequence with >> when the head looks like an IO action
            if is_io_exp(&head) || is_io_exp(&body) {
                return ir::Exp::app(ir::Exp::var("(>>)"), vec![head, body]);
            }
            // Pure: discard via let _ =
            ir::Exp::Let(
                vec![ir::Binding {
                    name: "_".into(),
                    value: head,
                }],
                Box::new(body),
            )
        }
        Stmt::Return(val, _) => match val {
            Some(e) => lower_expr(e, ctx),
            None => ir::Exp::Lit(ir::Lit::Unit),
        },
        Stmt::Break(span) | Stmt::Continue(span) => {
            ctx.diags.push(Diagnostic::unsupported(
                "break/continue",
                &ctx.filename,
                span.line,
                span.column,
            ));
            ir::Exp::Error("break/continue".into())
        }
    }
}

fn lower_for_as_foldl(
    pat: &str,
    iter: &Expr,
    body: &Block,
    rest: &[Stmt],
    trailing: Option<&Expr>,
    ctx: &mut LowerCtx,
    notes: &mut TransNotes,
) -> Option<ir::Exp> {
    // If the for is the last meaningful thing and body mutates one accumulator via +=
    // already handled by pattern detectors on whole function body.
    // Here: treat as map if body is a single expression used somehow — limited.

    // Simple case: for x in xs { } with no mutation — ignore
    let _ = (pat, body, rest, trailing, notes);

    // Fallback: foldl over unit — not useful
    let xs = lower_expr(iter, ctx);
    // If body is just an expression statement that doesn't assign, skip
    None.filter(|_| {
        let _ = xs;
        false
    })
}

fn lower_expr(expr: &Expr, ctx: &mut LowerCtx) -> ir::Exp {
    // Iterator chains
    if let Some((base, steps)) = patterns::peel_iterator_chain(expr) {
        return lower_iterator_chain(base, steps, ctx);
    }

    match expr {
        Expr::Lit(l, _) => ir::Exp::Lit(lower_lit(l)),
        Expr::Path(p, _) => lower_path(p),
        Expr::Field { base, field, .. } => {
            ir::Exp::Field(Box::new(lower_expr(base, ctx)), camel(field))
        }
        Expr::Index { base, index, .. } => ir::Exp::app(
            ir::Exp::var("(!!)"),
            vec![lower_expr(base, ctx), lower_expr(index, ctx)],
        ),
        Expr::Call { func, args, .. } => {
            let f = lower_expr(func, ctx);
            let as_ = args
                .iter()
                .map(|a| lower_expr(a, ctx))
                .collect::<Vec<_>>();
            // Special: Some(x) → Just x, None → Nothing, Ok → Right, Err → Left
            match &f {
                ir::Exp::Var(name) => match name.as_str() {
                    "Some" => ir::Exp::app(ir::Exp::var("Just"), as_),
                    "None" => ir::Exp::var("Nothing"),
                    "Ok" => ir::Exp::app(ir::Exp::var("Right"), as_),
                    "Err" => ir::Exp::app(ir::Exp::var("Left"), as_),
                    "range" => {
                        // range a b → [a .. b-1]  (Rust is exclusive end)
                        if as_.len() == 2 {
                            ir::Exp::app(
                                ir::Exp::var("enumFromTo"),
                                vec![
                                    as_[0].clone(),
                                    ir::Exp::BinOp(
                                        ir::BinOp::Sub,
                                        Box::new(as_[1].clone()),
                                        Box::new(ir::Exp::int(1)),
                                    ),
                                ],
                            )
                        } else {
                            ir::Exp::App(Box::new(f), as_)
                        }
                    }
                    "replicate" => ir::Exp::app(ir::Exp::var("replicate"), as_),
                    "println" | "print" => {
                        // IO: putStrLn
                        ir::Exp::app(ir::Exp::var("putStrLn"), as_)
                    }
                    _ => ir::Exp::App(Box::new(f), as_),
                },
                _ => ir::Exp::App(Box::new(f), as_),
            }
        }
        Expr::MethodCall {
            receiver,
            method,
            args,
            span,
        } => match method.as_str() {
            "len" | "length" => ir::Exp::app(ir::Exp::var("length"), vec![lower_expr(receiver, ctx)]),
            "is_empty" => ir::Exp::app(
                ir::Exp::var("null"),
                vec![lower_expr(receiver, ctx)],
            ),
            "clone" | "to_owned" | "to_string" => lower_expr(receiver, ctx),
            "as_str" => lower_expr(receiver, ctx),
            "unwrap" => ir::Exp::app(
                ir::Exp::var("fromJust"),
                vec![lower_expr(receiver, ctx)],
            ),
            "unwrap_or" => {
                let default = args
                    .first()
                    .map(|a| lower_expr(a, ctx))
                    .unwrap_or(ir::Exp::Lit(ir::Lit::Unit));
                ir::Exp::app(
                    ir::Exp::var("fromMaybe"),
                    vec![default, lower_expr(receiver, ctx)],
                )
            }
            "push" => {
                // x.push(v) as expression is unusual; treat as snoc
                let v = args
                    .first()
                    .map(|a| lower_expr(a, ctx))
                    .unwrap_or(ir::Exp::Lit(ir::Lit::Unit));
                ir::Exp::BinOp(
                    ir::BinOp::Append,
                    Box::new(lower_expr(receiver, ctx)),
                    Box::new(ir::Exp::List(vec![v])),
                )
            }
            "new" => {
                // Type::new() — Vec::new already handled as path call
                ir::Exp::List(vec![])
            }
            other => {
                ctx.diags.push(
                    Diagnostic::unsupported(
                        &format!("method `.{other}`"),
                        &ctx.filename,
                        span.line,
                        span.column,
                    )
                    .note("Map this method to a Haskell equivalent or rewrite the source."),
                );
                ir::Exp::Error(format!("method {other}"))
            }
        },
        Expr::Binary {
            op, left, right, ..
        } => ir::Exp::BinOp(
            lower_binop(*op),
            Box::new(lower_expr(left, ctx)),
            Box::new(lower_expr(right, ctx)),
        ),
        Expr::Unary { op, expr, .. } => {
            let u = match op {
                UnOp::Neg => ir::UnOp::Neg,
                UnOp::Not => ir::UnOp::Not,
            };
            ir::Exp::UnOp(u, Box::new(lower_expr(expr, ctx)))
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => {
            let c = lower_expr(cond, ctx);
            let mut notes = TransNotes::default();
            let t = lower_block(then_branch, ctx, &mut notes);
            let e = match else_branch {
                Some(b) => match b.as_ref() {
                    Expr::Block(bl) => lower_block(bl, ctx, &mut notes),
                    other => lower_expr(other, ctx),
                },
                None => ir::Exp::Lit(ir::Lit::Unit),
            };
            ir::Exp::If(Box::new(c), Box::new(t), Box::new(e))
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            let s = lower_expr(scrutinee, ctx);
            let ir_arms = arms
                .iter()
                .map(|arm| ir::Arm {
                    pattern: lower_pattern(&arm.pattern),
                    guard: arm.guard.as_ref().map(|g| lower_expr(g, ctx)),
                    body: lower_expr(&arm.body, ctx),
                })
                .collect();
            ir::Exp::Case(Box::new(s), ir_arms)
        }
        Expr::Block(b) => {
            let mut notes = TransNotes::default();
            lower_block(b, ctx, &mut notes)
        }
        Expr::Closure { params, body, .. } => ir::Exp::Lam(
            params.iter().map(|p| camel(p)).collect(),
            Box::new(lower_expr(body, ctx)),
        ),
        Expr::Tuple(es, _) => {
            ir::Exp::Tuple(es.iter().map(|e| lower_expr(e, ctx)).collect())
        }
        Expr::Array(es, _) => {
            ir::Exp::List(es.iter().map(|e| lower_expr(e, ctx)).collect())
        }
        Expr::Struct { name, fields, .. } => {
            let mut map = IndexMap::new();
            for (k, v) in fields {
                map.insert(camel(k), lower_expr(v, ctx));
            }
            ir::Exp::Record {
                name: name.clone(),
                fields: map,
            }
        }
        Expr::Assign { target, value, .. } => {
            // As expression, rare — just produce value
            let _ = target;
            lower_expr(value, ctx)
        }
        Expr::AssignOp {
            op,
            target,
            value,
            ..
        } => {
            if let Expr::Path(name, _) = target.as_ref() {
                ir::Exp::BinOp(
                    lower_binop(*op),
                    Box::new(ir::Exp::var(camel(name))),
                    Box::new(lower_expr(value, ctx)),
                )
            } else {
                lower_expr(value, ctx)
            }
        }
        Expr::Reference { expr, .. } | Expr::Deref { expr, .. } => lower_expr(expr, ctx),
        Expr::For { span, .. } => {
            ctx.diags.push(Diagnostic::unsupported(
                "for expression in this position",
                &ctx.filename,
                span.line,
                span.column,
            ));
            ir::Exp::Error("for".into())
        }
        Expr::While { span, .. } => {
            ctx.diags.push(Diagnostic::unsupported(
                "while",
                &ctx.filename,
                span.line,
                span.column,
            ));
            ir::Exp::Error("while".into())
        }
        Expr::Loop { span, .. } => {
            ctx.diags.push(Diagnostic::unsupported(
                "loop",
                &ctx.filename,
                span.line,
                span.column,
            ));
            ir::Exp::Error("loop".into())
        }
        Expr::Return(v, _) => match v {
            Some(e) => lower_expr(e, ctx),
            None => ir::Exp::Lit(ir::Lit::Unit),
        },
        Expr::Cast { expr, .. } => lower_expr(expr, ctx), // erase casts
        Expr::Try(e, span) => {
            // e? → case e of Left err -> Left err; Right x -> ...  (needs monadic context)
            // For now: translate as fromRight-ish or keep as app of a helper
            ctx.diags.push(
                Diagnostic::warning("W0001", "`?` operator lowered as `either` projection")
                    .at(&ctx.filename, span.line, span.column)
                    .note("Prefer explicit match for clearer Either handling."),
            );
            ir::Exp::app(
                ir::Exp::var("either"),
                vec![
                    ir::Exp::var("Left"),
                    ir::Exp::var("id"),
                    lower_expr(e, ctx),
                ],
            )
        }
        Expr::Unsupported { description, span } => {
            ctx.diags.push(Diagnostic::unsupported(
                description,
                &ctx.filename,
                span.line,
                span.column,
            ));
            ir::Exp::Error(description.clone())
        }
    }
}

fn lower_iterator_chain(
    base: Expr,
    steps: Vec<patterns::IterStep>,
    ctx: &mut LowerCtx,
) -> ir::Exp {
    let mut exp = lower_expr(&base, ctx);
    // Skip iter/into_iter/iter_mut/cloned/copied — erase
    for step in steps {
        match step.method.as_str() {
            "iter" | "into_iter" | "iter_mut" | "cloned" | "copied" => {}
            "map" => {
                let f = step
                    .args
                    .first()
                    .map(|a| lower_closure_or_expr(a, ctx))
                    .unwrap_or(ir::Exp::var("id"));
                exp = ir::Exp::Map(Box::new(f), Box::new(exp));
            }
            "filter" => {
                let p = step
                    .args
                    .first()
                    .map(|a| lower_closure_or_expr(a, ctx))
                    .unwrap_or(ir::Exp::var("const True"));
                exp = ir::Exp::Filter(Box::new(p), Box::new(exp));
            }
            "collect" => {
                // identity — already a list
            }
            "sum" => {
                exp = ir::Exp::Sum(Box::new(exp));
            }
            "fold" => {
                // fold(init, |acc, x| ...)
                if step.args.len() >= 2 {
                    let init = lower_expr(&step.args[0], ctx);
                    let f = lower_closure_or_expr(&step.args[1], ctx);
                    exp = ir::Exp::Fold(Box::new(f), Box::new(init), Box::new(exp));
                }
            }
            "count" => {
                exp = ir::Exp::app(ir::Exp::var("length"), vec![exp]);
            }
            "take" => {
                let n = step
                    .args
                    .first()
                    .map(|a| lower_expr(a, ctx))
                    .unwrap_or(ir::Exp::int(0));
                exp = ir::Exp::app(ir::Exp::var("take"), vec![n, exp]);
            }
            "skip" => {
                let n = step
                    .args
                    .first()
                    .map(|a| lower_expr(a, ctx))
                    .unwrap_or(ir::Exp::int(0));
                exp = ir::Exp::app(ir::Exp::var("drop"), vec![n, exp]);
            }
            "rev" => {
                exp = ir::Exp::app(ir::Exp::var("reverse"), vec![exp]);
            }
            "any" => {
                let p = step
                    .args
                    .first()
                    .map(|a| lower_closure_or_expr(a, ctx))
                    .unwrap_or(ir::Exp::var("const True"));
                exp = ir::Exp::app(ir::Exp::var("any"), vec![p, exp]);
            }
            "all" => {
                let p = step
                    .args
                    .first()
                    .map(|a| lower_closure_or_expr(a, ctx))
                    .unwrap_or(ir::Exp::var("const True"));
                exp = ir::Exp::app(ir::Exp::var("all"), vec![p, exp]);
            }
            "filter_map" => {
                let f = step
                    .args
                    .first()
                    .map(|a| lower_closure_or_expr(a, ctx))
                    .unwrap_or(ir::Exp::var("Just"));
                // mapMaybe in Haskell
                exp = ir::Exp::app(ir::Exp::var("mapMaybe"), vec![f, exp]);
            }
            other => {
                ctx.diags.push(
                    Diagnostic::warning(
                        "W0002",
                        format!("iterator method `.{other}` passed through as application"),
                    )
                    .note("Verify the generated Haskell."),
                );
                exp = ir::Exp::app(ir::Exp::var(camel(other)), vec![exp]);
            }
        }
    }
    exp
}

fn lower_closure_or_expr(expr: &Expr, ctx: &mut LowerCtx) -> ir::Exp {
    match expr {
        Expr::Closure { params, body, .. } => {
            // Strip ref/deref noise in body for |x| **x > 10 style
            let body = strip_ref_noise(body);
            ir::Exp::Lam(
                params.iter().map(|p| camel(p)).collect(),
                Box::new(lower_expr(&body, ctx)),
            )
        }
        other => lower_expr(other, ctx),
    }
}

fn strip_ref_noise(expr: &Expr) -> Expr {
    match expr {
        Expr::Unary {
            op: UnOp::Not,
            ..
        } => expr.clone(),
        Expr::Deref { expr, .. } => strip_ref_noise(expr),
        Expr::Reference { expr, .. } => strip_ref_noise(expr),
        Expr::Binary {
            op,
            left,
            right,
            span,
        } => Expr::Binary {
            op: *op,
            left: Box::new(strip_ref_noise(left)),
            right: Box::new(strip_ref_noise(right)),
            span: *span,
        },
        Expr::Unary { op, expr, span } => Expr::Unary {
            op: *op,
            expr: Box::new(strip_ref_noise(expr)),
            span: *span,
        },
        other => other.clone(),
    }
}

fn lower_path(p: &str) -> ir::Exp {
    // Qualified paths
    if let Some((a, b)) = p.rsplit_once("::") {
        match (a, b) {
            (_, "Some") => ir::Exp::var("Just"),
            (_, "None") => ir::Exp::var("Nothing"),
            (_, "Ok") => ir::Exp::var("Right"),
            (_, "Err") => ir::Exp::var("Left"),
            ("Vec", "new") => ir::Exp::List(vec![]),
            ("String", "new") => ir::Exp::Lit(ir::Lit::Str(String::new())),
            ("String", "from") => {
                // String::from is applied via Call — path alone is id
                ir::Exp::var("id")
            }
            (_, "new") if a.ends_with("Vec") => ir::Exp::List(vec![]),
            (_, "from") => ir::Exp::var("id"),
            _ => {
                // Enum variant: Shape::Circle → Circle
                ir::Exp::var(b.to_string())
            }
        }
    } else {
        match p {
            "Some" => ir::Exp::var("Just"),
            "None" => ir::Exp::var("Nothing"),
            "Ok" => ir::Exp::var("Right"),
            "Err" => ir::Exp::var("Left"),
            "true" => ir::Exp::Lit(ir::Lit::Bool(true)),
            "false" => ir::Exp::Lit(ir::Lit::Bool(false)),
            other => ir::Exp::var(camel(other)),
        }
    }
}

fn lower_lit(l: &Lit) -> ir::Lit {
    match l {
        Lit::Int(n) => ir::Lit::Int(*n),
        Lit::Float(f) => ir::Lit::Float(*f),
        Lit::Bool(b) => ir::Lit::Bool(*b),
        Lit::Str(s) => ir::Lit::Str(s.clone()),
        Lit::Char(c) => ir::Lit::Char(*c),
        Lit::Unit => ir::Lit::Unit,
    }
}

fn lower_binop(op: BinOp) -> ir::BinOp {
    match op {
        BinOp::Add => ir::BinOp::Add,
        BinOp::Sub => ir::BinOp::Sub,
        BinOp::Mul => ir::BinOp::Mul,
        BinOp::Div => ir::BinOp::Div,
        BinOp::Rem => ir::BinOp::Rem,
        BinOp::Eq => ir::BinOp::Eq,
        BinOp::Ne => ir::BinOp::Ne,
        BinOp::Lt => ir::BinOp::Lt,
        BinOp::Le => ir::BinOp::Le,
        BinOp::Gt => ir::BinOp::Gt,
        BinOp::Ge => ir::BinOp::Ge,
        BinOp::And => ir::BinOp::And,
        BinOp::Or => ir::BinOp::Or,
        BinOp::BitAnd => ir::BinOp::BitAnd,
        BinOp::BitOr => ir::BinOp::BitOr,
        BinOp::BitXor => ir::BinOp::BitXor,
        BinOp::Shl | BinOp::Shr => ir::BinOp::Mul, // placeholder — should diagnostic
    }
}

fn lower_pattern(pat: &Pattern) -> ir::Pat {
    match pat {
        Pattern::Wildcard => ir::Pat::Wildcard,
        Pattern::Lit(l) => ir::Pat::Lit(lower_lit(l)),
        Pattern::Ident(n) => match n.as_str() {
            "None" => ir::Pat::Constr {
                name: "Nothing".into(),
                args: vec![],
            },
            "Some" => ir::Pat::Constr {
                name: "Just".into(),
                args: vec![],
            },
            "Ok" => ir::Pat::Constr {
                name: "Right".into(),
                args: vec![],
            },
            "Err" => ir::Pat::Constr {
                name: "Left".into(),
                args: vec![],
            },
            "true" | "True" => ir::Pat::Lit(ir::Lit::Bool(true)),
            "false" | "False" => ir::Pat::Lit(ir::Lit::Bool(false)),
            other => ir::Pat::Var(camel(other)),
        },
        Pattern::Tuple(ps) => ir::Pat::Tuple(ps.iter().map(lower_pattern).collect()),
        Pattern::TupleStruct { name, elems } => {
            let name = match name_last(name).as_str() {
                "Some" => "Just".into(),
                "None" => "Nothing".into(),
                "Ok" => "Right".into(),
                "Err" => "Left".into(),
                other => other.to_string(),
            };
            ir::Pat::Constr {
                name,
                args: elems.iter().map(lower_pattern).collect(),
            }
        }
        Pattern::Variant {
            variant, elems, ..
        } => {
            let name = match variant.as_str() {
                "Some" => "Just".into(),
                "None" => "Nothing".into(),
                "Ok" => "Right".into(),
                "Err" => "Left".into(),
                other => other.to_string(),
            };
            ir::Pat::Constr {
                name,
                args: elems.iter().map(lower_pattern).collect(),
            }
        }
        Pattern::Struct { name, fields } => {
            let mut map = IndexMap::new();
            for (k, p) in fields {
                map.insert(camel(k), lower_pattern(p));
            }
            ir::Pat::Record {
                name: name_last(name),
                fields: map,
            }
        }
        Pattern::Path(p) => {
            let name = name_last(p);
            let name = match name.as_str() {
                "Some" => "Just".into(),
                "None" => "Nothing".into(),
                "Ok" => "Right".into(),
                "Err" => "Left".into(),
                other => other.to_string(),
            };
            ir::Pat::Constr {
                name,
                args: vec![],
            }
        }
        Pattern::Ref { inner, .. } => lower_pattern(inner),
    }
}

fn name_last(p: &str) -> String {
    p.rsplit("::").next().unwrap_or(p).to_string()
}

fn is_io_exp(exp: &ir::Exp) -> bool {
    match exp {
        ir::Exp::App(f, _) => match f.as_ref() {
            ir::Exp::Var(v) => {
                matches!(v.as_str(), "putStrLn" | "putStr" | "getLine" | "print" | "(>>)" | "(>>=)")
                    || is_io_exp(f)
            }
            _ => is_io_exp(f),
        },
        ir::Exp::Var(v) => matches!(v.as_str(), "putStrLn" | "putStr" | "getLine"),
        ir::Exp::Let(_, body) => is_io_exp(body),
        ir::Exp::Do(_, _) => true,
        _ => false,
    }
}
