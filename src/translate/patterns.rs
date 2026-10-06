//! Pattern recognition for idiomatic translations.

use crate::ast::*;

/// Recognised high-level patterns.
#[derive(Debug, Clone)]
pub enum PatternMatch {
    /// `let mut acc = 0; for x in xs { if cond { acc += x } }; acc` → sum/filter
    ConditionalSum {
        collection: String,
        pred_op: BinOp,
        pred_rhs: i64,
        element: String,
    },
    /// `let mut acc = 0; for x in xs { acc += x }; acc` → sum
    Sum { collection: String },
    /// `let mut r = Vec::new(); for x in xs { r.push(f(x)) }; r` → map
    MapPush,
    /// Iterator chain `.iter().filter().map().collect()`
    IteratorChain,
    /// Sequential mutation of a scalar → nested arithmetic
    ScalarMutation,
}

/// Detect sum-of-positives style:
/// ```ignore
/// let mut total = 0;
/// for value in values {
///     if value > 0 { total += value; }
/// }
/// total
/// ```
pub fn detect_conditional_sum(block: &Block) -> Option<(String, String, BinOp, Expr)> {
    // Need: let mut NAME = 0; for PAT in ITER { if COND { NAME += PAT } }; NAME
    let skip = skip_setup_lets(block);
    if block.stmts.len().saturating_sub(skip) < 2 {
        return None;
    }

    let acc_name = match &block.stmts[skip] {
        Stmt::Let {
            name,
            is_mut: true,
            value: Some(Expr::Lit(Lit::Int(0), _)),
            ..
        } => name.clone(),
        _ => return None,
    };

    let (pat, iter, body) = match &block.stmts[skip + 1] {
        Stmt::Expr(Expr::For {
            pat, iter, body, ..
        }) => (pat.clone(), iter.clone(), body),
        _ => return None,
    };

    if !matches!(&block.expr, Some(e) if matches!(e.as_ref(), Expr::Path(n, _) if n == &acc_name)) {
        return None;
    }

    // `if` may be a statement or the trailing expression of the for-body.
    let (cond, then_branch) = match for_body_if(body) {
        Some(v) => v,
        None => return None,
    };

    if !accumulates_add(then_branch, &acc_name, &pat) {
        return None;
    }

    let collection = match iter.as_ref() {
        Expr::Path(n, _) => n.clone(),
        _ => return None,
    };

    Some((acc_name, collection, BinOp::Gt, cond.clone()))
}

fn for_body_if(body: &Block) -> Option<(&Expr, &Block)> {
    if let Some(Stmt::Expr(Expr::If {
        cond,
        then_branch,
        else_branch: None,
        ..
    })) = body.stmts.first()
    {
        return Some((cond.as_ref(), then_branch));
    }
    if let Some(Expr::If {
        cond,
        then_branch,
        else_branch: None,
        ..
    }) = body.expr.as_deref()
    {
        return Some((cond, then_branch));
    }
    None
}

fn accumulates_add(then_branch: &Block, acc_name: &str, pat: &str) -> bool {
    then_branch.stmts.iter().any(|s| match s {
        Stmt::Expr(Expr::AssignOp {
            op: BinOp::Add,
            target,
            value,
            ..
        }) => {
            matches!(target.as_ref(), Expr::Path(n, _) if n == acc_name)
                && matches!(value.as_ref(), Expr::Path(n, _) if n == pat)
        }
        Stmt::Expr(Expr::Assign { target, value, .. }) => {
            matches!(target.as_ref(), Expr::Path(n, _) if n == acc_name)
                && matches!(
                    value.as_ref(),
                    Expr::Binary {
                        op: BinOp::Add,
                        ..
                    }
                )
        }
        _ => false,
    })
}

/// Detect: let mut total = 0; for x in xs { total += x }; total
pub fn detect_plain_sum(block: &Block) -> Option<String> {
    let skip = skip_setup_lets(block);
    if block.stmts.len().saturating_sub(skip) < 2 {
        return None;
    }
    let acc_name = match &block.stmts[skip] {
        Stmt::Let {
            name,
            is_mut: true,
            value: Some(Expr::Lit(Lit::Int(0), _)),
            ..
        } => name.clone(),
        _ => return None,
    };
    let (pat, iter, body) = match &block.stmts[skip + 1] {
        Stmt::Expr(Expr::For {
            pat, iter, body, ..
        }) => (pat.clone(), iter, body),
        _ => return None,
    };
    if !matches!(&block.expr, Some(e) if matches!(e.as_ref(), Expr::Path(n, _) if n == &acc_name)) {
        return None;
    }
    let assign = body
        .stmts
        .first()
        .and_then(|s| match s {
            Stmt::Expr(e) => Some(e),
            _ => None,
        })
        .or(body.expr.as_deref());
    let ok = matches!(
        assign,
        Some(Expr::AssignOp {
            op: BinOp::Add,
            target,
            value,
            ..
        }) if matches!(target.as_ref(), Expr::Path(n, _) if n == &acc_name)
            && matches!(value.as_ref(), Expr::Path(n, _) if n == &pat)
    );
    if !ok {
        return None;
    }
    match iter.as_ref() {
        Expr::Path(n, _) => Some(n.clone()),
        _ => None,
    }
}

/// Detect map via push:
/// ```ignore
/// let mut result = Vec::new();
/// for value in values {
///     result.push(value * 2);
/// }
/// result
/// ```
pub fn detect_map_push(block: &Block) -> Option<(String, String, Expr)> {
    let skip = skip_setup_lets(block);
    if block.stmts.len().saturating_sub(skip) < 2 {
        return None;
    }
    let result_name = match &block.stmts[skip] {
        Stmt::Let {
            name,
            is_mut: true,
            value: Some(v),
            ..
        } if is_vec_new(v) => name.clone(),
        _ => return None,
    };
    let (pat, iter, body) = match &block.stmts[skip + 1] {
        Stmt::Expr(Expr::For {
            pat, iter, body, ..
        }) => (pat.clone(), iter, body),
        _ => return None,
    };
    if !matches!(&block.expr, Some(e) if matches!(e.as_ref(), Expr::Path(n, _) if n == &result_name))
    {
        return None;
    }
    // body: result.push(EXPR) as stmt or trailing expr
    let mapped = match body
        .stmts
        .first()
        .and_then(|s| match s {
            Stmt::Expr(e) => Some(e),
            _ => None,
        })
        .or(body.expr.as_deref())
    {
        Some(Expr::MethodCall {
            receiver,
            method,
            args,
            ..
        }) if method == "push"
            && matches!(receiver.as_ref(), Expr::Path(n, _) if n == &result_name)
            && args.len() == 1 =>
        {
            args[0].clone()
        }
        _ => return None,
    };
    let collection = match iter.as_ref() {
        Expr::Path(n, _) => n.clone(),
        _ => return None,
    };
    let _ = pat;
    Some((result_name, collection, mapped))
}

fn is_vec_new(expr: &Expr) -> bool {
    match expr {
        Expr::Call { func, args, .. } if args.is_empty() => match func.as_ref() {
            Expr::Path(p, _) => p == "Vec::new" || p.ends_with("::new"),
            _ => false,
        },
        Expr::Array(v, _) if v.is_empty() => true,
        _ => false,
    }
}


/// Skip leading pure setup `let`s (immutable bindings) so patterns still match
/// when a function first builds inputs, then runs an imperative loop.
pub fn skip_setup_lets(block: &Block) -> usize {
    let mut i = 0;
    while let Some(Stmt::Let { is_mut: false, .. }) = block.stmts.get(i) {
        i += 1;
    }
    i
}


/// Detect scalar mutation chain:
/// ```ignore
/// let mut x = START;
/// x += a;
/// x *= b;
/// x
/// ```
pub fn detect_scalar_mutation(block: &Block) -> Option<(String, Expr, Vec<(BinOp, Expr)>)> {
    if block.stmts.is_empty() {
        return None;
    }
    let (name, init) = match &block.stmts[0] {
        Stmt::Let {
            name,
            is_mut: true,
            value: Some(v),
            ..
        } => (name.clone(), v.clone()),
        _ => return None,
    };
    if !matches!(&block.expr, Some(e) if matches!(e.as_ref(), Expr::Path(n, _) if n == &name)) {
        return None;
    }
    let mut ops = Vec::new();
    for stmt in block.stmts.iter().skip(1) {
        match stmt {
            Stmt::Expr(Expr::AssignOp {
                op,
                target,
                value,
                ..
            }) if matches!(target.as_ref(), Expr::Path(n, _) if n == &name) => {
                ops.push((*op, value.as_ref().clone()));
            }
            Stmt::Expr(Expr::Assign { target, value, .. })
                if matches!(target.as_ref(), Expr::Path(n, _) if n == &name) =>
            {
                // x = x OP v
                if let Expr::Binary {
                    op,
                    left,
                    right,
                    ..
                } = value.as_ref()
                {
                    if matches!(left.as_ref(), Expr::Path(n, _) if n == &name) {
                        ops.push((*op, right.as_ref().clone()));
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            }
            _ => return None,
        }
    }
    if ops.is_empty() {
        return None;
    }
    Some((name, init, ops))
}

/// Recognised while-loop accumulator pattern:
/// ```ignore
/// let mut n = START;
/// let mut acc = INIT;
/// while n > 0 {
///     acc += n;
///     n -= 1;
/// }
/// acc
/// ```
/// Order of the two mutable lets does not matter; the variable mentioned in the
/// while-condition is treated as the counter.
#[derive(Debug, Clone)]
pub struct WhileAccum {
    pub counter: String,
    pub counter_init: Expr,
    pub acc: String,
    pub acc_init: Expr,
    pub cond: Expr,
    /// Updates inside the loop: (target_name, op, value)
    pub updates: Vec<(String, BinOp, Expr)>,
}

pub fn detect_while_accum(block: &Block) -> Option<WhileAccum> {
    let skip = skip_setup_lets(block);
    if block.stmts.len().saturating_sub(skip) < 3 {
        return None;
    }
    let (a_name, a_init) = match &block.stmts[skip] {
        Stmt::Let {
            name,
            is_mut: true,
            value: Some(v),
            ..
        } => (name.clone(), v.clone()),
        _ => return None,
    };
    let (b_name, b_init) = match &block.stmts[skip + 1] {
        Stmt::Let {
            name,
            is_mut: true,
            value: Some(v),
            ..
        } => (name.clone(), v.clone()),
        _ => return None,
    };
    let (cond, body) = match &block.stmts[skip + 2] {
        Stmt::Expr(Expr::While { cond, body, .. }) => (cond.as_ref().clone(), body),
        _ => return None,
    };
    // Counter = variable appearing in the while condition; acc = the returned one.
    let cond_vars = path_names_in(&cond);
    let (counter, counter_init, acc, acc_init) = if cond_vars.contains(&a_name)
        && !cond_vars.contains(&b_name)
    {
        (a_name, a_init, b_name, b_init)
    } else if cond_vars.contains(&b_name) && !cond_vars.contains(&a_name) {
        (b_name, b_init, a_name, a_init)
    } else if matches!(&block.expr, Some(e) if matches!(e.as_ref(), Expr::Path(n, _) if n == &a_name))
    {
        // Ambiguous condition — prefer returned name as acc.
        (b_name, b_init, a_name, a_init)
    } else if matches!(&block.expr, Some(e) if matches!(e.as_ref(), Expr::Path(n, _) if n == &b_name))
    {
        (a_name, a_init, b_name, b_init)
    } else {
        return None;
    };
    if !matches!(&block.expr, Some(e) if matches!(e.as_ref(), Expr::Path(n, _) if n == &acc)) {
        return None;
    }
    let mut updates = Vec::new();
    for stmt in &body.stmts {
        match stmt {
            Stmt::Expr(Expr::AssignOp {
                op,
                target,
                value,
                ..
            }) => {
                if let Expr::Path(n, _) = target.as_ref() {
                    if n == &counter || n == &acc {
                        updates.push((n.clone(), *op, value.as_ref().clone()));
                        continue;
                    }
                }
                return None;
            }
            _ => return None,
        }
    }
    // Also allow trailing assign-op as body expr
    if let Some(Expr::AssignOp {
        op,
        target,
        value,
        ..
    }) = body.expr.as_deref()
    {
        if let Expr::Path(n, _) = target.as_ref() {
            if n == &counter || n == &acc {
                updates.push((n.clone(), *op, value.as_ref().clone()));
            }
        }
    }
    if updates.is_empty() {
        return None;
    }
    Some(WhileAccum {
        counter,
        counter_init,
        acc,
        acc_init,
        cond,
        updates,
    })
}

fn path_names_in(expr: &Expr) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(e: &Expr, out: &mut Vec<String>) {
        match e {
            Expr::Path(n, _) => out.push(n.clone()),
            Expr::Binary { left, right, .. } => {
                walk(left, out);
                walk(right, out);
            }
            Expr::Unary { expr, .. } => walk(expr, out),
            Expr::Call { func, args, .. } => {
                walk(func, out);
                for a in args {
                    walk(a, out);
                }
            }
            Expr::MethodCall { receiver, args, .. } => {
                walk(receiver, out);
                for a in args {
                    walk(a, out);
                }
            }
            Expr::Index { base, index, .. } => {
                walk(base, out);
                walk(index, out);
            }
            Expr::Field { base, .. }
            | Expr::Reference { expr: base, .. }
            | Expr::Deref { expr: base, .. } => {
                walk(base, out);
            }
            Expr::Tuple(es, _) | Expr::Array(es, _) => {
                for e in es {
                    walk(e, out);
                }
            }
            _ => {}
        }
    }
    walk(expr, &mut out);
    out
}

/// Detect conditional map via push:
/// ```ignore
/// let mut result = Vec::new();
/// for x in xs {
///     if COND { result.push(MAPPED); }
/// }
/// result
/// ```
/// → filter + map (or mapMaybe)
pub fn detect_filter_map_push(block: &Block) -> Option<(String, String, Expr, Expr)> {
    let skip = skip_setup_lets(block);
    if block.stmts.len().saturating_sub(skip) < 2 {
        return None;
    }
    let result_name = match &block.stmts[skip] {
        Stmt::Let {
            name,
            is_mut: true,
            value: Some(v),
            ..
        } if is_vec_new(v) => name.clone(),
        _ => return None,
    };
    let (pat, iter, body) = match &block.stmts[skip + 1] {
        Stmt::Expr(Expr::For {
            pat, iter, body, ..
        }) => (pat.clone(), iter, body),
        _ => return None,
    };
    if !matches!(&block.expr, Some(e) if matches!(e.as_ref(), Expr::Path(n, _) if n == &result_name))
    {
        return None;
    }
    let (cond, then_branch) = for_body_if(body)?;
    // then: result.push(EXPR)
    let mapped = match then_branch
        .stmts
        .first()
        .and_then(|s| match s {
            Stmt::Expr(e) => Some(e),
            _ => None,
        })
        .or(then_branch.expr.as_deref())
    {
        Some(Expr::MethodCall {
            receiver,
            method,
            args,
            ..
        }) if method == "push"
            && matches!(receiver.as_ref(), Expr::Path(n, _) if n == &result_name)
            && args.len() == 1 =>
        {
            args[0].clone()
        }
        _ => return None,
    };
    let collection = match iter.as_ref() {
        Expr::Path(n, _) => n.clone(),
        _ => return None,
    };
    let _ = pat;
    Some((result_name, collection, cond.clone(), mapped))
}

/// Detect fold/reduction: `let mut acc = INIT; for x in xs { acc += EXPR }; acc`
/// where EXPR may be more than just `x` (e.g. `x * x`).
pub fn detect_fold_add(block: &Block) -> Option<(String, Expr, String, String, Expr)> {
    let skip = skip_setup_lets(block);
    if block.stmts.len().saturating_sub(skip) < 2 {
        return None;
    }
    let (acc_name, init) = match &block.stmts[skip] {
        Stmt::Let {
            name,
            is_mut: true,
            value: Some(v),
            ..
        } => (name.clone(), v.clone()),
        _ => return None,
    };
    let (pat, iter, body) = match &block.stmts[skip + 1] {
        Stmt::Expr(Expr::For {
            pat, iter, body, ..
        }) => (pat.clone(), iter, body),
        _ => return None,
    };
    if !matches!(&block.expr, Some(e) if matches!(e.as_ref(), Expr::Path(n, _) if n == &acc_name)) {
        return None;
    }
    let assign = body
        .stmts
        .first()
        .and_then(|s| match s {
            Stmt::Expr(e) => Some(e),
            _ => None,
        })
        .or(body.expr.as_deref());
    let addend = match assign {
        Some(Expr::AssignOp {
            op: BinOp::Add,
            target,
            value,
            ..
        }) if matches!(target.as_ref(), Expr::Path(n, _) if n == &acc_name) => {
            value.as_ref().clone()
        }
        _ => return None,
    };
    // Plain sum (addend == pat) is already handled elsewhere — still OK to return
    let collection = match iter.as_ref() {
        Expr::Path(n, _) => n.clone(),
        _ => return None,
    };
    Some((acc_name, init, pat, collection, addend))
}

/// Detect multi-accumulator loop returning a tuple:
/// ```ignore
/// let mut a = 0; let mut b = 0;
/// for x in xs { a += x; b += 1; }
/// (a, b)
/// ```
pub fn detect_multi_accum(block: &Block) -> Option<(Vec<(String, Expr)>, String, String, Vec<(String, BinOp, Expr)>)> {
    // Find consecutive mut lets, then a for, then tuple of those names
    let mut inits = Vec::new();
    let mut i = 0;
    while let Some(Stmt::Let {
        name,
        is_mut: true,
        value: Some(v),
        ..
    }) = block.stmts.get(i)
    {
        inits.push((name.clone(), v.clone()));
        i += 1;
    }
    if inits.len() < 2 {
        return None;
    }
    let (pat, iter, body) = match block.stmts.get(i) {
        Some(Stmt::Expr(Expr::For {
            pat, iter, body, ..
        })) => (pat.clone(), iter, body),
        _ => return None,
    };
    i += 1;
    if i != block.stmts.len() {
        return None;
    }
    let names: Vec<_> = inits.iter().map(|(n, _)| n.clone()).collect();
    match block.expr.as_deref() {
        Some(Expr::Tuple(es, _)) if es.len() == names.len() => {
            for (e, n) in es.iter().zip(&names) {
                if !matches!(e, Expr::Path(p, _) if p == n) {
                    return None;
                }
            }
        }
        _ => return None,
    }
    let mut updates = Vec::new();
    for stmt in &body.stmts {
        match stmt {
            Stmt::Expr(Expr::AssignOp {
                op,
                target,
                value,
                ..
            }) => {
                if let Expr::Path(n, _) = target.as_ref() {
                    if names.iter().any(|a| a == n) {
                        updates.push((n.clone(), *op, value.as_ref().clone()));
                        continue;
                    }
                }
                return None;
            }
            _ => return None,
        }
    }
    if let Some(Expr::AssignOp {
        op,
        target,
        value,
        ..
    }) = body.expr.as_deref()
    {
        if let Expr::Path(n, _) = target.as_ref() {
            if names.iter().any(|a| a == n) {
                updates.push((n.clone(), *op, value.as_ref().clone()));
            }
        }
    }
    if updates.is_empty() {
        return None;
    }
    let collection = match iter.as_ref() {
        Expr::Path(n, _) => n.clone(),
        _ => return None,
    };
    Some((inits, pat, collection, updates))
}

/// Detect record field updates then return the record:
/// ```ignore
/// user.name = name;
/// user
/// ```
/// or with AssignOp on fields. Also accepts a trailing-less block of field
/// updates (typical `&mut self` / `&mut T` methods that return `()`).
/// The bool is true when the Rust block explicitly returns the record.
pub fn detect_record_updates(
    block: &Block,
) -> Option<(String, Vec<(String, Option<BinOp>, Expr)>, bool)> {
    if block.stmts.is_empty() {
        return None;
    }
    let (record, explicit_return) = match block.expr.as_deref() {
        Some(Expr::Path(n, _)) => (n.clone(), true),
        None => {
            // Infer record from first field update target
            let first = block.stmts.first()?;
            let name = match first {
                Stmt::Expr(Expr::Assign { target, .. })
                | Stmt::Expr(Expr::AssignOp { target, .. }) => match target.as_ref() {
                    Expr::Field { base, .. } => match base.as_ref() {
                        Expr::Path(n, _) => n.clone(),
                        _ => return None,
                    },
                    _ => return None,
                },
                _ => return None,
            };
            (name, false)
        }
        _ => return None,
    };
    let mut updates = Vec::new();
    for stmt in &block.stmts {
        match stmt {
            Stmt::Expr(Expr::Assign { target, value, .. }) => {
                if let Expr::Field { base, field, .. } = target.as_ref() {
                    if matches!(base.as_ref(), Expr::Path(n, _) if n == &record) {
                        updates.push((field.clone(), None, value.as_ref().clone()));
                        continue;
                    }
                }
                return None;
            }
            Stmt::Expr(Expr::AssignOp {
                op,
                target,
                value,
                ..
            }) => {
                if let Expr::Field { base, field, .. } = target.as_ref() {
                    if matches!(base.as_ref(), Expr::Path(n, _) if n == &record) {
                        updates.push((field.clone(), Some(*op), value.as_ref().clone()));
                        continue;
                    }
                }
                return None;
            }
            _ => return None,
        }
    }
    if updates.is_empty() {
        return None;
    }
    Some((record, updates, explicit_return))
}

/// Detect early-return chains:
/// ```ignore
/// if cond { return v; }
/// …
/// trailing
/// ```
/// Returns the first (cond, value) and whether the then-branch is a bare return.
pub fn is_early_return_if(stmt: &Stmt) -> Option<(Expr, Expr)> {
    match stmt {
        Stmt::Expr(Expr::If {
            cond,
            then_branch,
            else_branch: None,
            ..
        }) => {
            // then is only `return v;` or expr return
            if then_branch.stmts.len() == 1 && then_branch.expr.is_none() {
                if let Stmt::Return(Some(v), _) = &then_branch.stmts[0] {
                    return Some(((**cond).clone(), v.clone()));
                }
            }
            if then_branch.stmts.is_empty() {
                if let Some(Expr::Return(Some(v), _)) = then_branch.expr.as_deref() {
                    return Some(((**cond).clone(), (**v).clone()));
                }
            }
            None
        }
        _ => None,
    }
}

/// Detect max-scan:
/// ```ignore
/// let mut best = xs[0];
/// for v in xs { if v > best { best = v; } }
/// best
/// ```
pub fn detect_max_scan(block: &Block) -> Option<String> {
    let skip = skip_setup_lets(block);
    if block.stmts.len().saturating_sub(skip) < 2 {
        return None;
    }
    let (best, collection_from_init) = match &block.stmts[skip] {
        Stmt::Let {
            name,
            is_mut: true,
            value: Some(Expr::Index { base, .. }),
            ..
        } => {
            let col = match base.as_ref() {
                Expr::Path(n, _) => n.clone(),
                _ => return None,
            };
            (name.clone(), col)
        }
        _ => return None,
    };
    let (pat, iter, body) = match &block.stmts[skip + 1] {
        Stmt::Expr(Expr::For {
            pat, iter, body, ..
        }) => (pat.clone(), iter, body),
        _ => return None,
    };
    if !matches!(&block.expr, Some(e) if matches!(e.as_ref(), Expr::Path(n, _) if n == &best)) {
        return None;
    }
    let collection = match iter.as_ref() {
        Expr::Path(n, _) => n.clone(),
        _ => return None,
    };
    if collection != collection_from_init {
        return None;
    }
    let (cond, then_branch) = for_body_if(body)?;
    // cond: v > best
    let ok_cond = matches!(
        cond,
        Expr::Binary {
            op: BinOp::Gt,
            left,
            right,
            ..
        } if matches!(left.as_ref(), Expr::Path(n, _) if n == &pat)
            && matches!(right.as_ref(), Expr::Path(n, _) if n == &best)
    );
    if !ok_cond {
        return None;
    }
    let ok_assign = matches!(
        then_branch.stmts.first().and_then(|s| match s {
            Stmt::Expr(e) => Some(e),
            _ => None,
        }).or(then_branch.expr.as_deref()),
        Some(Expr::Assign { target, value, .. })
            if matches!(target.as_ref(), Expr::Path(n, _) if n == &best)
                && matches!(value.as_ref(), Expr::Path(n, _) if n == &pat)
    );
    if !ok_assign {
        return None;
    }
    Some(collection)
}

/// Detect fold-with-continue/break:
/// ```ignore
/// let mut total = 0;
/// for value in values {
///     if value < 0 { continue; }
///     total += value;
///     if total > 100 { break; }
/// }
/// total
/// ```
pub struct ContinueBreakFold {
    pub acc: String,
    pub acc_init: Expr,
    pub pat: String,
    pub collection: String,
    pub skip_cond: Option<Expr>,
    pub addend: Expr,
    pub break_cond: Option<Expr>,
}

pub fn detect_continue_break_fold(block: &Block) -> Option<ContinueBreakFold> {
    let skip = skip_setup_lets(block);
    if block.stmts.len().saturating_sub(skip) < 2 {
        return None;
    }
    let (acc, acc_init) = match &block.stmts[skip] {
        Stmt::Let {
            name,
            is_mut: true,
            value: Some(v),
            ..
        } => (name.clone(), v.clone()),
        _ => return None,
    };
    let (pat, iter, body) = match &block.stmts[skip + 1] {
        Stmt::Expr(Expr::For {
            pat, iter, body, ..
        }) => (pat.clone(), iter, body),
        _ => return None,
    };
    if !matches!(&block.expr, Some(e) if matches!(e.as_ref(), Expr::Path(n, _) if n == &acc)) {
        return None;
    }
    let collection = match iter.as_ref() {
        Expr::Path(n, _) => n.clone(),
        _ => return None,
    };

    let mut skip_cond = None;
    let mut break_cond = None;
    let mut addend = None;

    let mut consider_expr = |expr: &Expr| -> Option<()> {
        match expr {
            Expr::If {
                cond,
                then_branch,
                else_branch: None,
                ..
            } => {
                let is_continue = matches!(then_branch.stmts.first(), Some(Stmt::Continue(_)));
                let is_break = matches!(then_branch.stmts.first(), Some(Stmt::Break(_)));
                if is_continue {
                    skip_cond = Some(cond.as_ref().clone());
                    Some(())
                } else if is_break {
                    break_cond = Some(cond.as_ref().clone());
                    Some(())
                } else {
                    None
                }
            }
            Expr::AssignOp {
                op: BinOp::Add,
                target,
                value,
                ..
            } if matches!(target.as_ref(), Expr::Path(n, _) if n == &acc) => {
                addend = Some(value.as_ref().clone());
                Some(())
            }
            _ => None,
        }
    };

    for stmt in &body.stmts {
        match stmt {
            Stmt::Expr(e) => {
                consider_expr(e)?;
            }
            Stmt::Continue(_) | Stmt::Break(_) => return None,
            _ => return None,
        }
    }
    // Trailing expression of the for-body (common for final `if … { break; }`)
    if let Some(e) = body.expr.as_deref() {
        consider_expr(e)?;
    }

    let addend = addend?;
    if skip_cond.is_none() && break_cond.is_none() {
        return None; // plain sum handled elsewhere
    }
    Some(ContinueBreakFold {
        acc,
        acc_init,
        pat,
        collection,
        skip_cond,
        addend,
        break_cond,
    })
}

/// Peel iterator method chains into (base, steps).
/// e.g. values.iter().filter(...).map(...).collect()
#[derive(Debug, Clone)]
pub struct IterStep {
    pub method: String,
    pub args: Vec<Expr>,
}

pub fn peel_iterator_chain(expr: &Expr) -> Option<(Expr, Vec<IterStep>)> {
    let mut steps = Vec::new();
    let mut current = expr;
    loop {
        match current {
            Expr::MethodCall {
                receiver,
                method,
                args,
                ..
            } => {
                let m = method.as_str();
                if matches!(
                    m,
                    "iter"
                        | "into_iter"
                        | "iter_mut"
                        | "filter"
                        | "map"
                        | "collect"
                        | "sum"
                        | "fold"
                        | "cloned"
                        | "copied"
                        | "enumerate"
                        | "take"
                        | "skip"
                        | "rev"
                        | "count"
                        | "any"
                        | "all"
                        | "find"
                        | "flatten"
                        | "flat_map"
                        | "chain"
                        | "zip"
                        | "filter_map"
                        | "partition"
                        | "max"
                        | "min"
                        | "product"
                        | "position"
                        | "rposition"
                ) {
                    steps.push(IterStep {
                        method: method.clone(),
                        args: args.clone(),
                    });
                    current = receiver;
                } else {
                    break;
                }
            }
            _ => break,
        }
    }
    if steps.is_empty() {
        return None;
    }
    steps.reverse();
    // Must start with iter/into_iter or be a chain ending in collect/sum
    let has_terminal = steps.iter().any(|s| {
        matches!(
            s.method.as_str(),
            "collect" | "sum" | "fold" | "count" | "any" | "all" | "find"
        )
    }) || steps
        .iter()
        .any(|s| matches!(s.method.as_str(), "iter" | "into_iter" | "iter_mut"));
    if !has_terminal {
        return None;
    }
    Some((current.clone(), steps))
}

/// Indexed linear search:
/// ```ignore
/// let mut i = 0;
/// for v in values {
///     if v == target { return Some(i); }
///     i += 1;
/// }
/// None
/// ```
/// → `elemIndex` / `findIndex`
#[derive(Debug, Clone)]
pub struct IndexedSearch {
    pub index: String,
    pub pat: String,
    pub collection: String,
    pub target: Expr,
}

pub fn detect_indexed_search(block: &Block) -> Option<IndexedSearch> {
    let skip = skip_setup_lets(block);
    if block.stmts.len().saturating_sub(skip) < 2 {
        return None;
    }
    // Trailing must be None
    match &block.expr {
        Some(e) if matches!(e.as_ref(), Expr::Path(n, _) if n == "None" || n.ends_with("::None")) => {}
        _ => return None,
    }
    let index = match &block.stmts[skip] {
        Stmt::Let {
            name,
            is_mut: true,
            value: Some(Expr::Lit(Lit::Int(0), _)),
            ..
        } => name.clone(),
        _ => return None,
    };
    let (pat, iter, body) = match &block.stmts[skip + 1] {
        Stmt::Expr(Expr::For {
            pat, iter, body, ..
        }) => (pat.clone(), iter, body),
        _ => return None,
    };
    let collection = match iter.as_ref() {
        Expr::Path(n, _) => n.clone(),
        _ => return None,
    };

    // Body: if pat == TARGET { return Some(index); }; index += 1
    // or if as trailing expr
    let mut target = None;
    let mut saw_inc = false;

    let mut consider = |expr: &Expr| -> bool {
        match expr {
            Expr::If {
                cond,
                then_branch,
                else_branch: None,
                ..
            } => {
                // cond: pat == target
                let t = match cond.as_ref() {
                    Expr::Binary {
                        op: BinOp::Eq,
                        left,
                        right,
                        ..
                    } if matches!(left.as_ref(), Expr::Path(n, _) if n == &pat) => {
                        Some(right.as_ref().clone())
                    }
                    Expr::Binary {
                        op: BinOp::Eq,
                        left,
                        right,
                        ..
                    } if matches!(right.as_ref(), Expr::Path(n, _) if n == &pat) => {
                        Some(left.as_ref().clone())
                    }
                    _ => None,
                };
                // then: return Some(index)
                let returns_some_i = match then_branch.stmts.first() {
                    Some(Stmt::Return(Some(Expr::Call { func, args, .. }), _))
                        if matches!(func.as_ref(), Expr::Path(n, _) if n == "Some" || n.ends_with("::Some"))
                            && args.len() == 1
                            && matches!(&args[0], Expr::Path(n, _) if n == &index) =>
                    {
                        true
                    }
                    _ => matches!(
                        then_branch.expr.as_deref(),
                        Some(Expr::Return(Some(v), _))
                            if matches!(
                                v.as_ref(),
                                Expr::Call { func, args, .. }
                                    if matches!(func.as_ref(), Expr::Path(n, _) if n == "Some" || n.ends_with("::Some"))
                                        && args.len() == 1
                                        && matches!(&args[0], Expr::Path(n, _) if n == &index)
                            )
                    ),
                };
                if returns_some_i {
                    if let Some(t) = t {
                        target = Some(t);
                        return true;
                    }
                }
                false
            }
            Expr::AssignOp {
                op: BinOp::Add,
                target: tgt,
                value,
                ..
            } if matches!(tgt.as_ref(), Expr::Path(n, _) if n == &index)
                && matches!(value.as_ref(), Expr::Lit(Lit::Int(1), _)) =>
            {
                saw_inc = true;
                true
            }
            _ => false,
        }
    };

    for stmt in &body.stmts {
        match stmt {
            Stmt::Expr(e) => {
                if !consider(e) {
                    return None;
                }
            }
            Stmt::Return(Some(Expr::Call { func, args, .. }), _)
                if matches!(func.as_ref(), Expr::Path(n, _) if n == "Some" || n.ends_with("::Some")) =>
            {
                // return Some(i) as bare statement without if — not this pattern
                let _ = args;
                return None;
            }
            _ => return None,
        }
    }
    if let Some(e) = body.expr.as_deref() {
        if !consider(e) {
            return None;
        }
    }
    let target = target?;
    if !saw_inc {
        return None;
    }
    Some(IndexedSearch {
        index,
        pat,
        collection,
        target,
    })
}

/// Adjacent ordered scan with early `return false`:
/// ```ignore
/// let mut prev = values[0];
/// for v in values {
///     if v < prev { return false; }
///     prev = v;
/// }
/// true
/// ```
/// → `and (zipWith (<=) xs (tail xs))`
#[derive(Debug, Clone)]
pub struct AdjacentOrderScan {
    pub collection: String,
    pub cmp: BinOp, // the failing comparison, e.g. Lt means sorted ascending
}

pub fn detect_adjacent_order_scan(block: &Block) -> Option<AdjacentOrderScan> {
    let skip = skip_setup_lets(block);
    if block.stmts.len().saturating_sub(skip) < 2 {
        return None;
    }
    if !matches!(&block.expr, Some(e) if matches!(e.as_ref(), Expr::Lit(Lit::Bool(true), _))) {
        return None;
    }
    let (prev, collection_from_init) = match &block.stmts[skip] {
        Stmt::Let {
            name,
            is_mut: true,
            value: Some(Expr::Index { base, index, .. }),
            ..
        } if matches!(index.as_ref(), Expr::Lit(Lit::Int(0), _)) => {
            let col = match base.as_ref() {
                Expr::Path(n, _) => n.clone(),
                _ => return None,
            };
            (name.clone(), col)
        }
        _ => return None,
    };
    let (pat, iter, body) = match &block.stmts[skip + 1] {
        Stmt::Expr(Expr::For {
            pat, iter, body, ..
        }) => (pat.clone(), iter, body),
        _ => return None,
    };
    let collection = match iter.as_ref() {
        Expr::Path(n, _) => n.clone(),
        _ => return None,
    };
    if collection != collection_from_init {
        return None;
    }

    let mut fail_op = None;
    let mut saw_assign_prev = false;

    let mut consider = |expr: &Expr| -> bool {
        match expr {
            Expr::If {
                cond,
                then_branch,
                else_branch: None,
                ..
            } => {
                let op = match cond.as_ref() {
                    Expr::Binary {
                        op,
                        left,
                        right,
                        ..
                    } if matches!(left.as_ref(), Expr::Path(n, _) if n == &pat)
                        && matches!(right.as_ref(), Expr::Path(n, _) if n == &prev)
                        && matches!(op, BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge) =>
                    {
                        Some(*op)
                    }
                    _ => None,
                };
                let returns_false = matches!(
                    then_branch.stmts.first(),
                    Some(Stmt::Return(Some(Expr::Lit(Lit::Bool(false), _)), _))
                ) || matches!(
                    then_branch.expr.as_deref(),
                    Some(Expr::Return(Some(v), _))
                        if matches!(v.as_ref(), Expr::Lit(Lit::Bool(false), _))
                ) || matches!(
                    then_branch.expr.as_deref(),
                    Some(Expr::Lit(Lit::Bool(false), _))
                );
                if returns_false {
                    if let Some(op) = op {
                        fail_op = Some(op);
                        return true;
                    }
                }
                false
            }
            Expr::Assign { target, value, .. }
                if matches!(target.as_ref(), Expr::Path(n, _) if n == &prev)
                    && matches!(value.as_ref(), Expr::Path(n, _) if n == &pat) =>
            {
                saw_assign_prev = true;
                true
            }
            _ => false,
        }
    };

    for stmt in &body.stmts {
        match stmt {
            Stmt::Expr(e) => {
                if !consider(e) {
                    return None;
                }
            }
            _ => return None,
        }
    }
    if let Some(e) = body.expr.as_deref() {
        if !consider(e) {
            return None;
        }
    }
    let fail_op = fail_op?;
    if !saw_assign_prev {
        return None;
    }
    Some(AdjacentOrderScan {
        collection,
        cmp: fail_op,
    })
}

/// Euclidean algorithm / two-variable while with temporary swap:
/// ```ignore
/// while b != 0 {
///     let t = b;
///     b = a % b;
///     a = t;
/// }
/// a
/// ```
#[derive(Debug, Clone)]
pub struct EuclidWhile {
    pub a: String,
    pub b: String,
    /// Optional inits when a/b are local mut lets rather than parameters.
    pub a_init: Option<Expr>,
    pub b_init: Option<Expr>,
}

pub fn detect_euclid_while(block: &Block) -> Option<EuclidWhile> {
    let skip = skip_setup_lets(block);
    let mut a_init = None;
    let mut b_init = None;
    let mut idx = skip;

    // Optional: let mut a = …; let mut b = …;
    if let Some(Stmt::Let {
        name,
        is_mut: true,
        value: Some(v),
        ..
    }) = block.stmts.get(idx)
    {
        // Look ahead for second mut let + while
        if let Some(Stmt::Let {
            name: name2,
            is_mut: true,
            value: Some(v2),
            ..
        }) = block.stmts.get(idx + 1)
        {
            if matches!(
                block.stmts.get(idx + 2),
                Some(Stmt::Expr(Expr::While { .. }))
            ) {
                a_init = Some((name.clone(), v.clone()));
                b_init = Some((name2.clone(), v2.clone()));
                idx += 2;
            }
        }
    }

    let (cond, body) = match block.stmts.get(idx) {
        Some(Stmt::Expr(Expr::While { cond, body, .. })) => (cond.as_ref(), body),
        _ => return None,
    };

    // Cond: b != 0
    let b_name = match cond {
        Expr::Binary {
            op: BinOp::Ne,
            left,
            right,
            ..
        } if matches!(right.as_ref(), Expr::Lit(Lit::Int(0), _)) => match left.as_ref() {
            Expr::Path(n, _) => n.clone(),
            _ => return None,
        },
        _ => return None,
    };

    // Trailing: a
    let a_name = match &block.expr {
        Some(e) => match e.as_ref() {
            Expr::Path(n, _) if n != &b_name => n.clone(),
            _ => return None,
        },
        None => return None,
    };

    // Body: let t = b; b = a % b; a = t
    // Allow as stmts and/or trailing expr
    let stmts: Vec<&Stmt> = body.stmts.iter().collect();
    // If trailing is an assign, treat as stmt-like via synthetic — handle separately
    let trailing_assign = body.expr.as_deref();

    // Expect: Let t = b, Assign b = a % b, Assign a = t
    if stmts.len() < 2 {
        return None;
    }
    let t_name = match stmts[0] {
        Stmt::Let {
            name,
            value: Some(Expr::Path(n, _)),
            ..
        } if n == &b_name => name.clone(),
        _ => return None,
    };

    // Find b = a % b and a = t among remaining stmts + trailing
    let mut rest: Vec<&Expr> = stmts[1..]
        .iter()
        .filter_map(|s| match s {
            Stmt::Expr(e) => Some(e),
            _ => None,
        })
        .collect();
    if let Some(e) = trailing_assign {
        rest.push(e);
    }
    if rest.len() < 2 {
        return None;
    }

    let mut saw_b_update = false;
    let mut saw_a_update = false;
    for e in rest {
        match e {
            Expr::Assign { target, value, .. } => match target.as_ref() {
                Expr::Path(n, _) if n == &b_name => {
                    // a % b
                    let ok = matches!(
                        value.as_ref(),
                        Expr::Binary {
                            op: BinOp::Rem,
                            left,
                            right,
                            ..
                        } if matches!(left.as_ref(), Expr::Path(l, _) if l == &a_name)
                            && matches!(right.as_ref(), Expr::Path(r, _) if r == &b_name)
                    );
                    if !ok {
                        return None;
                    }
                    saw_b_update = true;
                }
                Expr::Path(n, _) if n == &a_name => {
                    if !matches!(value.as_ref(), Expr::Path(t, _) if t == &t_name) {
                        return None;
                    }
                    saw_a_update = true;
                }
                _ => return None,
            },
            _ => return None,
        }
    }
    if !(saw_a_update && saw_b_update) {
        return None;
    }

    // If we captured inits, names must match
    let (a_init_e, b_init_e) = match (a_init, b_init) {
        (Some((an, av)), Some((bn, bv))) if an == a_name && bn == b_name => {
            (Some(av), Some(bv))
        }
        (None, None) => (None, None),
        _ => return None,
    };

    let _ = idx;
    Some(EuclidWhile {
        a: a_name,
        b: b_name,
        a_init: a_init_e,
        b_init: b_init_e,
    })
}
