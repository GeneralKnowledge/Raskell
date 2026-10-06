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
    if block.stmts.len() < 2 {
        return None;
    }

    let acc_name = match &block.stmts[0] {
        Stmt::Let {
            name,
            is_mut: true,
            value: Some(Expr::Lit(Lit::Int(0), _)),
            ..
        } => name.clone(),
        _ => return None,
    };

    let (pat, iter, body) = match &block.stmts[1] {
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
    if block.stmts.len() < 2 {
        return None;
    }
    let acc_name = match &block.stmts[0] {
        Stmt::Let {
            name,
            is_mut: true,
            value: Some(Expr::Lit(Lit::Int(0), _)),
            ..
        } => name.clone(),
        _ => return None,
    };
    let (pat, iter, body) = match &block.stmts[1] {
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
    if block.stmts.len() < 2 {
        return None;
    }
    let result_name = match &block.stmts[0] {
        Stmt::Let {
            name,
            is_mut: true,
            value: Some(v),
            ..
        } if is_vec_new(v) => name.clone(),
        _ => return None,
    };
    let (pat, iter, body) = match &block.stmts[1] {
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
    if block.stmts.len() < 3 {
        return None;
    }
    let (counter, counter_init) = match &block.stmts[0] {
        Stmt::Let {
            name,
            is_mut: true,
            value: Some(v),
            ..
        } => (name.clone(), v.clone()),
        _ => return None,
    };
    let (acc, acc_init) = match &block.stmts[1] {
        Stmt::Let {
            name,
            is_mut: true,
            value: Some(v),
            ..
        } => (name.clone(), v.clone()),
        _ => return None,
    };
    if !matches!(&block.expr, Some(e) if matches!(e.as_ref(), Expr::Path(n, _) if n == &acc)) {
        return None;
    }
    let (cond, body) = match &block.stmts[2] {
        Stmt::Expr(Expr::While { cond, body, .. }) => (cond.as_ref().clone(), body),
        _ => return None,
    };
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
