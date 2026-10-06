//! Pretty-printer for the Haskell AST.

use crate::haskell::ast::*;

pub fn pretty_print(module: &HsModule) -> String {
    let mut out = String::new();
    for p in &module.pragmas {
        out.push_str(&format!("{{ bak-# {p} #-}}\n"));
    }
    // Fix pragma format
    out.clear();
    for p in &module.pragmas {
        out.push_str(&format!("{{-# {p} #-}}\n"));
    }

    out.push_str(&format!("module {} where\n\n", module.name));

    for imp in &module.imports {
        out.push_str(&format_import(imp));
        out.push('\n');
    }
    if !module.imports.is_empty() {
        out.push('\n');
    }

    for decl in &module.decls {
        out.push_str(&format_decl(decl));
        out.push_str("\n\n");
    }
    out
}

fn format_import(imp: &HsImport) -> String {
    let mut s = String::from("import ");
    if imp.qualified {
        s.push_str("qualified ");
    }
    s.push_str(&imp.module);
    if let Some(alias) = &imp.alias {
        s.push_str(" as ");
        s.push_str(alias);
    }
    if let Some(items) = &imp.items {
        if items.first().map(|s| s.as_str()) == Some("hiding") {
            s.push_str(" hiding (");
            s.push_str(&items[1..].join(", "));
            s.push(')');
        } else {
            s.push_str(" (");
            s.push_str(&items.join(", "));
            s.push(')');
        }
    }
    s
}

fn format_decl(decl: &HsDecl) -> String {
    match decl {
        HsDecl::Data {
            name,
            generics,
            ctors,
            deriving,
        } => {
            let gens = if generics.is_empty() {
                String::new()
            } else {
                format!(" {}", generics.join(" "))
            };
            let mut s = format!("data {name}{gens}");
            if ctors.is_empty() {
                s.push_str(&format!(" = {name}"));
            } else {
                s.push_str("\n    = ");
                let parts: Vec<String> = ctors.iter().map(format_ctor).collect();
                s.push_str(&parts.join("\n    | "));
            }
            if !deriving.is_empty() {
                s.push_str(&format!("\n    deriving ({})", deriving.join(", ")));
            }
            s
        }
        HsDecl::TypeSig {
            name,
            constraints,
            ty,
        } => {
            let ty_s = if constraints.is_empty() {
                format_type(ty)
            } else if let HsType::Constrained(_, inner) = ty {
                let ctx = format_constraints(constraints);
                format!("{ctx} => {}", format_type(inner))
            } else {
                let ctx = format_constraints(constraints);
                format!("{ctx} => {}", format_type(ty))
            };
            format!("{name} :: {ty_s}")
        }
        HsDecl::FunBind { name, equations } => {
            let eqs: Vec<String> = equations
                .iter()
                .map(|eq| {
                    let pats = eq
                        .pats
                        .iter()
                        .map(|p| format_pat(p, true))
                        .collect::<Vec<_>>()
                        .join(" ");
                    let lhs = if pats.is_empty() {
                        name.clone()
                    } else {
                        format!("{name} {pats}")
                    };
                    format!("{lhs} = {}", format_exp(&eq.body, 0))
                })
                .collect();
            eqs.join("\n")
        }
        HsDecl::PatBind { name, body } => format!("{name} = {}", format_exp(body, 0)),
        HsDecl::Class {
            name,
            type_var,
            methods,
        } => {
            let mut s = format!("class {name} {type_var} where");
            for (m, ty) in methods {
                s.push_str(&format!("\n  {m} :: {}", format_type(ty)));
            }
            s
        }
        HsDecl::Instance { class, ty, methods } => {
            let mut s = format!("instance {class} {} where", format_type(ty));
            for (m, pats, body) in methods {
                let ps = pats
                    .iter()
                    .map(|p| format_pat(p, true))
                    .collect::<Vec<_>>()
                    .join(" ");
                if ps.is_empty() {
                    s.push_str(&format!("\n  {m} = {}", format_exp(body, 0)));
                } else {
                    s.push_str(&format!("\n  {m} {ps} = {}", format_exp(body, 0)));
                }
            }
            s
        }
    }
}

fn format_constraints(constraints: &[(String, String)]) -> String {
    let parts: Vec<String> = constraints
        .iter()
        .map(|(c, v)| format!("{c} {v}"))
        .collect();
    if parts.len() == 1 {
        parts[0].clone()
    } else {
        format!("({})", parts.join(", "))
    }
}

fn format_ctor(c: &HsCtor) -> String {
    if let Some(record) = &c.record {
        let fields: Vec<String> = record
            .iter()
            .map(|(n, t)| format!("{n} :: {}", format_type(t)))
            .collect();
        format!("{} {{ {} }}", c.name, fields.join(", "))
    } else if c.fields.is_empty() {
        c.name.clone()
    } else {
        let fields: Vec<String> = c.fields.iter().map(|t| format_type_atom(t)).collect();
        format!("{} {}", c.name, fields.join(" "))
    }
}

fn format_type(ty: &HsType) -> String {
    match ty {
        HsType::Fun(a, b) => format!("{} -> {}", format_type_atom(a), format_type(b)),
        HsType::Constrained(cs, inner) => {
            format!("{} => {}", format_constraints(cs), format_type(inner))
        }
        other => format_type_atom(other),
    }
}

fn format_type_atom(ty: &HsType) -> String {
    match ty {
        HsType::Var(v) => v.clone(),
        HsType::Con(c) => c.clone(),
        HsType::App(f, a) => format!("{} {}", format_type_atom(f), format_type_atom(a)),
        HsType::Fun(_, _) | HsType::Constrained(_, _) => format!("({})", format_type(ty)),
        HsType::Tuple(ts) => {
            let inner: Vec<_> = ts.iter().map(format_type).collect();
            format!("({})", inner.join(", "))
        }
        HsType::List(t) => format!("[{}]", format_type(t)),
        HsType::Paren(t) => format!("({})", format_type(t)),
    }
}

fn format_exp(exp: &HsExp, prec: u8) -> String {
    match exp {
        HsExp::Var(v) => v.clone(),
        HsExp::Con(c) => c.clone(),
        HsExp::Lit(l) => format_lit(l),
        HsExp::App(f, a) => {
            let s = format!("{} {}", format_exp(f, 10), format_exp(a, 11));
            if prec > 10 {
                format!("({s})")
            } else {
                s
            }
        }
        HsExp::Infix(op, l, r) => {
            let s = format!("{} {} {}", format_exp(l, 6), op, format_exp(r, 6));
            if prec > 5 {
                format!("({s})")
            } else {
                s
            }
        }
        HsExp::Lam(ps, body) => {
            let s = format!("\\{} -> {}", ps.join(" "), format_exp(body, 0));
            if prec > 0 {
                format!("({s})")
            } else {
                s
            }
        }
        HsExp::Let(binds, body) => {
            let bs: Vec<String> = binds
                .iter()
                .map(|(n, e)| format!("{n} = {}", format_exp(e, 0)))
                .collect();
            format!("let {} in {}", bs.join("; "), format_exp(body, 0))
        }
        HsExp::If(c, t, e) => {
            let s = format!(
                "if {} then {} else {}",
                format_exp(c, 0),
                format_exp(t, 0),
                format_exp(e, 0)
            );
            if prec > 0 {
                format!("({s})")
            } else {
                s
            }
        }
        HsExp::Case(s, arms) => {
            let mut out = format!("case {} of", format_exp(s, 0));
            for (p, e) in arms {
                out.push_str(&format!("\n    {} -> {}", format_pat(p, false), format_exp(e, 0)));
            }
            if prec > 0 {
                format!("({out})")
            } else {
                out
            }
        }
        HsExp::Tuple(es) => {
            let inner: Vec<_> = es.iter().map(|e| format_exp(e, 0)).collect();
            format!("({})", inner.join(", "))
        }
        HsExp::List(es) => {
            let inner: Vec<_> = es.iter().map(|e| format_exp(e, 0)).collect();
            format!("[{}]", inner.join(", "))
        }
        HsExp::ListComp { expr, quals } => {
            let qs: Vec<String> = quals
                .iter()
                .map(|q| match q {
                    HsQual::Gen(p, e) => format!("{} <- {}", format_pat(p, false), format_exp(e, 0)),
                    HsQual::Filter(e) => format_exp(e, 0),
                })
                .collect();
            format!("[{} | {}]", format_exp(expr, 0), qs.join(", "))
        }
        HsExp::Record { name, fields } => {
            let fs: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{k} = {}", format_exp(v, 0)))
                .collect();
            format!("{name} {{ {} }}", fs.join(", "))
        }
        HsExp::RecordUpdate { base, fields } => {
            let fs: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{k} = {}", format_exp(v, 0)))
                .collect();
            format!("{} {{ {} }}", format_exp(base, 10), fs.join(", "))
        }
        HsExp::Field(e, f) => format!("{} {}", f, format_exp(e, 11)),
        HsExp::Paren(e) => format!("({})", format_exp(e, 0)),
        HsExp::Neg(e) => format!("-{}", format_exp(e, 11)),
        HsExp::Do(stmts) => {
            let mut out = String::from("do");
            for s in stmts {
                match s {
                    HsDoStmt::Bind(p, e) => {
                        out.push_str(&format!(
                            "\n  {} <- {}",
                            format_pat(p, false),
                            format_exp(e, 0)
                        ));
                    }
                    HsDoStmt::Let(n, e) => {
                        out.push_str(&format!("\n  let {} = {}", n, format_exp(e, 0)));
                    }
                    HsDoStmt::Exp(e) => {
                        out.push_str(&format!("\n  {}", format_exp(e, 0)));
                    }
                }
            }
            out
        }
    }
}

fn format_pat(pat: &HsPat, atom: bool) -> String {
    let s = match pat {
        HsPat::Wildcard => "_".into(),
        HsPat::Var(v) => v.clone(),
        HsPat::Lit(l) => format_lit(l),
        HsPat::Con(n, args) => {
            if n == "(:)" && args.len() == 2 {
                format!(
                    "({}:{})",
                    format_pat(&args[0], true),
                    format_pat(&args[1], true)
                )
            } else if args.is_empty() {
                n.clone()
            } else {
                let a: Vec<_> = args.iter().map(|p| format_pat(p, true)).collect();
                format!("{n} {}", a.join(" "))
            }
        }
        HsPat::Tuple(ps) => {
            let inner: Vec<_> = ps.iter().map(|p| format_pat(p, false)).collect();
            format!("({})", inner.join(", "))
        }
        HsPat::List(ps) => {
            let inner: Vec<_> = ps.iter().map(|p| format_pat(p, false)).collect();
            format!("[{}]", inner.join(", "))
        }
        HsPat::Record(n, fields) => {
            let fs: Vec<_> = fields
                .iter()
                .map(|(k, v)| format!("{k} = {}", format_pat(v, false)))
                .collect();
            format!("{n} {{ {} }}", fs.join(", "))
        }
        HsPat::As(n, p) => format!("{n}@{}", format_pat(p, true)),
        HsPat::Paren(p) => format!("({})", format_pat(p, false)),
    };
    if atom && matches!(pat, HsPat::Con(_, args) if !args.is_empty()) {
        format!("({s})")
    } else {
        s
    }
}

fn format_lit(l: &HsLit) -> String {
    match l {
        HsLit::Int(n) => n.to_string(),
        HsLit::Float(f) => format!("{f}"),
        HsLit::Bool(true) => "True".into(),
        HsLit::Bool(false) => "False".into(),
        HsLit::Str(s) => format!("\"{}\"", escape_str(s)),
        HsLit::Char(c) => format!("'{c}'"),
    }
}

fn escape_str(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\\' => "\\\\".into(),
            '"' => "\\\"".into(),
            '\n' => "\\n".into(),
            other => other.to_string(),
        })
        .collect()
}
