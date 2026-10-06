//! Convert `syn` AST → Raskell AST.

use crate::ast::*;
use crate::diagnostics::{Diagnostic, Diagnostics};
use proc_macro2::Span as PmSpan;
use syn::spanned::Spanned;
use syn::{self, FnArg, Item as SynItem, Pat, ReturnType, Visibility};

pub fn line_col(span: PmSpan) -> (usize, usize) {
    let start = span.start();
    (start.line, start.column)
}

fn span_of(span: PmSpan) -> Span {
    let (line, column) = line_col(span);
    Span::new(line, column)
}

pub fn convert_file(file: &mut syn::File, filename: &str) -> (Program, Diagnostics) {
    let mut diags = Diagnostics::new();
    let mut items = Vec::new();

    for item in &file.items {
        match convert_item(item, filename, &mut diags) {
            Some(i) => items.push(i),
            None => {}
        }
    }

    (
        Program {
            filename: filename.to_string(),
            items,
        },
        diags,
    )
}

fn convert_item(item: &SynItem, filename: &str, diags: &mut Diagnostics) -> Option<Item> {
    match item {
        SynItem::Fn(f) => Some(Item::Function(convert_fn(f, filename, diags))),
        SynItem::Struct(s) => Some(Item::Struct(convert_struct(s, filename, diags))),
        SynItem::Enum(e) => Some(Item::Enum(convert_enum(e, filename, diags))),
        SynItem::Const(c) => Some(Item::Const(convert_const(c, filename, diags))),
        SynItem::Use(u) => {
            let path = path_to_string(&u.tree);
            Some(Item::Use(UseItem {
                path,
                span: span_of(u.span()),
            }))
        }
        SynItem::Mod(m) => {
            let (line, col) = line_col(m.span());
            diags.push(
                Diagnostic::unsupported("module declarations", filename, line, col)
                    .note("Put everything in a single file for now."),
            );
            None
        }
        SynItem::Impl(i) => Some(Item::Impl(convert_impl(i, filename, diags))),
        SynItem::Trait(t) => Some(Item::Trait(convert_trait(t, filename, diags))),
        SynItem::Macro(m) => {
            let (line, col) = line_col(m.span());
            diags.push(Diagnostic::unsupported("macros", filename, line, col));
            None
        }
        SynItem::ForeignMod(f) => {
            let (line, col) = line_col(f.span());
            diags.push(Diagnostic::unsupported("FFI / extern blocks", filename, line, col));
            None
        }
        SynItem::Static(s) => {
            let (line, col) = line_col(s.span());
            diags.push(Diagnostic::unsupported("static items", filename, line, col));
            None
        }
        SynItem::Type(t) => {
            let (line, col) = line_col(t.span());
            diags.push(Diagnostic::unsupported("type aliases", filename, line, col));
            None
        }
        SynItem::Union(u) => {
            let (line, col) = line_col(u.span());
            diags.push(Diagnostic::unsupported("unions", filename, line, col));
            None
        }
        other => {
            let (line, col) = line_col(other.span());
            diags.push(Diagnostic::unsupported(
                "this top-level item",
                filename,
                line,
                col,
            ));
            None
        }
    }
}

fn path_to_string(tree: &syn::UseTree) -> String {
    match tree {
        syn::UseTree::Path(p) => format!("{}::{}", p.ident, path_to_string(&p.tree)),
        syn::UseTree::Name(n) => n.ident.to_string(),
        syn::UseTree::Rename(r) => format!("{} as {}", r.ident, r.rename),
        syn::UseTree::Glob(_) => "*".into(),
        syn::UseTree::Group(g) => {
            let parts: Vec<_> = g.items.iter().map(path_to_string).collect();
            format!("{{{}}}", parts.join(", "))
        }
    }
}

fn is_pub(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

fn convert_fn(f: &syn::ItemFn, filename: &str, diags: &mut Diagnostics) -> Function {
    // Reject unsafe / async / const fn
    if f.sig.unsafety.is_some() {
        let (line, col) = line_col(f.sig.unsafety.span());
        diags.push(Diagnostic::unsupported("unsafe fn", filename, line, col));
    }
    if f.sig.asyncness.is_some() {
        let (line, col) = line_col(f.sig.asyncness.span());
        diags.push(
            Diagnostic::unsupported("async fn", filename, line, col)
                .note("Async/await is on the roadmap."),
        );
    }

    let (generics, bounds) = convert_generics(&f.sig.generics);
    let params = convert_fn_args(&f.sig.inputs, filename, diags);

    let return_type = match &f.sig.output {
        ReturnType::Default => None,
        ReturnType::Type(_, ty) => Some(convert_type(ty, filename, diags)),
    };

    Function {
        name: f.sig.ident.to_string(),
        params,
        return_type,
        body: convert_block(&f.block, filename, diags),
        is_pub: is_pub(&f.vis),
        generics,
        bounds,
        span: span_of(f.span()),
    }
}

fn convert_generics(generics: &syn::Generics) -> (Vec<String>, Vec<(String, Vec<String>)>) {
    let names: Vec<String> = generics
        .type_params()
        .map(|p| p.ident.to_string())
        .collect();
    let mut bounds: Vec<(String, Vec<String>)> = Vec::new();
    for p in generics.type_params() {
        let mut bs = Vec::new();
        for b in &p.bounds {
            if let syn::TypeParamBound::Trait(t) = b {
                if let Some(seg) = t.path.segments.last() {
                    bs.push(seg.ident.to_string());
                }
            }
        }
        if !bs.is_empty() {
            bounds.push((p.ident.to_string(), bs));
        }
    }
    if let Some(where_clause) = &generics.where_clause {
        for pred in &where_clause.predicates {
            if let syn::WherePredicate::Type(tp) = pred {
                if let syn::Type::Path(path) = &tp.bounded_ty {
                    if let Some(seg) = path.path.segments.last() {
                        let name = seg.ident.to_string();
                        let mut bs = Vec::new();
                        for b in &tp.bounds {
                            if let syn::TypeParamBound::Trait(t) = b {
                                if let Some(s) = t.path.segments.last() {
                                    bs.push(s.ident.to_string());
                                }
                            }
                        }
                        if !bs.is_empty() {
                            bounds.push((name, bs));
                        }
                    }
                }
            }
        }
    }
    (names, bounds)
}

fn convert_fn_args(
    inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
    filename: &str,
    diags: &mut Diagnostics,
) -> Vec<Param> {
    inputs
        .iter()
        .map(|arg| match arg {
            FnArg::Typed(pat_ty) => {
                let (name, is_mut, by_ref) = pat_name(&pat_ty.pat);
                Param {
                    name,
                    ty: convert_type(&pat_ty.ty, filename, diags),
                    is_mut,
                    by_ref,
                    is_self: false,
                    span: span_of(pat_ty.span()),
                }
            }
            FnArg::Receiver(r) => {
                let by_ref = r.reference.is_some();
                let is_mut = r.mutability.is_some();
                Param {
                    name: "self".into(),
                    ty: if by_ref {
                        Type::Ref {
                            is_mut,
                            inner: Box::new(Type::SelfType),
                        }
                    } else {
                        Type::SelfType
                    },
                    is_mut,
                    by_ref,
                    is_self: true,
                    span: span_of(r.span()),
                }
            }
        })
        .collect()
}

fn convert_trait(t: &syn::ItemTrait, filename: &str, diags: &mut Diagnostics) -> TraitDef {
    let (generics, _) = convert_generics(&t.generics);
    let mut methods = Vec::new();
    for item in &t.items {
        match item {
            syn::TraitItem::Fn(m) => {
                let (mg, mb) = convert_generics(&m.sig.generics);
                let params = convert_fn_args(&m.sig.inputs, filename, diags);
                let return_type = match &m.sig.output {
                    ReturnType::Default => None,
                    ReturnType::Type(_, ty) => Some(convert_type(ty, filename, diags)),
                };
                let default_body = m
                    .default
                    .as_ref()
                    .map(|b| convert_block(b, filename, diags));
                methods.push(TraitMethod {
                    name: m.sig.ident.to_string(),
                    params,
                    return_type,
                    generics: mg,
                    bounds: mb,
                    default_body,
                    span: span_of(m.span()),
                });
            }
            other => {
                let (line, col) = line_col(other.span());
                diags.push(
                    Diagnostic::unsupported("non-method trait items", filename, line, col)
                        .note("Raskell currently supports method signatures in traits."),
                );
            }
        }
    }
    TraitDef {
        name: t.ident.to_string(),
        generics,
        methods,
        is_pub: is_pub(&t.vis),
        span: span_of(t.span()),
    }
}

fn convert_impl(i: &syn::ItemImpl, filename: &str, diags: &mut Diagnostics) -> ImplBlock {
    if i.unsafety.is_some() {
        let (line, col) = line_col(i.unsafety.span());
        diags.push(Diagnostic::unsupported("unsafe impl", filename, line, col));
    }
    let trait_name = i.trait_.as_ref().map(|(_, path, _)| {
        path.segments
            .last()
            .map(|s| s.ident.to_string())
            .unwrap_or_else(|| "Unknown".into())
    });
    let for_type = convert_type(&i.self_ty, filename, diags);
    let mut methods = Vec::new();
    for item in &i.items {
        match item {
            syn::ImplItem::Fn(m) => {
                // Reuse convert_fn shape via a synthetic ItemFn-like path
                let (generics, bounds) = convert_generics(&m.sig.generics);
                if m.sig.asyncness.is_some() {
                    let (line, col) = line_col(m.sig.asyncness.span());
                    diags.push(Diagnostic::unsupported("async methods", filename, line, col));
                }
                let params = convert_fn_args(&m.sig.inputs, filename, diags);
                let return_type = match &m.sig.output {
                    ReturnType::Default => None,
                    ReturnType::Type(_, ty) => Some(convert_type(ty, filename, diags)),
                };
                methods.push(Function {
                    name: m.sig.ident.to_string(),
                    params,
                    return_type,
                    body: convert_block(&m.block, filename, diags),
                    is_pub: is_pub(&m.vis),
                    generics,
                    bounds,
                    span: span_of(m.span()),
                });
            }
            other => {
                let (line, col) = line_col(other.span());
                diags.push(Diagnostic::unsupported(
                    "non-method impl items",
                    filename,
                    line,
                    col,
                ));
            }
        }
    }
    ImplBlock {
        trait_name,
        for_type,
        methods,
        span: span_of(i.span()),
    }
}

fn pat_name(pat: &Pat) -> (String, bool, bool) {
    match pat {
        Pat::Ident(i) => (i.ident.to_string(), i.mutability.is_some(), false),
        Pat::Reference(r) => {
            let (n, m, _) = pat_name(&r.pat);
            (n, m, true)
        }
        Pat::Wild(_) => ("_".into(), false, false),
        other => (format!("/*pat:{other:?}*/").chars().take(40).collect(), false, false),
    }
}

fn convert_struct(s: &syn::ItemStruct, filename: &str, diags: &mut Diagnostics) -> StructDef {
    let fields = match &s.fields {
        syn::Fields::Named(n) => n
            .named
            .iter()
            .map(|f| Field {
                name: f.ident.as_ref().unwrap().to_string(),
                ty: convert_type(&f.ty, filename, diags),
                is_pub: is_pub(&f.vis),
                span: span_of(f.span()),
            })
            .collect(),
        syn::Fields::Unnamed(u) => {
            let (line, col) = line_col(u.span());
            diags.push(Diagnostic::unsupported("tuple structs", filename, line, col));
            Vec::new()
        }
        syn::Fields::Unit => Vec::new(),
    };
    let generics: Vec<String> = s
        .generics
        .type_params()
        .map(|p| p.ident.to_string())
        .collect();
    StructDef {
        name: s.ident.to_string(),
        fields,
        generics,
        is_pub: is_pub(&s.vis),
        span: span_of(s.span()),
    }
}

fn convert_enum(e: &syn::ItemEnum, filename: &str, diags: &mut Diagnostics) -> EnumDef {
    let variants = e
        .variants
        .iter()
        .map(|v| {
            let fields = match &v.fields {
                syn::Fields::Unit => VariantFields::Unit,
                syn::Fields::Unnamed(u) => VariantFields::Tuple(
                    u.unnamed
                        .iter()
                        .map(|f| convert_type(&f.ty, filename, diags))
                        .collect(),
                ),
                syn::Fields::Named(n) => VariantFields::Struct(
                    n.named
                        .iter()
                        .map(|f| Field {
                            name: f.ident.as_ref().unwrap().to_string(),
                            ty: convert_type(&f.ty, filename, diags),
                            is_pub: is_pub(&f.vis),
                            span: span_of(f.span()),
                        })
                        .collect(),
                ),
            };
            Variant {
                name: v.ident.to_string(),
                fields,
                span: span_of(v.span()),
            }
        })
        .collect();
    let generics: Vec<String> = e
        .generics
        .type_params()
        .map(|p| p.ident.to_string())
        .collect();
    EnumDef {
        name: e.ident.to_string(),
        variants,
        generics,
        is_pub: is_pub(&e.vis),
        span: span_of(e.span()),
    }
}

fn convert_const(c: &syn::ItemConst, filename: &str, diags: &mut Diagnostics) -> ConstDef {
    ConstDef {
        name: c.ident.to_string(),
        ty: convert_type(&c.ty, filename, diags),
        value: convert_expr(&c.expr, filename, diags),
        span: span_of(c.span()),
    }
}

fn convert_type(ty: &syn::Type, filename: &str, diags: &mut Diagnostics) -> Type {
    match ty {
        syn::Type::Path(p) => {
            let seg = p.path.segments.last().unwrap();
            let name = seg.ident.to_string();
            match name.as_str() {
                "Vec" => {
                    if let syn::PathArguments::AngleBracketed(a) = &seg.arguments {
                        if let Some(syn::GenericArgument::Type(t)) = a.args.first() {
                            return Type::Vec(Box::new(convert_type(t, filename, diags)));
                        }
                    }
                    Type::Named("Vec".into(), vec![])
                }
                "Option" => {
                    if let syn::PathArguments::AngleBracketed(a) = &seg.arguments {
                        if let Some(syn::GenericArgument::Type(t)) = a.args.first() {
                            return Type::Option(Box::new(convert_type(t, filename, diags)));
                        }
                    }
                    Type::Named("Option".into(), vec![])
                }
                "Result" => {
                    if let syn::PathArguments::AngleBracketed(a) = &seg.arguments {
                        let mut args = a.args.iter().filter_map(|g| match g {
                            syn::GenericArgument::Type(t) => Some(convert_type(t, filename, diags)),
                            _ => None,
                        });
                        if let (Some(ok), Some(err)) = (args.next(), args.next()) {
                            return Type::Result(Box::new(ok), Box::new(err));
                        }
                    }
                    Type::Named("Result".into(), vec![])
                }
                "Self" => Type::SelfType,
                "String" | "str" | "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8"
                | "u16" | "u32" | "u64" | "u128" | "usize" | "f32" | "f64" | "bool" | "char" => {
                    Type::Named(name, vec![])
                }
                other => {
                    let args = if let syn::PathArguments::AngleBracketed(a) = &seg.arguments {
                        a.args
                            .iter()
                            .filter_map(|g| match g {
                                syn::GenericArgument::Type(t) => {
                                    Some(convert_type(t, filename, diags))
                                }
                                _ => None,
                            })
                            .collect()
                    } else {
                        vec![]
                    };
                    // Single-letter uppercase type params (T, E, A, …) → Generic.
                    // Longer names are concrete types (possibly with arguments).
                    if args.is_empty()
                        && other.len() == 1
                        && other.chars().next().is_some_and(|c| c.is_uppercase())
                    {
                        Type::Generic(other.to_string())
                    } else {
                        Type::Named(other.to_string(), args)
                    }
                }
            }
        }
        syn::Type::Reference(r) => Type::Ref {
            is_mut: r.mutability.is_some(),
            inner: Box::new(convert_type(&r.elem, filename, diags)),
        },
        syn::Type::Tuple(t) => {
            if t.elems.is_empty() {
                Type::Unit
            } else {
                Type::Tuple(
                    t.elems
                        .iter()
                        .map(|e| convert_type(e, filename, diags))
                        .collect(),
                )
            }
        }
        syn::Type::Slice(s) => Type::Array(Box::new(convert_type(&s.elem, filename, diags))),
        syn::Type::Array(a) => Type::Array(Box::new(convert_type(&a.elem, filename, diags))),
        syn::Type::BareFn(f) => {
            let params = f
                .inputs
                .iter()
                .map(|i| convert_type(&i.ty, filename, diags))
                .collect();
            let ret = match &f.output {
                ReturnType::Default => Type::Unit,
                ReturnType::Type(_, t) => convert_type(t, filename, diags),
            };
            Type::Fun {
                params,
                ret: Box::new(ret),
            }
        }
        syn::Type::Infer(_) => Type::Infer,
        other => {
            let (line, col) = line_col(other.span());
            diags.push(Diagnostic::unsupported(
                "this type",
                filename,
                line,
                col,
            ));
            Type::Named("UNSUPPORTED".into(), vec![])
        }
    }
}

fn convert_block(block: &syn::Block, filename: &str, diags: &mut Diagnostics) -> Block {
    let mut stmts = Vec::new();
    let mut trailing = None;

    let len = block.stmts.len();
    for (i, stmt) in block.stmts.iter().enumerate() {
        let is_last = i + 1 == len;
        match stmt {
            syn::Stmt::Local(local) => {
                let (name, is_mut, _) = pat_name(&local.pat);
                let ty = match &local.pat {
                    Pat::Type(pt) => Some(convert_type(&pt.ty, filename, diags)),
                    _ => None,
                };
                // Handle Pat::Type wrapping Ident
                let (name, is_mut) = match &local.pat {
                    Pat::Type(pt) => {
                        let (n, m, _) = pat_name(&pt.pat);
                        (n, m)
                    }
                    _ => (name, is_mut),
                };
                let value = local
                    .init
                    .as_ref()
                    .map(|init| convert_expr(&init.expr, filename, diags));
                stmts.push(Stmt::Let {
                    name,
                    is_mut,
                    ty,
                    value,
                    span: span_of(local.span()),
                });
            }
            syn::Stmt::Expr(expr, semi) => {
                if is_last && semi.is_none() {
                    trailing = Some(Box::new(convert_expr(expr, filename, diags)));
                } else {
                    // Check for return/break/continue as expressions
                    match expr {
                        syn::Expr::Return(r) => {
                            stmts.push(Stmt::Return(
                                r.expr.as_ref().map(|e| convert_expr(e, filename, diags)),
                                span_of(r.span()),
                            ));
                        }
                        syn::Expr::Break(b) => {
                            stmts.push(Stmt::Break(span_of(b.span())));
                        }
                        syn::Expr::Continue(c) => {
                            stmts.push(Stmt::Continue(span_of(c.span())));
                        }
                        _ => stmts.push(Stmt::Expr(convert_expr(expr, filename, diags))),
                    }
                }
            }
            syn::Stmt::Item(item) => {
                let (line, col) = line_col(item.span());
                diags.push(Diagnostic::unsupported(
                    "nested items in blocks",
                    filename,
                    line,
                    col,
                ));
            }
            syn::Stmt::Macro(m) => {
                let name = path_expr_string(&m.mac.path);
                if name == "vec"
                    || matches!(name.as_str(), "println" | "print" | "eprintln" | "eprint" | "format")
                {
                    let expr = convert_expr(
                        &syn::Expr::Macro(syn::ExprMacro {
                            attrs: vec![],
                            mac: m.mac.clone(),
                        }),
                        filename,
                        diags,
                    );
                    if is_last && m.semi_token.is_none() {
                        trailing = Some(Box::new(expr));
                    } else {
                        stmts.push(Stmt::Expr(expr));
                    }
                } else {
                    let (line, col) = line_col(m.span());
                    diags.push(
                        Diagnostic::unsupported(&format!("macro `{name}!`"), filename, line, col)
                            .note("Supported macros: vec!, println!, print!, format!."),
                    );
                }
            }
        }
    }

    Block {
        stmts,
        expr: trailing,
        span: span_of(block.span()),
    }
}

pub fn convert_expr(expr: &syn::Expr, filename: &str, diags: &mut Diagnostics) -> Expr {
    match expr {
        syn::Expr::Lit(l) => Expr::Lit(convert_lit(&l.lit), span_of(l.span())),
        syn::Expr::Path(p) => Expr::Path(path_expr_string(&p.path), span_of(p.span())),
        syn::Expr::Field(f) => {
            let field = match &f.member {
                syn::Member::Named(id) => id.to_string(),
                syn::Member::Unnamed(i) => i.index.to_string(),
            };
            Expr::Field {
                base: Box::new(convert_expr(&f.base, filename, diags)),
                field,
                span: span_of(f.span()),
            }
        }
        syn::Expr::Index(i) => Expr::Index {
            base: Box::new(convert_expr(&i.expr, filename, diags)),
            index: Box::new(convert_expr(&i.index, filename, diags)),
            span: span_of(i.span()),
        },
        syn::Expr::Call(c) => Expr::Call {
            func: Box::new(convert_expr(&c.func, filename, diags)),
            args: c
                .args
                .iter()
                .map(|a| convert_expr(a, filename, diags))
                .collect(),
            span: span_of(c.span()),
        },
        syn::Expr::MethodCall(m) => Expr::MethodCall {
            receiver: Box::new(convert_expr(&m.receiver, filename, diags)),
            method: m.method.to_string(),
            args: m
                .args
                .iter()
                .map(|a| convert_expr(a, filename, diags))
                .collect(),
            span: span_of(m.span()),
        },
        syn::Expr::Binary(b) => {
            // Compound assignment (+=, *=, ...) is Expr::Binary with *Assign ops in syn 2.
            if let Some(op) = assign_op_of(&b.op) {
                return Expr::AssignOp {
                    op,
                    target: Box::new(convert_expr(&b.left, filename, diags)),
                    value: Box::new(convert_expr(&b.right, filename, diags)),
                    span: span_of(b.span()),
                };
            }
            let op = convert_binop(&b.op, filename, diags);
            Expr::Binary {
                op,
                left: Box::new(convert_expr(&b.left, filename, diags)),
                right: Box::new(convert_expr(&b.right, filename, diags)),
                span: span_of(b.span()),
            }
        }
        syn::Expr::Unary(u) => {
            let op = match &u.op {
                syn::UnOp::Neg(_) => UnOp::Neg,
                syn::UnOp::Not(_) => UnOp::Not,
                syn::UnOp::Deref(_) => {
                    return Expr::Deref {
                        expr: Box::new(convert_expr(&u.expr, filename, diags)),
                        span: span_of(u.span()),
                    };
                }
                _ => {
                    let (line, col) = line_col(u.span());
                    diags.push(Diagnostic::unsupported("this unary operator", filename, line, col));
                    UnOp::Not
                }
            };
            Expr::Unary {
                op,
                expr: Box::new(convert_expr(&u.expr, filename, diags)),
                span: span_of(u.span()),
            }
        }
        syn::Expr::If(i) => {
            let else_branch = i.else_branch.as_ref().map(|(_, e)| Box::new(convert_expr(e, filename, diags)));
            Expr::If {
                cond: Box::new(convert_expr(&i.cond, filename, diags)),
                then_branch: convert_block(&i.then_branch, filename, diags),
                else_branch,
                span: span_of(i.span()),
            }
        }
        syn::Expr::Match(m) => Expr::Match {
            scrutinee: Box::new(convert_expr(&m.expr, filename, diags)),
            arms: m
                .arms
                .iter()
                .map(|arm| MatchArm {
                    pattern: convert_pat(&arm.pat, filename, diags),
                    guard: arm
                        .guard
                        .as_ref()
                        .map(|(_, g)| convert_expr(g, filename, diags)),
                    body: convert_expr(&arm.body, filename, diags),
                    span: span_of(arm.span()),
                })
                .collect(),
            span: span_of(m.span()),
        },
        syn::Expr::Block(b) => {
            if b.label.is_some() {
                let (line, col) = line_col(b.span());
                diags.push(Diagnostic::unsupported("labeled blocks", filename, line, col));
            }
            Expr::Block(convert_block(&b.block, filename, diags))
        }
        syn::Expr::Closure(c) => {
            if c.asyncness.is_some() || c.movability.is_some() {
                let (line, col) = line_col(c.span());
                diags.push(Diagnostic::unsupported("async/move closures", filename, line, col));
            }
            // Support simple tuple patterns: |(a, b)| …
            let mut params: Vec<String> = Vec::new();
            let mut body = convert_expr(&c.body, filename, diags);
            for p in &c.inputs {
                match p {
                    Pat::Tuple(t) => {
                        let names: Vec<String> =
                            t.elems.iter().map(|e| pat_name(e).0).collect();
                        // Encode as single param + destructure via nested lambdas later;
                        // use synthetic name and rewrite body to project fields.
                        let syn_name = format!("tup{}", params.len());
                        // Replace uses… keep simple: flatten to multiple params if body only uses names
                        for (i, n) in names.iter().enumerate() {
                            let _ = i;
                            params.push(n.clone());
                        }
                        let _ = syn_name;
                        let _ = &mut body;
                    }
                    _ => params.push(pat_name(p).0),
                }
            }
            Expr::Closure {
                params,
                body: Box::new(body),
                span: span_of(c.span()),
            }
        }
        syn::Expr::Tuple(t) => Expr::Tuple(
            t.elems
                .iter()
                .map(|e| convert_expr(e, filename, diags))
                .collect(),
            span_of(t.span()),
        ),
        syn::Expr::Array(a) => Expr::Array(
            a.elems
                .iter()
                .map(|e| convert_expr(e, filename, diags))
                .collect(),
            span_of(a.span()),
        ),
        syn::Expr::Struct(s) => {
            let name = path_expr_string(&s.path);
            let fields = s
                .fields
                .iter()
                .filter_map(|f| match &f.member {
                    syn::Member::Named(id) => {
                        Some((id.to_string(), convert_expr(&f.expr, filename, diags)))
                    }
                    _ => None,
                })
                .collect();
            if s.rest.is_some() {
                let (line, col) = line_col(s.span());
                diags.push(Diagnostic::unsupported("struct update syntax (..)", filename, line, col));
            }
            Expr::Struct {
                name,
                fields,
                span: span_of(s.span()),
            }
        }
        syn::Expr::Assign(a) => Expr::Assign {
            target: Box::new(convert_expr(&a.left, filename, diags)),
            value: Box::new(convert_expr(&a.right, filename, diags)),
            span: span_of(a.span()),
        },
        syn::Expr::Reference(r) => Expr::Reference {
            is_mut: r.mutability.is_some(),
            expr: Box::new(convert_expr(&r.expr, filename, diags)),
            span: span_of(r.span()),
        },
        syn::Expr::ForLoop(f) => {
            let pat = pat_name(&f.pat).0;
            Expr::For {
                pat,
                iter: Box::new(convert_expr(&f.expr, filename, diags)),
                body: convert_block(&f.body, filename, diags),
                span: span_of(f.span()),
            }
        }
        syn::Expr::While(w) => Expr::While {
            cond: Box::new(convert_expr(&w.cond, filename, diags)),
            body: convert_block(&w.body, filename, diags),
            span: span_of(w.span()),
        },
        syn::Expr::Loop(l) => Expr::Loop {
            body: convert_block(&l.body, filename, diags),
            span: span_of(l.span()),
        },
        syn::Expr::Return(r) => Expr::Return(
            r.expr
                .as_ref()
                .map(|e| Box::new(convert_expr(e, filename, diags))),
            span_of(r.span()),
        ),
        syn::Expr::Cast(c) => Expr::Cast {
            expr: Box::new(convert_expr(&c.expr, filename, diags)),
            ty: convert_type(&c.ty, filename, diags),
            span: span_of(c.span()),
        },
        syn::Expr::Try(t) => Expr::Try(Box::new(convert_expr(&t.expr, filename, diags)), span_of(t.span())),
        syn::Expr::Paren(p) => convert_expr(&p.expr, filename, diags),
        syn::Expr::Group(g) => convert_expr(&g.expr, filename, diags),
        syn::Expr::Unsafe(u) => {
            let (line, col) = line_col(u.span());
            diags.push(
                Diagnostic::unsupported("unsafe", filename, line, col)
                    .span("unsafe")
                    .note("Raskell cannot currently establish a safe semantic translation for this operation."),
            );
            Expr::Unsupported {
                description: "unsafe block".into(),
                span: span_of(u.span()),
            }
        }
        syn::Expr::Macro(m) => {
            let (line, col) = line_col(m.span());
            let name = path_expr_string(&m.mac.path);
            if name == "vec" {
                return convert_vec_macro(&m.mac, filename, diags);
            }
            if matches!(name.as_str(), "println" | "print" | "eprintln" | "eprint") {
                return convert_print_macro(&m.mac, &name, filename, diags);
            }
            if name == "format" {
                return convert_format_macro(&m.mac, filename, diags);
            }
            diags.push(
                Diagnostic::unsupported(&format!("macro `{name}!`"), filename, line, col)
                    .note("Supported macros: vec!, println!, print!, format!."),
            );
            Expr::Unsupported {
                description: format!("macro {name}!"),
                span: span_of(m.span()),
            }
        }
        syn::Expr::Range(r) => {
            // Support simple start..end ranges as a call to a range helper conceptually
            let start = r
                .start
                .as_ref()
                .map(|e| convert_expr(e, filename, diags))
                .unwrap_or(Expr::Lit(Lit::Int(0), span_of(r.span())));
            let end = r
                .end
                .as_ref()
                .map(|e| convert_expr(e, filename, diags));
            if let Some(end) = end {
                // Represent as MethodCall-like: Range { start, end } via Call to "range"
                Expr::Call {
                    func: Box::new(Expr::Path("range".into(), span_of(r.span()))),
                    args: vec![start, end],
                    span: span_of(r.span()),
                }
            } else {
                let (line, col) = line_col(r.span());
                diags.push(Diagnostic::unsupported("open-ended ranges", filename, line, col));
                Expr::Unsupported {
                    description: "range".into(),
                    span: span_of(r.span()),
                }
            }
        }
        syn::Expr::Repeat(r) => {
            // [expr; n] — limited support
            Expr::Call {
                func: Box::new(Expr::Path("replicate".into(), span_of(r.span()))),
                args: vec![
                    convert_expr(&r.len, filename, diags),
                    convert_expr(&r.expr, filename, diags),
                ],
                span: span_of(r.span()),
            }
        }
        other => {
            let (line, col) = line_col(other.span());
            diags.push(Diagnostic::unsupported(
                "this expression",
                filename,
                line,
                col,
            ));
            Expr::Unsupported {
                description: "unsupported expression".into(),
                span: span_of(other.span()),
            }
        }
    }
}

fn convert_print_macro(mac: &syn::Macro, name: &str, filename: &str, diags: &mut Diagnostics) -> Expr {
    // println!("...") or println!("{}", x)
    let parser = syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated;
    let args: Vec<Expr> = match syn::parse::Parser::parse2(parser, mac.tokens.clone()) {
        Ok(elems) => elems
            .iter()
            .map(|e| convert_expr(e, filename, diags))
            .collect(),
        Err(_) => vec![Expr::Lit(Lit::Str(String::new()), span_of(mac.span()))],
    };
    let func = match name {
        "print" | "eprint" => "putStr",
        _ => "putStrLn",
    };
    // If single string literal, use directly; if format-like, show via show
    let arg = if args.is_empty() {
        Expr::Lit(Lit::Str(String::new()), span_of(mac.span()))
    } else if args.len() == 1 {
        args[0].clone()
    } else {
        // format string + args → show of first value arg (skip format string)
        let value = args.get(1).cloned().unwrap_or_else(|| args[0].clone());
        match &value {
            Expr::Lit(Lit::Str(_), _) => value,
            other => Expr::Call {
                func: Box::new(Expr::Path("show".into(), span_of(mac.span()))),
                args: vec![other.clone()],
                span: span_of(mac.span()),
            },
        }
    };
    Expr::Call {
        func: Box::new(Expr::Path(func.into(), span_of(mac.span()))),
        args: vec![arg],
        span: span_of(mac.span()),
    }
}

fn convert_format_macro(mac: &syn::Macro, filename: &str, diags: &mut Diagnostics) -> Expr {
    let parser = syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated;
    let span = span_of(mac.span());
    match syn::parse::Parser::parse2(parser, mac.tokens.clone()) {
        Ok(elems) if elems.is_empty() => Expr::Lit(Lit::Str(String::new()), span),
        Ok(elems) if elems.len() == 1 => convert_expr(&elems[0], filename, diags),
        Ok(elems) => {
            let fmt = match elems.first() {
                Some(syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                })) => Some(s.value()),
                _ => None,
            };
            let values: Vec<Expr> = elems
                .iter()
                .skip(1)
                .map(|e| {
                    let v = convert_expr(e, filename, diags);
                    match &v {
                        Expr::Lit(Lit::Str(_), _) => v,
                        other => Expr::Call {
                            func: Box::new(Expr::Path("show".into(), span)),
                            args: vec![other.clone()],
                            span,
                        },
                    }
                })
                .collect();

            if let Some(fmt) = fmt {
                if let Some(expr) = expand_format_template(&fmt, &values, span) {
                    return expr;
                }
            }

            // Fallback: concatenate shown values
            match values.len() {
                0 => convert_expr(&elems[0], filename, diags),
                1 => values.into_iter().next().unwrap(),
                _ => values
                    .into_iter()
                    .reduce(|left, right| str_append(left, right, span))
                    .unwrap(),
            }
        }
        _ => Expr::Lit(Lit::Str(String::new()), span),
    }
}

/// Expand simple `format!("a{}b{}c", …)` templates into `++` concatenations.
fn expand_format_template(fmt: &str, values: &[Expr], span: Span) -> Option<Expr> {
    // Only handle plain `{}` placeholders (no named/positional specs).
    if fmt.contains("{:") || fmt.contains("{0") || fmt.contains("{1") {
        return None;
    }
    let parts: Vec<&str> = fmt.split("{}").collect();
    if parts.len() != values.len() + 1 {
        return None;
    }
    let mut pieces: Vec<Expr> = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        if !part.is_empty() {
            pieces.push(Expr::Lit(Lit::Str((*part).to_string()), span));
        }
        if i < values.len() {
            pieces.push(values[i].clone());
        }
    }
    match pieces.len() {
        0 => Some(Expr::Lit(Lit::Str(String::new()), span)),
        1 => pieces.into_iter().next(),
        _ => pieces.into_iter().reduce(|l, r| str_append(l, r, span)),
    }
}

fn str_append(left: Expr, right: Expr, span: Span) -> Expr {
    Expr::Call {
        func: Box::new(Expr::Path("strAppend".into(), span)),
        args: vec![left, right],
        span,
    }
}

fn convert_vec_macro(mac: &syn::Macro, filename: &str, diags: &mut Diagnostics) -> Expr {
    // vec![a, b, c] or vec![x; n]
    let tokens = mac.tokens.clone();
    // Try parsing as comma-separated exprs
    let parser = syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated;
    if let Ok(elems) = syn::parse::Parser::parse2(parser, tokens.clone()) {
        return Expr::Array(
            elems
                .iter()
                .map(|e| convert_expr(e, filename, diags))
                .collect(),
            span_of(mac.span()),
        );
    }
    // Try [expr; n]
    if let Ok(syn::Expr::Repeat(r)) = syn::parse2::<syn::Expr>(quote::quote! { [#tokens] }) {
        return Expr::Call {
            func: Box::new(Expr::Path("replicate".into(), span_of(mac.span()))),
            args: vec![
                convert_expr(&r.len, filename, diags),
                convert_expr(&r.expr, filename, diags),
            ],
            span: span_of(mac.span()),
        };
    }
    let (line, col) = line_col(mac.span());
    diags.push(Diagnostic::error("E0422", "could not parse vec! macro").at(filename, line, col));
    Expr::Array(vec![], span_of(mac.span()))
}

fn convert_lit(lit: &syn::Lit) -> Lit {
    match lit {
        syn::Lit::Int(i) => Lit::Int(i.base10_parse::<i64>().unwrap_or(0)),
        syn::Lit::Float(f) => Lit::Float(f.base10_parse::<f64>().unwrap_or(0.0)),
        syn::Lit::Bool(b) => Lit::Bool(b.value),
        syn::Lit::Str(s) => Lit::Str(s.value()),
        syn::Lit::Char(c) => Lit::Char(c.value()),
        _ => Lit::Unit,
    }
}

fn assign_op_of(op: &syn::BinOp) -> Option<BinOp> {
    match op {
        syn::BinOp::AddAssign(_) => Some(BinOp::Add),
        syn::BinOp::SubAssign(_) => Some(BinOp::Sub),
        syn::BinOp::MulAssign(_) => Some(BinOp::Mul),
        syn::BinOp::DivAssign(_) => Some(BinOp::Div),
        syn::BinOp::RemAssign(_) => Some(BinOp::Rem),
        syn::BinOp::BitAndAssign(_) => Some(BinOp::BitAnd),
        syn::BinOp::BitOrAssign(_) => Some(BinOp::BitOr),
        syn::BinOp::BitXorAssign(_) => Some(BinOp::BitXor),
        syn::BinOp::ShlAssign(_) => Some(BinOp::Shl),
        syn::BinOp::ShrAssign(_) => Some(BinOp::Shr),
        _ => None,
    }
}

fn convert_binop(op: &syn::BinOp, filename: &str, diags: &mut Diagnostics) -> BinOp {
    match op {
        syn::BinOp::Add(_) => BinOp::Add,
        syn::BinOp::Sub(_) => BinOp::Sub,
        syn::BinOp::Mul(_) => BinOp::Mul,
        syn::BinOp::Div(_) => BinOp::Div,
        syn::BinOp::Rem(_) => BinOp::Rem,
        syn::BinOp::Eq(_) => BinOp::Eq,
        syn::BinOp::Ne(_) => BinOp::Ne,
        syn::BinOp::Lt(_) => BinOp::Lt,
        syn::BinOp::Le(_) => BinOp::Le,
        syn::BinOp::Gt(_) => BinOp::Gt,
        syn::BinOp::Ge(_) => BinOp::Ge,
        syn::BinOp::And(_) => BinOp::And,
        syn::BinOp::Or(_) => BinOp::Or,
        syn::BinOp::BitAnd(_) => BinOp::BitAnd,
        syn::BinOp::BitOr(_) => BinOp::BitOr,
        syn::BinOp::BitXor(_) => BinOp::BitXor,
        syn::BinOp::Shl(_) => BinOp::Shl,
        syn::BinOp::Shr(_) => BinOp::Shr,
        syn::BinOp::AddAssign(_)
        | syn::BinOp::SubAssign(_)
        | syn::BinOp::MulAssign(_)
        | syn::BinOp::DivAssign(_)
        | syn::BinOp::RemAssign(_)
        | syn::BinOp::BitAndAssign(_)
        | syn::BinOp::BitOrAssign(_)
        | syn::BinOp::BitXorAssign(_)
        | syn::BinOp::ShlAssign(_)
        | syn::BinOp::ShrAssign(_) => {
            // Handled via assign_op_of before convert_binop is called.
            BinOp::Add
        }
        other => {
            let (line, col) = line_col(other.span());
            diags.push(Diagnostic::unsupported("this binary operator", filename, line, col));
            BinOp::Add
        }
    }
}

// Handle AssignOp specially - syn has Expr::AssignOp
// We need to add it to convert_expr - let me check if I missed it.

fn path_expr_string(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

fn convert_pat(pat: &Pat, filename: &str, diags: &mut Diagnostics) -> Pattern {
    match pat {
        Pat::Wild(_) => Pattern::Wildcard,
        Pat::Lit(l) => Pattern::Lit(convert_lit(&l.lit)),
        Pat::Ident(i) => {
            if i.subpat.is_some() {
                let (line, col) = line_col(i.span());
                diags.push(Diagnostic::unsupported("@ patterns", filename, line, col));
            }
            Pattern::Ident(i.ident.to_string())
        }
        Pat::Tuple(t) => Pattern::Tuple(
            t.elems
                .iter()
                .map(|p| convert_pat(p, filename, diags))
                .collect(),
        ),
        Pat::TupleStruct(ts) => {
            let name = path_expr_string(&ts.path);
            let elems = ts
                .elems
                .iter()
                .map(|p| convert_pat(p, filename, diags))
                .collect();
            // Parse Enum::Variant
            if let Some((enum_name, variant)) = name.rsplit_once("::") {
                Pattern::Variant {
                    enum_name: Some(enum_name.to_string()),
                    variant: variant.to_string(),
                    elems,
                }
            } else {
                Pattern::TupleStruct { name, elems }
            }
        }
        Pat::Struct(s) => {
            let name = path_expr_string(&s.path);
            let fields = s
                .fields
                .iter()
                .filter_map(|f| match &f.member {
                    syn::Member::Named(id) => {
                        Some((id.to_string(), convert_pat(&f.pat, filename, diags)))
                    }
                    _ => None,
                })
                .collect();
            Pattern::Struct { name, fields }
        }
        Pat::Path(p) => {
            let name = path_expr_string(&p.path);
            if let Some((enum_name, variant)) = name.rsplit_once("::") {
                Pattern::Variant {
                    enum_name: Some(enum_name.to_string()),
                    variant: variant.to_string(),
                    elems: vec![],
                }
            } else {
                Pattern::Path(name)
            }
        }
        Pat::Reference(r) => Pattern::Ref {
            is_mut: r.mutability.is_some(),
            inner: Box::new(convert_pat(&r.pat, filename, diags)),
        },
        Pat::Type(t) => convert_pat(&t.pat, filename, diags),
        Pat::Paren(p) => convert_pat(&p.pat, filename, diags),
        other => {
            let (line, col) = line_col(other.span());
            diags.push(Diagnostic::unsupported("this pattern", filename, line, col));
            Pattern::Wildcard
        }
    }
}
