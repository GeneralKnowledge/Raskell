//! Lower translation IR → Haskell AST.

use crate::diagnostics::Diagnostics;
use crate::haskell::ast::*;
use crate::ir::{self, Module, Ty};
use indexmap::IndexMap;

pub fn lower_module(module: &Module) -> anyhow::Result<(HsModule, Diagnostics)> {
    let diags = Diagnostics::new();
    let mut hs = HsModule {
        name: module.name.clone(),
        pragmas: vec![],
        exports: None,
        imports: default_imports(module),
        decls: vec![],
    };

    for decl in &module.decls {
        match decl {
            ir::Decl::Data(d) => {
                hs.decls.push(lower_data(d));
            }
            ir::Decl::Func(f) => {
                hs.decls.push(HsDecl::TypeSig {
                    name: f.name.clone(),
                    ty: lower_fun_type(&f.params, &f.return_ty),
                });
                hs.decls.push(HsDecl::FunBind {
                    name: f.name.clone(),
                    equations: vec![HsEquation {
                        pats: f
                            .params
                            .iter()
                            .map(|(n, _)| HsPat::Var(n.clone()))
                            .collect(),
                        body: lower_exp(&f.body),
                        guards: vec![],
                    }],
                });
            }
            ir::Decl::Const { name, ty, value } => {
                hs.decls.push(HsDecl::TypeSig {
                    name: name.clone(),
                    ty: lower_ty(ty),
                });
                hs.decls.push(HsDecl::PatBind {
                    name: name.clone(),
                    body: lower_exp(value),
                });
            }
        }
    }

    Ok((hs, diags))
}

fn default_imports(module: &Module) -> Vec<HsImport> {
    let mut imports = vec![];
    let needs_maybe = module_needs(module, &["Just", "Nothing", "Maybe", "fromMaybe", "fromJust", "mapMaybe"]);
    let needs_either = module_needs(module, &["Left", "Right", "Either", "either"]);
    let needs_word = module_has_ty(module, |t| matches!(t, Ty::Word32 | Ty::Word64));

    // Hide Prelude names that collide with user definitions
    let prelude_conflicts: &[&str] = &[
        "gcd", "lcm", "sum", "product", "id", "map", "filter", "foldr", "foldl", "head", "tail",
        "exp", "log", "sin", "cos", "tan",
    ];
    let hidden: Vec<String> = module
        .decls
        .iter()
        .filter_map(|d| match d {
            ir::Decl::Func(f) if prelude_conflicts.contains(&f.name.as_str()) => {
                Some(f.name.clone())
            }
            _ => None,
        })
        .collect();
    if !hidden.is_empty() {
        imports.push(HsImport {
            module: "Prelude".into(),
            qualified: false,
            alias: None,
            items: Some(
                // Represent as hiding — handled specially in pretty printer
                std::iter::once("hiding".to_string())
                    .chain(hidden)
                    .collect(),
            ),
        });
    }

    if needs_word {
        imports.push(HsImport {
            module: "Data.Word".into(),
            qualified: false,
            alias: None,
            items: Some(vec!["Word32".into(), "Word64".into()]),
        });
    }
    if needs_maybe {
        imports.push(HsImport {
            module: "Data.Maybe".into(),
            qualified: false,
            alias: None,
            items: None,
        });
    }
    if needs_either {
        imports.push(HsImport {
            module: "Data.Either".into(),
            qualified: false,
            alias: None,
            items: None,
        });
    }
    imports
}

fn module_needs(module: &Module, names: &[&str]) -> bool {
    let s = format!("{module:?}");
    names.iter().any(|n| s.contains(n))
}

fn module_has_ty(module: &Module, pred: impl Fn(&Ty) -> bool) -> bool {
    for d in &module.decls {
        match d {
            ir::Decl::Func(f) => {
                if f.params.iter().any(|(_, t)| pred(t)) || pred(&f.return_ty) {
                    return true;
                }
            }
            ir::Decl::Data(dt) => {
                for c in &dt.constructors {
                    for f in &c.fields {
                        if pred(&f.ty) {
                            return true;
                        }
                    }
                }
            }
            ir::Decl::Const { ty, .. } => {
                if pred(ty) {
                    return true;
                }
            }
        }
    }
    false
}

fn lower_data(d: &ir::DataType) -> HsDecl {
    let ctors = d
        .constructors
        .iter()
        .map(|c| {
            if d.is_record && c.fields.iter().all(|f| f.name.is_some()) && !c.fields.is_empty() {
                let mut record = IndexMap::new();
                for f in &c.fields {
                    record.insert(f.name.clone().unwrap(), lower_ty(&f.ty));
                }
                HsCtor {
                    name: c.name.clone(),
                    record: Some(record),
                    fields: vec![],
                }
            } else {
                HsCtor {
                    name: c.name.clone(),
                    record: None,
                    fields: c.fields.iter().map(|f| lower_ty(&f.ty)).collect(),
                }
            }
        })
        .collect();

    HsDecl::Data {
        name: d.name.clone(),
        generics: d.generics.iter().map(|g| g.to_lowercase()).collect(),
        ctors,
        deriving: vec!["Show".into(), "Eq".into()],
    }
}

fn lower_fun_type(params: &[(String, Ty)], ret: &Ty) -> HsType {
    let mut ty = lower_ty(ret);
    for (_, p) in params.iter().rev() {
        ty = HsType::Fun(Box::new(lower_ty(p)), Box::new(ty));
    }
    ty
}

fn lower_ty(ty: &Ty) -> HsType {
    match ty {
        Ty::Unit => HsType::Con("()".into()),
        Ty::Bool => HsType::Con("Bool".into()),
        Ty::Int | Ty::Integer => HsType::Con("Int".into()),
        Ty::Word32 => HsType::Con("Word32".into()),
        Ty::Word64 => HsType::Con("Word64".into()),
        Ty::Double => HsType::Con("Double".into()),
        Ty::Float => HsType::Con("Float".into()),
        Ty::Char => HsType::Con("Char".into()),
        Ty::String => HsType::Con("String".into()),
        Ty::List(t) => HsType::List(Box::new(lower_ty(t))),
        Ty::Maybe(t) => HsType::App(Box::new(HsType::Con("Maybe".into())), Box::new(lower_ty(t))),
        Ty::Either(e, a) => HsType::App(
            Box::new(HsType::App(
                Box::new(HsType::Con("Either".into())),
                Box::new(lower_ty(e)),
            )),
            Box::new(lower_ty(a)),
        ),
        Ty::Tuple(ts) => HsType::Tuple(ts.iter().map(lower_ty).collect()),
        Ty::Named(n, args) => {
            let mut t = HsType::Con(n.clone());
            for a in args {
                t = HsType::App(Box::new(t), Box::new(lower_ty(a)));
            }
            t
        }
        Ty::Fun(ps, r) => {
            let mut t = lower_ty(r);
            for p in ps.iter().rev() {
                t = HsType::Fun(Box::new(lower_ty(p)), Box::new(t));
            }
            t
        }
        Ty::Var(v) => HsType::Var(v.to_lowercase()),
    }
}

fn lower_exp(exp: &ir::Exp) -> HsExp {
    match exp {
        ir::Exp::Lit(ir::Lit::Unit) => HsExp::Con("()".into()),
        ir::Exp::Lit(l) => HsExp::Lit(lower_lit(l)),
        ir::Exp::Var(v) => {
            // Constructors start with uppercase
            if v.chars().next().is_some_and(|c| c.is_uppercase()) {
                HsExp::Con(v.clone())
            } else if v == "(!!)" {
                HsExp::Var("(!!)".into())
            } else {
                HsExp::Var(v.clone())
            }
        }
        ir::Exp::App(f, args) => {
            let mut e = lower_exp(f);
            for a in args {
                e = HsExp::App(Box::new(e), Box::new(paren_atom(lower_exp(a))));
            }
            e
        }
        ir::Exp::Lam(ps, body) => HsExp::Lam(ps.clone(), Box::new(lower_exp(body))),
        ir::Exp::Let(binds, body) => HsExp::Let(
            binds
                .iter()
                .map(|b| (b.name.clone(), lower_exp(&b.value)))
                .collect(),
            Box::new(lower_exp(body)),
        ),
        ir::Exp::If(c, t, e) => HsExp::If(
            Box::new(lower_exp(c)),
            Box::new(lower_exp(t)),
            Box::new(lower_exp(e)),
        ),
        ir::Exp::Case(s, arms) => HsExp::Case(
            Box::new(lower_exp(s)),
            arms.iter()
                .map(|a| (lower_pat(&a.pattern), lower_exp(&a.body)))
                .collect(),
        ),
        ir::Exp::Tuple(es) => HsExp::Tuple(es.iter().map(lower_exp).collect()),
        ir::Exp::List(es) => HsExp::List(es.iter().map(lower_exp).collect()),
        ir::Exp::Record { name, fields } => HsExp::Record {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(k, v)| (k.clone(), lower_exp(v)))
                .collect(),
        },
        ir::Exp::Field(e, f) => {
            // Haskell record selector: field record
            HsExp::App(
                Box::new(HsExp::Var(f.clone())),
                Box::new(paren_atom(lower_exp(e))),
            )
        }
        ir::Exp::BinOp(op, l, r) => {
            let sym = binop_sym(*op);
            HsExp::Infix(
                sym.into(),
                Box::new(paren_if_needed(lower_exp(l))),
                Box::new(paren_if_needed(lower_exp(r))),
            )
        }
        ir::Exp::UnOp(op, e) => match op {
            ir::UnOp::Neg => HsExp::Neg(Box::new(paren_atom(lower_exp(e)))),
            ir::UnOp::Not => HsExp::App(
                Box::new(HsExp::Var("not".into())),
                Box::new(paren_atom(lower_exp(e))),
            ),
        },
        ir::Exp::Map(f, xs) => HsExp::App(
            Box::new(HsExp::App(
                Box::new(HsExp::Var("map".into())),
                Box::new(paren_atom(lower_exp(f))),
            )),
            Box::new(paren_atom(lower_exp(xs))),
        ),
        ir::Exp::Filter(p, xs) => HsExp::App(
            Box::new(HsExp::App(
                Box::new(HsExp::Var("filter".into())),
                Box::new(paren_atom(lower_exp(p))),
            )),
            Box::new(paren_atom(lower_exp(xs))),
        ),
        ir::Exp::Fold(f, z, xs) => HsExp::App(
            Box::new(HsExp::App(
                Box::new(HsExp::App(
                    Box::new(HsExp::Var("foldl".into())),
                    Box::new(paren_atom(lower_exp(f))),
                )),
                Box::new(paren_atom(lower_exp(z))),
            )),
            Box::new(paren_atom(lower_exp(xs))),
        ),
        ir::Exp::Sum(xs) => HsExp::App(
            Box::new(HsExp::Var("sum".into())),
            Box::new(paren_atom(lower_exp(xs))),
        ),
        ir::Exp::Seq(es) => {
            // Sequence as nested lets discarding — last value wins
            if let Some((last, init)) = es.split_last() {
                let mut e = lower_exp(last);
                for (i, x) in init.iter().enumerate().rev() {
                    e = HsExp::Let(
                        vec![(format!("_s{i}"), lower_exp(x))],
                        Box::new(e),
                    );
                }
                e
            } else {
                HsExp::Con("()".into())
            }
        }
        ir::Exp::Do(stmts, last) => {
            let mut ds: Vec<HsDoStmt> = stmts
                .iter()
                .map(|s| match s {
                    ir::DoStmt::Bind { name, exp } => {
                        HsDoStmt::Bind(HsPat::Var(name.clone()), lower_exp(exp))
                    }
                    ir::DoStmt::Exp(e) => HsDoStmt::Exp(lower_exp(e)),
                    ir::DoStmt::Let { name, exp } => HsDoStmt::Let(name.clone(), lower_exp(exp)),
                })
                .collect();
            ds.push(HsDoStmt::Exp(lower_exp(last)));
            HsExp::Do(ds)
        }
        ir::Exp::Pure(e) => HsExp::App(
            Box::new(HsExp::Var("return".into())),
            Box::new(paren_atom(lower_exp(e))),
        ),
        ir::Exp::Error(msg) => HsExp::App(
            Box::new(HsExp::Var("error".into())),
            Box::new(HsExp::Lit(HsLit::Str(msg.clone()))),
        ),
    }
}

fn lower_pat(pat: &ir::Pat) -> HsPat {
    match pat {
        ir::Pat::Wildcard => HsPat::Wildcard,
        ir::Pat::Lit(l) => HsPat::Lit(lower_lit(l)),
        ir::Pat::Var(v) => HsPat::Var(v.clone()),
        ir::Pat::Tuple(ps) => HsPat::Tuple(ps.iter().map(lower_pat).collect()),
        ir::Pat::Constr { name, args } => {
            HsPat::Con(name.clone(), args.iter().map(lower_pat).collect())
        }
        ir::Pat::Record { name, fields } => HsPat::Record(
            name.clone(),
            fields
                .iter()
                .map(|(k, v)| (k.clone(), lower_pat(v)))
                .collect(),
        ),
    }
}

fn lower_lit(l: &ir::Lit) -> HsLit {
    match l {
        ir::Lit::Int(n) => HsLit::Int(*n),
        ir::Lit::Float(f) => HsLit::Float(*f),
        ir::Lit::Bool(b) => HsLit::Bool(*b),
        ir::Lit::Str(s) => HsLit::Str(s.clone()),
        ir::Lit::Char(c) => HsLit::Char(*c),
        ir::Lit::Unit => HsLit::Str("__UNIT__".into()), // rewritten below — use Con actually
    }
}

fn binop_sym(op: ir::BinOp) -> &'static str {
    match op {
        ir::BinOp::Add => "+",
        ir::BinOp::Sub => "-",
        ir::BinOp::Mul => "*",
        ir::BinOp::Div => "`div`",
        ir::BinOp::Rem => "`mod`",
        ir::BinOp::Eq => "==",
        ir::BinOp::Ne => "/=",
        ir::BinOp::Lt => "<",
        ir::BinOp::Le => "<=",
        ir::BinOp::Gt => ">",
        ir::BinOp::Ge => ">=",
        ir::BinOp::And => "&&",
        ir::BinOp::Or => "||",
        ir::BinOp::BitAnd => "&&", // simplified
        ir::BinOp::BitOr => "||",
        ir::BinOp::BitXor => "/=",
        ir::BinOp::Append => "++",
    }
}

fn paren_atom(e: HsExp) -> HsExp {
    match &e {
        HsExp::Var(_) | HsExp::Con(_) | HsExp::Lit(_) | HsExp::List(_) | HsExp::Tuple(_) | HsExp::Paren(_) => e,
        _ => HsExp::Paren(Box::new(e)),
    }
}

fn paren_if_needed(e: HsExp) -> HsExp {
    match &e {
        HsExp::Infix(_, _, _) | HsExp::Lam(_, _) | HsExp::If(_, _, _) | HsExp::Let(_, _) => {
            HsExp::Paren(Box::new(e))
        }
        _ => e,
    }
}
