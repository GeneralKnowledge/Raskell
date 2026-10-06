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

    // Traits → type classes
    for (name, t) in &analyzed.traits {
        module.decls.push(ir::Decl::Class(trait_to_class(t)));
        let _ = name;
    }

    // Trait impls → instances (inherent impls already expanded into functions)
    for impl_block in &analyzed.impls {
        if let Some(trait_name) = &impl_block.trait_name {
            let (inst, mut idiags) =
                impl_to_instance(impl_block, trait_name, &analyzed.program.filename);
            diags.append(std::mem::take(&mut idiags));
            module.decls.push(ir::Decl::Instance(inst));
        }
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

    // Functions (includes expanded inherent-impl methods)
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
    // Strip leading digits/underscores from corpus-style names (`01_square` → `square`)
    let trimmed = stem.trim_start_matches(|c: char| c.is_ascii_digit() || c == '_');
    let stem = if trimmed.is_empty() { "Main" } else { trimmed };
    // Haskell module names: PascalCase, no hyphens/underscores, must start with uppercase letter
    let name = stem.to_pascal_case().replace('-', "");
    if name
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_uppercase())
    {
        name
    } else {
        format!("M{name}")
    }
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
    lower_type_in(ty, &[])
}

fn hs_var(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) => format!("{}{}", c.to_lowercase(), chars.collect::<String>()),
        None => "a".into(),
    }
}

fn lower_type_in(ty: &Type, generics: &[String]) -> Ty {
    match ty {
        Type::Unit => Ty::Unit,
        Type::SelfType => Ty::Var("a".into()),
        Type::Generic(g) => Ty::Var(hs_var(g)),
        Type::Named(n, args) => {
            if generics.iter().any(|g| g == n) {
                return Ty::Var(hs_var(n));
            }
            match n.as_str() {
                "i8" | "i16" | "i32" | "i64" | "i128" | "isize" => Ty::Int,
                "u8" | "u16" | "u32" => Ty::Word32,
                "u64" | "u128" | "usize" => Ty::Word64,
                "f32" => Ty::Float,
                "f64" => Ty::Double,
                "bool" => Ty::Bool,
                "char" => Ty::Char,
                "String" | "str" => Ty::String,
                other => Ty::Named(
                    other.to_string(),
                    args.iter().map(|a| lower_type_in(a, generics)).collect(),
                ),
            }
        }
        Type::Path(parts) => {
            let n = parts.last().map(|s| s.as_str()).unwrap_or("()");
            lower_type_in(&Type::Named(n.to_string(), vec![]), generics)
        }
        Type::Ref { inner, .. } => lower_type_in(inner, generics),
        Type::Tuple(ts) => Ty::Tuple(ts.iter().map(|t| lower_type_in(t, generics)).collect()),
        Type::Array(t) | Type::Vec(t) => Ty::List(Box::new(lower_type_in(t, generics))),
        Type::Option(t) => Ty::Maybe(Box::new(lower_type_in(t, generics))),
        Type::Result(ok, err) => Ty::Either(
            Box::new(lower_type_in(err, generics)),
            Box::new(lower_type_in(ok, generics)),
        ),
        Type::Fun { params, ret } => Ty::Fun(
            params
                .iter()
                .map(|t| lower_type_in(t, generics))
                .collect(),
            Box::new(lower_type_in(ret, generics)),
        ),
        Type::Infer => Ty::Var("a".into()),
    }
}

fn is_self_ty(ty: &Type) -> bool {
    match ty {
        Type::SelfType => true,
        Type::Ref { inner, .. } => is_self_ty(inner),
        _ => false,
    }
}

fn trait_to_class(t: &TraitDef) -> ir::Class {
    let type_var = "a".to_string();
    let methods = t
        .methods
        .iter()
        .map(|m| {
            let params: Vec<Ty> = m
                .params
                .iter()
                .map(|p| {
                    if p.is_self || is_self_ty(&p.ty) {
                        Ty::Var(type_var.clone())
                    } else {
                        lower_type_in(&p.ty, &t.generics)
                    }
                })
                .collect();
            let ret = m
                .return_type
                .as_ref()
                .map(|rt| {
                    if is_self_ty(rt) {
                        Ty::Var(type_var.clone())
                    } else {
                        lower_type_in(rt, &t.generics)
                    }
                })
                .unwrap_or(Ty::Unit);
            let ty = if params.is_empty() {
                ret
            } else {
                Ty::Fun(params, Box::new(ret))
            };
            ir::ClassMethod {
                name: camel(&m.name),
                ty,
            }
        })
        .collect();
    ir::Class {
        name: t.name.clone(),
        type_var,
        methods,
    }
}

fn impl_to_instance(
    impl_block: &ImplBlock,
    trait_name: &str,
    filename: &str,
) -> (ir::Instance, Diagnostics) {
    let mut diags = Diagnostics::new();
    let self_ty = lower_type(&impl_block.for_type);
    let mut methods = Vec::new();
    for m in &impl_block.methods {
        let mut f = m.clone();
        // Rewrite self params to concrete type for lowering
        f.params = f
            .params
            .iter()
            .map(|p| {
                if p.is_self {
                    Param {
                        name: "self_".into(),
                        ty: impl_block.for_type.clone(),
                        is_mut: p.is_mut,
                        by_ref: false,
                        is_self: false,
                        span: p.span,
                    }
                } else {
                    p.clone()
                }
            })
            .collect();
        let (func, _notes, fdiags) = lower_function(&f, filename);
        diags.append(fdiags);
        methods.push(func);
    }
    (
        ir::Instance {
            class: trait_name.to_string(),
            ty: self_ty,
            methods,
        },
        diags,
    )
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

    let type_vars: Vec<String> = f.generics.iter().map(|g| hs_var(g)).collect();
    let constraints: Vec<(String, String)> = f
        .bounds
        .iter()
        .flat_map(|(param, traits)| {
            let v = hs_var(param);
            traits
                .iter()
                .map(|t| (t.clone(), v.clone()))
                .collect::<Vec<_>>()
        })
        .collect();

    let params: Vec<(String, Ty)> = f
        .params
        .iter()
        .map(|p| {
            let name = if p.name == "self" || p.name == "self_" {
                "self_".to_string()
            } else {
                camel(&p.name)
            };
            let ty = lower_type_in(&p.ty, &f.generics);
            (name, ty)
        })
        .collect();

    let return_ty = f
        .return_type
        .as_ref()
        .map(|t| lower_type_in(t, &f.generics))
        .unwrap_or(Ty::Unit);

    if !f.generics.is_empty() {
        notes.detected.push("generics".into());
        notes.translation.push("Haskell type variables".into());
    }
    if !constraints.is_empty() {
        notes.detected.push("trait bounds".into());
        notes.translation.push("Haskell type-class constraints".into());
    }

    // Try pattern recognition on the whole body first
    if let Some(body) = try_pattern_body(&f.body, &mut notes, &mut ctx) {
        let (body, return_ty) = promote_mut_update_return(body, return_ty, &params, &mut notes);
        let func = ir::Func {
            name: camel(&f.name),
            params,
            return_ty,
            body,
            notes: notes.detected.clone(),
            type_vars,
            constraints,
        };
        let diags = std::mem::take(&mut ctx.diags);
        return (func, notes, diags);
    }

    // `?` error propagation → do-notation
    if block_has_try(&f.body) {
        notes.detected.push("`?` error propagation".into());
        notes.translation.push("Either/Maybe do-notation".into());
        if let Some(body) = lower_try_block(&f.body, &mut ctx) {
            notes.summary = format!("{} = do …", camel(&f.name));
            let diags = std::mem::take(&mut ctx.diags);
            return (
                ir::Func {
                    name: camel(&f.name),
                    params,
                    return_ty,
                    body,
                    notes: notes.detected.clone(),
                    type_vars,
                    constraints,
                },
                notes,
                diags,
            );
        }
    }

    let body = lower_block(&f.body, &mut ctx, &mut notes);
    let (body, return_ty) = promote_mut_update_return(body, return_ty, &params, &mut notes);
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
            type_vars,
            constraints,
        },
        notes,
        diags,
    )
}

fn block_has_try(block: &Block) -> bool {
    block.stmts.iter().any(|s| match s {
        Stmt::Let {
            value: Some(e), ..
        }
        | Stmt::Expr(e)
        | Stmt::Return(Some(e), _) => expr_has_try(e),
        _ => false,
    }) || block
        .expr
        .as_ref()
        .is_some_and(|e| expr_has_try(e))
}

fn expr_has_try(expr: &Expr) -> bool {
    match expr {
        Expr::Try(_, _) => true,
        Expr::Call { func, args, .. } => {
            expr_has_try(func) || args.iter().any(expr_has_try)
        }
        Expr::MethodCall { receiver, args, .. } => {
            expr_has_try(receiver) || args.iter().any(expr_has_try)
        }
        Expr::Binary { left, right, .. } | Expr::Assign { target: left, value: right, .. } => {
            expr_has_try(left) || expr_has_try(right)
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => {
            expr_has_try(cond)
                || block_has_try(then_branch)
                || else_branch.as_ref().is_some_and(|e| expr_has_try(e))
        }
        Expr::Block(b) => block_has_try(b),
        Expr::Unary { expr, .. }
        | Expr::Deref { expr, .. }
        | Expr::Reference { expr, .. }
        | Expr::Field { base: expr, .. } => expr_has_try(expr),
        _ => false,
    }
}

/// Lower a block that uses `?` into `do` notation.
fn lower_try_block(block: &Block, ctx: &mut LowerCtx) -> Option<ir::Exp> {
    let mut stmts = Vec::new();
    for stmt in &block.stmts {
        match stmt {
            Stmt::Let {
                name,
                value: Some(Expr::Try(inner, _)),
                ..
            } => {
                stmts.push(ir::DoStmt::Bind {
                    name: camel(name),
                    exp: lower_expr(inner, ctx),
                });
            }
            Stmt::Let {
                name,
                value: Some(v),
                ..
            } => {
                stmts.push(ir::DoStmt::Let {
                    name: camel(name),
                    exp: lower_expr(v, ctx),
                });
            }
            Stmt::Expr(Expr::Try(inner, _)) => {
                stmts.push(ir::DoStmt::Bind {
                    name: "_".into(),
                    exp: lower_expr(inner, ctx),
                });
            }
            Stmt::Expr(e) => {
                stmts.push(ir::DoStmt::Exp(lower_expr(e, ctx)));
            }
            Stmt::Return(Some(e), _) => {
                let last = unwrap_ok_some(e, ctx);
                return Some(ir::Exp::Do(stmts, Box::new(last)));
            }
            _ => return None,
        }
    }
    let last = match &block.expr {
        Some(e) => unwrap_ok_some(e, ctx),
        None => ir::Exp::Lit(ir::Lit::Unit),
    };
    Some(ir::Exp::Do(stmts, Box::new(last)))
}

fn unwrap_ok_some(expr: &Expr, ctx: &mut LowerCtx) -> ir::Exp {
    match expr {
        Expr::Call { func, args, .. } => match func.as_ref() {
            Expr::Path(p, _) if p == "Ok" || p.ends_with("::Ok") || p == "Some" || p.ends_with("::Some") => {
                if let Some(a) = args.first() {
                    return ir::Exp::Pure(Box::new(lower_expr(a, ctx)));
                }
            }
            Expr::Path(p, _) if p == "Err" || p.ends_with("::Err") || p == "None" || p.ends_with("::None") => {
                return lower_expr(expr, ctx);
            }
            _ => {}
        },
        _ => {}
    }
    // Bare value in a Result-returning function — wrap with return
    ir::Exp::Pure(Box::new(lower_expr(expr, ctx)))
}

fn wrap_setup_lets(block: &Block, body: ir::Exp, ctx: &mut LowerCtx) -> ir::Exp {
    let skip = patterns::skip_setup_lets(block);
    let mut exp = body;
    for stmt in block.stmts[..skip].iter().rev() {
        if let Stmt::Let {
            name,
            value: Some(v),
            ..
        } = stmt
        {
            exp = ir::Exp::Let(
                vec![ir::Binding {
                    name: camel(name),
                    value: lower_expr(v, ctx),
                }],
                Box::new(exp),
            );
        }
    }
    exp
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
        let skip = patterns::skip_setup_lets(block);
        let pat = match &block.stmts[skip + 1] {
            Stmt::Expr(Expr::For { pat, .. }) => pat.clone(),
            _ => "x".into(),
        };
        let mut mapped_l = lower_expr(&mapped, ctx);
        mapped_l = rename_var(&mapped_l, &camel(&pat), "x");
        let f = ir::Exp::Lam(vec!["x".into()], Box::new(mapped_l));
        let col = camel(&collection);
        notes.summary = format!("map (...) {col}");
        return Some(wrap_setup_lets(block, ir::Exp::Map(Box::new(f), Box::new(ir::Exp::var(col))), ctx));
    }

    // Composable filter+map build (temps, continue-skips, if-push)
    if let Some(m) = crate::translate::loop_analysis::analyze_filter_map_build(block) {
        notes.detected.push("collection traversal".into());
        notes.detected.push("conditional filtering".into());
        notes.detected.push("element transformation".into());
        notes.translation.push("filter + map".into());
        let mut pred = lower_expr(&m.predicate, ctx);
        pred = rename_var(&pred, &camel(&m.pat), "x");
        let mut mapped_l = lower_expr(&m.mapped, ctx);
        mapped_l = rename_var(&mapped_l, &camel(&m.pat), "x");
        let col = camel(&m.collection);
        notes.summary = format!("map (…) (filter (…) {col})");
        return Some(wrap_setup_lets(
            block,
            ir::Exp::Map(
                Box::new(ir::Exp::Lam(vec!["x".into()], Box::new(mapped_l))),
                Box::new(ir::Exp::Filter(
                    Box::new(ir::Exp::Lam(vec!["x".into()], Box::new(pred))),
                    Box::new(ir::Exp::var(col)),
                )),
            ),
            ctx,
        ));
    }

    // Composable filtered fold: acc += f(x) under if / continue
    if let Some(f) = crate::translate::loop_analysis::analyze_filtered_fold(block) {
        notes.detected.push("collection traversal".into());
        notes.detected.push("integer accumulation".into());
        if f.predicate.is_some() {
            notes.detected.push("conditional filtering".into());
        }
        let mut add_l = lower_expr(&f.addend, ctx);
        add_l = rename_var(&add_l, &camel(&f.pat), "x");
        let col = camel(&f.collection);
        let mapped = if matches!(&f.addend, Expr::Path(n, _) if n == &f.pat) {
            ir::Exp::var(col.clone())
        } else {
            notes.detected.push("element transformation".into());
            ir::Exp::Map(
                Box::new(ir::Exp::Lam(vec!["x".into()], Box::new(add_l))),
                Box::new(ir::Exp::var(col.clone())),
            )
        };
        let body = if let Some(pred_e) = &f.predicate {
            let mut pred = lower_expr(pred_e, ctx);
            pred = rename_var(&pred, &camel(&f.pat), "x");
            // When we map then filter on the original value, filter first then map.
            // Predicate mentions the loop pat (pre-map), so: sum (map f (filter p xs))
            let filtered = ir::Exp::Filter(
                Box::new(ir::Exp::Lam(vec!["x".into()], Box::new(pred))),
                Box::new(ir::Exp::var(col.clone())),
            );
            let core = if matches!(&f.addend, Expr::Path(n, _) if n == &f.pat) {
                filtered
            } else {
                let mut add2 = lower_expr(&f.addend, ctx);
                add2 = rename_var(&add2, &camel(&f.pat), "x");
                ir::Exp::Map(
                    Box::new(ir::Exp::Lam(vec!["x".into()], Box::new(add2))),
                    Box::new(filtered),
                )
            };
            notes.translation.push("sum (filter … ∘ map …)".into());
            notes.summary = format!("sum (map (…) (filter (…) {col}))");
            ir::Exp::Sum(Box::new(core))
        } else {
            notes.translation.push("sum / map reduction".into());
            notes.summary = format!("sum (…) {col}");
            ir::Exp::Sum(Box::new(mapped))
        };
        // Only when init is 0 — otherwise fall through to generic fold later
        if matches!(&f.acc_init, Expr::Lit(Lit::Int(0), _)) {
            return Some(wrap_setup_lets(block, body, ctx));
        }
    }

    if let Some((_acc, _init, pat, collection, addend)) = patterns::detect_fold_add(block) {
        // Prefer sum (map f xs) when init is 0
        notes.detected.push("mutable accumulator".into());
        notes.detected.push("mapped reduction".into());
        notes.translation.push("sum (map …)".into());
        let mut add_l = lower_expr(&addend, ctx);
        add_l = rename_var(&add_l, &camel(&pat), "x");
        let col = camel(&collection);
        if matches!(&addend, Expr::Path(n, _) if n == &pat) {
            notes.summary = format!("sum {col}");
            return Some(wrap_setup_lets(block, ir::Exp::Sum(Box::new(ir::Exp::var(col))), ctx));
        }
        notes.summary = format!("sum (map (...) {col})");
        return Some(wrap_setup_lets(
            block,
            ir::Exp::Sum(Box::new(ir::Exp::Map(
                Box::new(ir::Exp::Lam(vec!["x".into()], Box::new(add_l))),
                Box::new(ir::Exp::var(col)),
            ))),
            ctx,
        ));
    }

    if let Some((inits, pat, collection, updates)) = patterns::detect_multi_accum(block) {
        if inits.len() != 2 {
            return None;
        }
        notes.detected.push("multiple accumulators".into());
        notes.translation.push("foldl over tuple state".into());
        let init_tuple = ir::Exp::Tuple(inits.iter().map(|(_, v)| lower_expr(v, ctx)).collect());
        let acc_names: Vec<String> = inits.iter().map(|(n, _)| camel(n)).collect();
        let mut fields: IndexMap<String, ir::Exp> = IndexMap::new();
        for n in &acc_names {
            fields.insert(n.clone(), ir::Exp::var(n.clone()));
        }
        for (target, op, value) in &updates {
            let t = camel(target);
            let mut v = lower_expr(value, ctx);
            v = rename_var(&v, &camel(&pat), "x");
            let current = fields.get(&t).cloned().unwrap_or(ir::Exp::var(t.clone()));
            let next = ir::Exp::BinOp(lower_binop(*op), Box::new(current), Box::new(v));
            fields.insert(t, next);
        }
        let body_tuple = ir::Exp::Tuple(acc_names.iter().map(|n| fields[n].clone()).collect());
        let a = &acc_names[0];
        let b = &acc_names[1];
        let fold_fn = ir::Exp::Lam(
            vec!["acc".into(), "x".into()],
            Box::new(ir::Exp::Let(
                vec![
                    ir::Binding {
                        name: a.clone(),
                        value: ir::Exp::app(ir::Exp::var("fst"), vec![ir::Exp::var("acc")]),
                    },
                    ir::Binding {
                        name: b.clone(),
                        value: ir::Exp::app(ir::Exp::var("snd"), vec![ir::Exp::var("acc")]),
                    },
                ],
                Box::new(body_tuple),
            )),
        );
        let col = camel(&collection);
        notes.summary = format!("foldl (…) (…) {col}");
        return Some(ir::Exp::Fold(
            Box::new(fold_fn),
            Box::new(init_tuple),
            Box::new(ir::Exp::var(col)),
        ));
    }

    if let Some((record, updates, _explicit)) = patterns::detect_record_updates(block) {
        notes.detected.push("record field mutation".into());
        notes.translation.push("Haskell record update".into());
        let mut fields = IndexMap::new();
        for (field, op, value) in updates {
            let v = match op {
                None => lower_expr(&value, ctx),
                Some(op) => ir::Exp::BinOp(
                    lower_binop(op),
                    Box::new(ir::Exp::Field(
                        Box::new(ir::Exp::var(camel(&record))),
                        camel(&field),
                    )),
                    Box::new(lower_expr(&value, ctx)),
                ),
            };
            fields.insert(camel(&field), v);
        }
        notes.summary = format!("{} {{ … }}", camel(&record));
        // Stash note when this was a unit-returning &mut update — caller may fix return ty
        if !_explicit {
            notes.detected.push("mut-borrow update → returned record".into());
        }
        return Some(ir::Exp::RecordUpdate {
            base: Box::new(ir::Exp::var(camel(&record))),
            fields,
        });
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

    if let Some(while_exp) = patterns::detect_while_accum(block) {
        notes.detected.push("while-loop accumulator".into());
        notes.translation.push("tail-recursive go helper".into());
        notes.summary = "go n acc = if … then go … else acc".into();
        return Some(wrap_setup_lets(
            block,
            lower_while_accum(while_exp, ctx),
            ctx,
        ));
    }

    if let Some(euclid) = patterns::detect_euclid_while(block) {
        notes.detected.push("two-variable while".into());
        notes.detected.push("temporary swap / remainder".into());
        notes.translation.push("Euclidean go (tail recursion)".into());
        notes.summary = "go a b = if b /= 0 then go b (a `mod` b) else a".into();
        return Some(wrap_setup_lets(block, lower_euclid_while(euclid, ctx), ctx));
    }

    if let Some(collection) = patterns::detect_max_scan(block) {
        notes.detected.push("imperative max scan".into());
        notes.translation.push("maximum".into());
        let col = camel(&collection);
        notes.summary = format!("maximum {col}");
        return Some(wrap_setup_lets(
            block,
            ir::Exp::app(ir::Exp::var("maximum"), vec![ir::Exp::var(col)]),
            ctx,
        ));
    }

    if let Some(fold) = patterns::detect_continue_break_fold(block) {
        notes.detected.push("fold with continue/break".into());
        notes.translation.push("tail-recursive go over list".into());
        notes.summary = "go acc xs = …".into();
        return Some(wrap_setup_lets(block, lower_continue_break_fold(fold, ctx), ctx));
    }

    if let Some(search) = patterns::detect_indexed_search(block) {
        notes.detected.push("indexed linear search".into());
        notes.detected.push("early return Some(index)".into());
        notes.translation.push("elemIndex".into());
        let col = camel(&search.collection);
        let target = lower_expr(&search.target, ctx);
        notes.summary = format!("elemIndex … {col}");
        return Some(wrap_setup_lets(
            block,
            ir::Exp::app(
                ir::Exp::var("elemIndex"),
                vec![target, ir::Exp::var(col)],
            ),
            ctx,
        ));
    }

    if let Some(find) = crate::translate::loop_analysis::analyze_early_find(block) {
        notes.detected.push("collection traversal".into());
        notes.detected.push("early return Some(…)".into());
        notes.translation.push("find".into());
        let mut pred = lower_expr(&find.predicate, ctx);
        pred = rename_var(&pred, &camel(&find.pat), "x");
        let mut val = lower_expr(&find.value, ctx);
        val = rename_var(&val, &camel(&find.pat), "x");
        let col = camel(&find.collection);
        // If value is just the element, find pred; else mapMaybe-style via listToMaybe . filter
        notes.summary = format!("find (…) {col}");
        let find_exp = if matches!(&find.value, Expr::Path(n, _) if n == &find.pat)
            || matches!(
                &find.value,
                Expr::Deref { expr, .. } if matches!(expr.as_ref(), Expr::Path(n, _) if n == &find.pat)
            )
        {
            ir::Exp::app(
                ir::Exp::var("find"),
                vec![
                    ir::Exp::Lam(vec!["x".into()], Box::new(pred)),
                    ir::Exp::var(col),
                ],
            )
        } else {
            // find that returns a projection: listToMaybe (map f (filter p xs))
            ir::Exp::app(
                ir::Exp::var("listToMaybe"),
                vec![ir::Exp::Map(
                    Box::new(ir::Exp::Lam(vec!["x".into()], Box::new(val))),
                    Box::new(ir::Exp::Filter(
                        Box::new(ir::Exp::Lam(vec!["x".into()], Box::new(pred))),
                        Box::new(ir::Exp::var(col)),
                    )),
                )],
            )
        };
        return Some(wrap_setup_lets(block, find_exp, ctx));
    }

    if let Some(m) = crate::translate::loop_analysis::analyze_filtered_multi_accum(block) {
        notes.detected.push("loop-carried state".into());
        notes.detected.push("multiple accumulators".into());
        if m.predicate.is_some() {
            notes.detected.push("conditional update".into());
        }
        notes.translation.push("foldl over tuple state".into());
        notes.summary = "foldl (…) (…) xs ; trailing expression".into();
        return Some(wrap_setup_lets(
            block,
            lower_filtered_multi_accum(m, ctx),
            ctx,
        ));
    }

    if let Some(scan) = patterns::detect_adjacent_order_scan(block) {
        notes.detected.push("adjacent comparison scan".into());
        notes.detected.push("early boolean exit".into());
        notes.translation.push("and (zipWith …)".into());
        let col = camel(&scan.collection);
        // Fail when v < prev ⇒ require ascending: zipWith (<=)
        let pass_op = match scan.cmp {
            BinOp::Lt => ir::BinOp::Le,
            BinOp::Gt => ir::BinOp::Ge,
            BinOp::Le => ir::BinOp::Lt,
            BinOp::Ge => ir::BinOp::Gt,
            other => lower_binop(other),
        };
        let cmp = ir::Exp::Lam(
            vec!["a".into(), "b".into()],
            Box::new(ir::Exp::BinOp(
                pass_op,
                Box::new(ir::Exp::var("a")),
                Box::new(ir::Exp::var("b")),
            )),
        );
        notes.summary = format!("and (zipWith (…) {col} (tail {col}))");
        return Some(wrap_setup_lets(
            block,
            ir::Exp::app(
                ir::Exp::var("and"),
                vec![ir::Exp::app(
                    ir::Exp::var("zipWith"),
                    vec![
                        cmp,
                        ir::Exp::var(col.clone()),
                        ir::Exp::app(ir::Exp::var("tail"), vec![ir::Exp::var(col)]),
                    ],
                )],
            ),
            ctx,
        ));
    }

    None
}

fn lower_filtered_multi_accum(
    m: crate::translate::loop_analysis::FilteredMultiAccum,
    ctx: &mut LowerCtx,
) -> ir::Exp {
    let init_tuple = ir::Exp::Tuple(m.inits.iter().map(|(_, v)| lower_expr(v, ctx)).collect());
    let acc_names: Vec<String> = m.inits.iter().map(|(n, _)| camel(n)).collect();
    let mut next_vals: IndexMap<String, ir::Exp> = IndexMap::new();
    for n in &acc_names {
        next_vals.insert(n.clone(), ir::Exp::var(n.clone()));
    }
    for (target, op, value) in &m.updates {
        let t = camel(target);
        let mut v = lower_expr(value, ctx);
        v = rename_var(&v, &camel(&m.pat), "x");
        for n in &acc_names {
            v = rename_var(&v, n, n); // no-op keep
        }
        let current = next_vals
            .get(&t)
            .cloned()
            .unwrap_or_else(|| ir::Exp::var(t.clone()));
        next_vals.insert(
            t,
            ir::Exp::BinOp(lower_binop(*op), Box::new(current), Box::new(v)),
        );
    }
    let kept_tuple = ir::Exp::Tuple(acc_names.iter().map(|n| next_vals[n].clone()).collect());
    let skip_tuple = ir::Exp::Tuple(acc_names.iter().map(|n| ir::Exp::var(n.clone())).collect());

    let step_body = if let Some(pred_e) = &m.predicate {
        let mut pred = lower_expr(pred_e, ctx);
        pred = rename_var(&pred, &camel(&m.pat), "x");
        ir::Exp::If(
            Box::new(pred),
            Box::new(kept_tuple),
            Box::new(skip_tuple),
        )
    } else {
        kept_tuple
    };

    let a = &acc_names[0];
    let b = &acc_names[1];
    let destructure = if acc_names.len() == 2 {
        vec![
            ir::Binding {
                name: a.clone(),
                value: ir::Exp::app(ir::Exp::var("fst"), vec![ir::Exp::var("acc")]),
            },
            ir::Binding {
                name: b.clone(),
                value: ir::Exp::app(ir::Exp::var("snd"), vec![ir::Exp::var("acc")]),
            },
        ]
    } else {
        // Fallback: bind whole acc (limited)
        vec![ir::Binding {
            name: "acc".into(),
            value: ir::Exp::var("acc"),
        }]
    };

    let fold_fn = ir::Exp::Lam(
        vec!["acc".into(), "x".into()],
        Box::new(ir::Exp::Let(destructure, Box::new(step_body))),
    );
    let folded = ir::Exp::Fold(
        Box::new(fold_fn),
        Box::new(init_tuple),
        Box::new(ir::Exp::var(camel(&m.collection))),
    );

    // Bind acc names from folded tuple, then evaluate trailing
    let mut trailing = lower_expr(&m.trailing, ctx);
    for n in &acc_names {
        trailing = rename_var(&trailing, n, n);
    }
    if acc_names.len() == 2 {
        ir::Exp::Let(
            vec![
                ir::Binding {
                    name: "state".into(),
                    value: folded,
                },
                ir::Binding {
                    name: a.clone(),
                    value: ir::Exp::app(ir::Exp::var("fst"), vec![ir::Exp::var("state")]),
                },
                ir::Binding {
                    name: b.clone(),
                    value: ir::Exp::app(ir::Exp::var("snd"), vec![ir::Exp::var("state")]),
                },
            ],
            Box::new(trailing),
        )
    } else {
        ir::Exp::Let(
            vec![ir::Binding {
                name: "state".into(),
                value: folded,
            }],
            Box::new(trailing),
        )
    }
}

fn lower_euclid_while(e: patterns::EuclidWhile, ctx: &mut LowerCtx) -> ir::Exp {
    // go a b = if b /= 0 then go b (a `mod` b) else a
    let a0 = match &e.a_init {
        Some(v) => lower_expr(v, ctx),
        None => ir::Exp::var(camel(&e.a)),
    };
    let b0 = match &e.b_init {
        Some(v) => lower_expr(v, ctx),
        None => ir::Exp::var(camel(&e.b)),
    };
    let go_body = ir::Exp::If(
        Box::new(ir::Exp::BinOp(
            ir::BinOp::Ne,
            Box::new(ir::Exp::var("b")),
            Box::new(ir::Exp::Lit(ir::Lit::Int(0))),
        )),
        Box::new(ir::Exp::app(
            ir::Exp::var("go"),
            vec![
                ir::Exp::var("b"),
                ir::Exp::BinOp(
                    ir::BinOp::Rem,
                    Box::new(ir::Exp::var("a")),
                    Box::new(ir::Exp::var("b")),
                ),
            ],
        )),
        Box::new(ir::Exp::var("a")),
    );
    ir::Exp::Let(
        vec![ir::Binding {
            name: "go".into(),
            value: ir::Exp::Lam(vec!["a".into(), "b".into()], Box::new(go_body)),
        }],
        Box::new(ir::Exp::app(ir::Exp::var("go"), vec![a0, b0])),
    )
}

fn lower_continue_break_fold(f: patterns::ContinueBreakFold, ctx: &mut LowerCtx) -> ir::Exp {
    // Use null/head/tail to avoid fragile case-syntax in lambdas:
    // go acc xs = if null xs then acc else let v = head xs; vs = tail xs in …
    let init = lower_expr(&f.acc_init, ctx);
    let mut addend = lower_expr(&f.addend, ctx);
    addend = rename_var(&addend, &camel(&f.pat), "v");
    addend = rename_var(&addend, &camel(&f.acc), "acc");
    let acc_next = ir::Exp::BinOp(
        ir::BinOp::Add,
        Box::new(ir::Exp::var("acc")),
        Box::new(addend),
    );

    let after_add = if let Some(bc) = &f.break_cond {
        let mut bcond = lower_expr(bc, ctx);
        bcond = rename_var(&bcond, &camel(&f.pat), "v");
        bcond = rename_var(&bcond, &camel(&f.acc), "accPrime");
        ir::Exp::Let(
            vec![ir::Binding {
                name: "accPrime".into(),
                value: acc_next.clone(),
            }],
            Box::new(ir::Exp::If(
                Box::new(bcond),
                Box::new(ir::Exp::var("accPrime")),
                Box::new(ir::Exp::app(
                    ir::Exp::var("go"),
                    vec![ir::Exp::var("accPrime"), ir::Exp::var("vs")],
                )),
            )),
        )
    } else {
        ir::Exp::app(ir::Exp::var("go"), vec![acc_next, ir::Exp::var("vs")])
    };

    let with_skip = if let Some(sc) = &f.skip_cond {
        let mut scond = lower_expr(sc, ctx);
        scond = rename_var(&scond, &camel(&f.pat), "v");
        ir::Exp::If(
            Box::new(scond),
            Box::new(ir::Exp::app(
                ir::Exp::var("go"),
                vec![ir::Exp::var("acc"), ir::Exp::var("vs")],
            )),
            Box::new(after_add),
        )
    } else {
        after_add
    };

    let nonempty = ir::Exp::Let(
        vec![
            ir::Binding {
                name: "v".into(),
                value: ir::Exp::app(ir::Exp::var("head"), vec![ir::Exp::var("xs")]),
            },
            ir::Binding {
                name: "vs".into(),
                value: ir::Exp::app(ir::Exp::var("tail"), vec![ir::Exp::var("xs")]),
            },
        ],
        Box::new(with_skip),
    );

    let go = ir::Exp::Lam(
        vec!["acc".into(), "xs".into()],
        Box::new(ir::Exp::If(
            Box::new(ir::Exp::app(ir::Exp::var("null"), vec![ir::Exp::var("xs")])),
            Box::new(ir::Exp::var("acc")),
            Box::new(nonempty),
        )),
    );

    ir::Exp::Let(
        vec![ir::Binding {
            name: "go".into(),
            value: go,
        }],
        Box::new(ir::Exp::app(
            ir::Exp::var("go"),
            vec![init, ir::Exp::var(camel(&f.collection))],
        )),
    )
}

fn promote_mut_update_return(
    body: ir::Exp,
    return_ty: Ty,
    params: &[(String, Ty)],
    notes: &mut TransNotes,
) -> (ir::Exp, Ty) {
    match (&body, &return_ty) {
        (ir::Exp::RecordUpdate { base, .. }, Ty::Unit) => {
            if let ir::Exp::Var(updated) = base.as_ref() {
                if let Some((_, pty)) = params.iter().find(|(n, _)| n == updated) {
                    notes
                        .detected
                        .push("unit &mut update promoted to returned record".into());
                    notes
                        .translation
                        .push("pure record update returning new value".into());
                    return (body, pty.clone());
                }
            }
            (body, return_ty)
        }
        _ => (body, return_ty),
    }
}

fn lower_while_accum(w: patterns::WhileAccum, ctx: &mut LowerCtx) -> ir::Exp {
    // go cnt acc = if cond then go cnt' acc' else acc
    // Use `cnt` (not `n`) so free variables like a function parameter `n` stay intact.
    let n0 = lower_expr(&w.counter_init, ctx);
    let a0 = lower_expr(&w.acc_init, ctx);
    let cond = rename_var(
        &rename_var(&lower_expr(&w.cond, ctx), &camel(&w.counter), "cnt"),
        &camel(&w.acc),
        "acc",
    );
    let mut n_next = ir::Exp::var("cnt");
    let mut a_next = ir::Exp::var("acc");
    for (target, op, val) in &w.updates {
        let v = rename_var(
            &rename_var(&lower_expr(val, ctx), &camel(&w.counter), "cnt"),
            &camel(&w.acc),
            "acc",
        );
        let updated = ir::Exp::BinOp(
            lower_binop(*op),
            Box::new(ir::Exp::var(if target == &w.counter {
                "cnt"
            } else {
                "acc"
            })),
            Box::new(v),
        );
        if target == &w.counter {
            n_next = updated;
        } else {
            a_next = updated;
        }
    }
    let go_body = ir::Exp::If(
        Box::new(cond),
        Box::new(ir::Exp::app(ir::Exp::var("go"), vec![n_next, a_next])),
        Box::new(ir::Exp::var("acc")),
    );
    ir::Exp::Let(
        vec![ir::Binding {
            name: "go".into(),
            value: ir::Exp::Lam(vec!["cnt".into(), "acc".into()], Box::new(go_body)),
        }],
        Box::new(ir::Exp::app(ir::Exp::var("go"), vec![n0, a0])),
    )
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
        ir::Exp::RecordUpdate { base, fields } => ir::Exp::RecordUpdate {
            base: Box::new(rename_var(base, from, to)),
            fields: fields
                .iter()
                .map(|(k, v)| (k.clone(), rename_var(v, from, to)))
                .collect(),
        },
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
                .note("Recognised computations: filter+map builds (if/continue/temps), filtered folds, continue/break folds, max scans, multi-accumulators (incl. filtered), indexed search (elemIndex), early find, adjacent order scans (zipWith).")
                .note("Raskell matches *computations* after normalising loop-local lets and skip/continue — not one AST template. See reports/coverage.md."),
            );
            ir::Exp::Error("unsupported for".into())
        }
        Stmt::Expr(e) => {
            // Early-return if → nested if/else of the remainder
            if let Some((cond, val)) = patterns::is_early_return_if(first) {
                notes.detected.push("early return".into());
                notes.translation.push("nested if/else".into());
                let then_e = lower_expr(&val, ctx);
                let else_e = lower_stmts(rest, trailing, ctx, notes);
                return ir::Exp::If(
                    Box::new(lower_expr(&cond, ctx)),
                    Box::new(then_e),
                    Box::new(else_e),
                );
            }
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
                    ).note("Recognised while patterns: countdown/counting accumulators (→ go), Euclidean remainder loops (→ recursive go).")
                    .note("Other while loops need a clear functional interpretation; prefer recursion or for when possible."));
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
            // String::from(x) / From::from(x) → x (Haskell String is already the value)
            if let Expr::Path(p, _) = func.as_ref() {
                if p.ends_with("::from") || p == "from" {
                    return args
                        .first()
                        .map(|a| lower_expr(a, ctx))
                        .unwrap_or(ir::Exp::Lit(ir::Lit::Str(String::new())));
                }
            }

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
                    "id" if as_.len() == 1 => as_[0].clone(),
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
                    "replicate" => {
                        // Rust [expr; n] was converted as replicate(n, expr);
                        // Haskell replicate :: Int -> a -> [a]
                        ir::Exp::app(ir::Exp::var("replicate"), as_)
                    }
                    "putStrLn" | "println" => ir::Exp::app(ir::Exp::var("putStrLn"), as_),
                    "putStr" | "print" => ir::Exp::app(ir::Exp::var("putStr"), as_),
                    "show" => ir::Exp::app(ir::Exp::var("show"), as_),
                    "strAppend" if as_.len() == 2 => ir::Exp::BinOp(
                        ir::BinOp::Append,
                        Box::new(as_[0].clone()),
                        Box::new(as_[1].clone()),
                    ),
                    "abs" => ir::Exp::app(ir::Exp::var("abs"), as_),
                    "min" if as_.len() == 2 => ir::Exp::app(ir::Exp::var("min"), as_),
                    "max" if as_.len() == 2 => ir::Exp::app(ir::Exp::var("max"), as_),
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
            "len" | "length" => {
                ir::Exp::app(ir::Exp::var("length"), vec![lower_expr(receiver, ctx)])
            }
            "is_empty" => ir::Exp::app(ir::Exp::var("null"), vec![lower_expr(receiver, ctx)]),
            "clone" | "to_owned" | "to_string" | "as_str" | "into" | "to_vec" => {
                lower_expr(receiver, ctx)
            }
            "chars" => lower_expr(receiver, ctx), // String ≅ [Char]
            "lines" => ir::Exp::app(ir::Exp::var("lines"), vec![lower_expr(receiver, ctx)]),
            "trim" => {
                // words then unwords approximates whitespace trim for simple cases
                ir::Exp::app(
                    ir::Exp::var("unwords"),
                    vec![ir::Exp::app(
                        ir::Exp::var("words"),
                        vec![lower_expr(receiver, ctx)],
                    )],
                )
            }
            "contains" => {
                let needle = args
                    .first()
                    .map(|a| lower_expr(a, ctx))
                    .unwrap_or(ir::Exp::Lit(ir::Lit::Str(String::new())));
                ir::Exp::app(
                    ir::Exp::var("isInfixOf"),
                    vec![needle, lower_expr(receiver, ctx)],
                )
            }
            "starts_with" => {
                let prefix = args
                    .first()
                    .map(|a| lower_expr(a, ctx))
                    .unwrap_or(ir::Exp::Lit(ir::Lit::Str(String::new())));
                ir::Exp::app(
                    ir::Exp::var("isPrefixOf"),
                    vec![prefix, lower_expr(receiver, ctx)],
                )
            }
            "ends_with" => {
                let suffix = args
                    .first()
                    .map(|a| lower_expr(a, ctx))
                    .unwrap_or(ir::Exp::Lit(ir::Lit::Str(String::new())));
                ir::Exp::app(
                    ir::Exp::var("isSuffixOf"),
                    vec![suffix, lower_expr(receiver, ctx)],
                )
            }
            "push_str" => {
                let v = args
                    .first()
                    .map(|a| lower_expr(a, ctx))
                    .unwrap_or(ir::Exp::Lit(ir::Lit::Str(String::new())));
                ir::Exp::BinOp(
                    ir::BinOp::Append,
                    Box::new(lower_expr(receiver, ctx)),
                    Box::new(v),
                )
            }
            "is_some" => ir::Exp::app(ir::Exp::var("isJust"), vec![lower_expr(receiver, ctx)]),
            "is_none" => {
                ir::Exp::app(ir::Exp::var("isNothing"), vec![lower_expr(receiver, ctx)])
            }
            "is_ok" => ir::Exp::app(ir::Exp::var("isRight"), vec![lower_expr(receiver, ctx)]),
            "is_err" => ir::Exp::app(ir::Exp::var("isLeft"), vec![lower_expr(receiver, ctx)]),
            "unwrap" => {
                ir::Exp::app(ir::Exp::var("fromJust"), vec![lower_expr(receiver, ctx)])
            }
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
            "unwrap_or_else" => {
                let f = args
                    .first()
                    .map(|a| lower_closure_or_expr(a, ctx))
                    .unwrap_or(ir::Exp::var("id"));
                // fromMaybe (f ()) m  — approximates lazy default for ()-taking closures
                ir::Exp::app(
                    ir::Exp::var("fromMaybe"),
                    vec![
                        ir::Exp::app(f, vec![ir::Exp::Lit(ir::Lit::Unit)]),
                        lower_expr(receiver, ctx),
                    ],
                )
            }
            "ok" => {
                // Result::ok → Either → Maybe via either (const Nothing) Just
                ir::Exp::app(
                    ir::Exp::var("either"),
                    vec![
                        ir::Exp::app(ir::Exp::var("const"), vec![ir::Exp::var("Nothing")]),
                        ir::Exp::var("Just"),
                        lower_expr(receiver, ctx),
                    ],
                )
            }
            "ok_or" => {
                let err = args
                    .first()
                    .map(|a| lower_expr(a, ctx))
                    .unwrap_or(ir::Exp::Lit(ir::Lit::Unit));
                ir::Exp::app(
                    ir::Exp::var("maybe"),
                    vec![
                        ir::Exp::app(ir::Exp::var("Left"), vec![err]),
                        ir::Exp::var("Right"),
                        lower_expr(receiver, ctx),
                    ],
                )
            }
            "and_then" => {
                let f = args
                    .first()
                    .map(|a| lower_closure_or_expr(a, ctx))
                    .unwrap_or(ir::Exp::var("return"));
                ir::Exp::app(
                    ir::Exp::var("(>>=)"),
                    vec![lower_expr(receiver, ctx), f],
                )
            }
            "or_else" => {
                let f = args
                    .first()
                    .map(|a| lower_closure_or_expr(a, ctx))
                    .unwrap_or(ir::Exp::var("return"));
                // For Either: either f Right
                ir::Exp::app(
                    ir::Exp::var("either"),
                    vec![f, ir::Exp::var("Right"), lower_expr(receiver, ctx)],
                )
            }
            "map_err" => {
                // either (Left . f) Right e
                let f = args
                    .first()
                    .map(|a| lower_closure_or_expr(a, ctx))
                    .unwrap_or(ir::Exp::var("id"));
                let left_f = ir::Exp::app(
                    ir::Exp::var("(.)"),
                    vec![ir::Exp::var("Left"), f],
                );
                ir::Exp::app(
                    ir::Exp::var("either"),
                    vec![left_f, ir::Exp::var("Right"), lower_expr(receiver, ctx)],
                )
            }
            "abs" => ir::Exp::app(ir::Exp::var("abs"), vec![lower_expr(receiver, ctx)]),
            "pow" => {
                let n = args
                    .first()
                    .map(|a| lower_expr(a, ctx))
                    .unwrap_or(ir::Exp::int(1));
                ir::Exp::app(ir::Exp::var("(^)"), vec![lower_expr(receiver, ctx), n])
            }
            "push" => {
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
            "new" => ir::Exp::List(vec![]),
            "next" => {
                ir::Exp::app(ir::Exp::var("listToMaybe"), vec![lower_expr(receiver, ctx)])
            }
            other => {
                // Typeclass / inherent methods → function application (greet x)
                let mut call_args = vec![lower_expr(receiver, ctx)];
                call_args.extend(args.iter().map(|a| lower_expr(a, ctx)));
                let _ = span;
                ir::Exp::app(ir::Exp::var(camel(other)), call_args)
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
            // Standalone `?` outside a try-block rewrite — emit bind-friendly form.
            ctx.diags.push(
                Diagnostic::warning(
                    "W0001",
                    "`?` used outside a recognised fallible-function pattern",
                )
                .at(&ctx.filename, span.line, span.column)
                .note("Raskell rewrites whole functions that use `?` into do-notation."),
            );
            lower_expr(e, ctx)
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
            "find" => {
                let p = step
                    .args
                    .first()
                    .map(|a| lower_closure_or_expr(a, ctx))
                    .unwrap_or(ir::Exp::var("const True"));
                // Data.List.find :: (a -> Bool) -> [a] -> Maybe a
                exp = ir::Exp::app(ir::Exp::var("find"), vec![p, exp]);
            }
            "position" => {
                let p = step
                    .args
                    .first()
                    .map(|a| lower_closure_or_expr(a, ctx))
                    .unwrap_or(ir::Exp::var("const True"));
                exp = ir::Exp::app(ir::Exp::var("findIndex"), vec![p, exp]);
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
                exp = ir::Exp::app(ir::Exp::var("mapMaybe"), vec![f, exp]);
            }
            "enumerate" => {
                exp = ir::Exp::app(ir::Exp::var("zip"), vec![
                    ir::Exp::app(ir::Exp::var("enumFrom"), vec![ir::Exp::int(0)]),
                    exp,
                ]);
            }
            "zip" => {
                let other = step
                    .args
                    .first()
                    .map(|a| lower_expr(a, ctx))
                    .unwrap_or(ir::Exp::List(vec![]));
                exp = ir::Exp::app(ir::Exp::var("zip"), vec![exp, other]);
            }
            "chain" => {
                let other = step
                    .args
                    .first()
                    .map(|a| lower_expr(a, ctx))
                    .unwrap_or(ir::Exp::List(vec![]));
                exp = ir::Exp::BinOp(
                    ir::BinOp::Append,
                    Box::new(exp),
                    Box::new(other),
                );
            }
            "flatten" | "flat_map" => {
                if step.method == "flat_map" {
                    let f = step
                        .args
                        .first()
                        .map(|a| lower_closure_or_expr(a, ctx))
                        .unwrap_or(ir::Exp::var("id"));
                    exp = ir::Exp::app(
                        ir::Exp::var("concatMap"),
                        vec![f, exp],
                    );
                } else {
                    exp = ir::Exp::app(ir::Exp::var("concat"), vec![exp]);
                }
            }
            "partition" => {
                let p = step
                    .args
                    .first()
                    .map(|a| lower_closure_or_expr(a, ctx))
                    .unwrap_or(ir::Exp::var("const True"));
                exp = ir::Exp::app(ir::Exp::var("partition"), vec![p, exp]);
            }
            "max" => {
                exp = ir::Exp::app(ir::Exp::var("maximum"), vec![exp]);
            }
            "min" => {
                exp = ir::Exp::app(ir::Exp::var("minimum"), vec![exp]);
            }
            "product" => {
                exp = ir::Exp::app(ir::Exp::var("product"), vec![exp]);
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
            "self" => ir::Exp::var("self_"),
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
            // Note: `(>>=)` alone is not IO — Maybe/Either also use it.
            ir::Exp::Var(v) => {
                matches!(v.as_str(), "putStrLn" | "putStr" | "getLine" | "print" | "(>>)")
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
