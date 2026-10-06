//! Composable analysis of imperative `for` loops.
//!
//! Instead of matching one brittle AST template per detector, we:
//! 1. Peel loop-local immutable `let`s and substitute them
//! 2. Classify remaining body actions (skip / push / accumulate / early return)
//! 3. Recognise semantic shapes (filter+map build, filtered fold, find, …)

use crate::ast::*;

/// Immutable bindings introduced at the start of a loop body (temps).
#[derive(Debug, Clone)]
pub struct LoopLocals {
    pub bindings: Vec<(String, Expr)>,
}

impl LoopLocals {
    pub fn substitute(&self, expr: &Expr) -> Expr {
        let mut e = expr.clone();
        // Substitute in declaration order so later temps see earlier ones.
        for (name, val) in &self.bindings {
            e = subst_path(&e, name, val);
        }
        strip_ref_noise_expr(&e)
    }
}

/// Normalised action inside a for-loop body (after peeling locals).
#[derive(Debug, Clone)]
pub enum LoopAction {
    /// `if COND { continue; }`
    SkipIf(Expr),
    /// `if COND { result.push(VAL); }`
    GuardedPush { cond: Expr, value: Expr },
    /// `result.push(VAL);`
    Push(Expr),
    /// `acc += VAL;`
    AccAdd { name: String, value: Expr },
    /// `if COND { acc += VAL; }`
    GuardedAccAdd { cond: Expr, name: String, value: Expr },
    /// `return Some(VAL);` or `if COND { return Some(VAL); }`
    EarlySome { cond: Option<Expr>, value: Expr },
    /// `if COND { return false; }`
    EarlyFalse { cond: Expr },
    /// `prev = VAL;`
    Assign { name: String, value: Expr },
}

/// Collect immutable `let`s anywhere in the for-body and return the remaining expressions.
/// Temps are substituted in declaration order before classification.
pub fn peel_loop_lets(body: &Block) -> (LoopLocals, Vec<Expr>, Option<Expr>) {
    let mut bindings = Vec::new();
    let mut rest = Vec::new();
    for stmt in &body.stmts {
        match stmt {
            Stmt::Let {
                name,
                is_mut: false,
                value: Some(v),
                ..
            } => {
                // Substitute earlier temps into this binding's RHS
                let locals = LoopLocals {
                    bindings: bindings.clone(),
                };
                bindings.push((name.clone(), locals.substitute(v)));
            }
            Stmt::Expr(e) => rest.push(e.clone()),
            Stmt::Continue(_) => {
                // Bare continue without if — not a recognised form
                rest.push(Expr::Path("__bare_continue__".into(), Span::default()));
            }
            Stmt::Break(_) => {
                rest.push(Expr::Path("__bare_break__".into(), Span::default()));
            }
            Stmt::Return(Some(v), span) => {
                rest.push(Expr::Return(Some(Box::new(v.clone())), *span));
            }
            Stmt::Return(None, _) | Stmt::Let { .. } => {
                rest.push(Expr::Path("__unsupported_stmt__".into(), Span::default()));
            }
        }
    }
    let trailing = body.expr.as_deref().cloned();
    (LoopLocals { bindings }, rest, trailing)
}

/// Classify expressions in a for-body into loop actions.
pub fn classify_actions(
    locals: &LoopLocals,
    exprs: &[Expr],
    trailing: Option<&Expr>,
) -> Option<Vec<LoopAction>> {
    let mut actions = Vec::new();
    let mut all: Vec<&Expr> = exprs.iter().collect();
    if let Some(t) = trailing {
        all.push(t);
    }
    for e in all {
        actions.push(classify_one(locals, e)?);
    }
    Some(actions)
}

fn classify_one(locals: &LoopLocals, expr: &Expr) -> Option<LoopAction> {
    match expr {
        Expr::If {
            cond,
            then_branch,
            else_branch: None,
            ..
        } => {
            let cond = locals.substitute(cond);
            if is_continue_only(then_branch) {
                return Some(LoopAction::SkipIf(cond));
            }
            if let Some(v) = is_return_some(then_branch) {
                return Some(LoopAction::EarlySome {
                    cond: Some(cond),
                    value: locals.substitute(&v),
                });
            }
            if is_return_false(then_branch) {
                return Some(LoopAction::EarlyFalse { cond });
            }
            if let Some((name, val)) = is_acc_add(then_branch) {
                return Some(LoopAction::GuardedAccAdd {
                    cond,
                    name,
                    value: locals.substitute(&val),
                });
            }
            if let Some(val) = is_push(then_branch) {
                return Some(LoopAction::GuardedPush {
                    cond,
                    value: locals.substitute(&val),
                });
            }
            None
        }
        Expr::AssignOp {
            op: BinOp::Add,
            target,
            value,
            ..
        } => {
            let name = match target.as_ref() {
                Expr::Path(n, _) => n.clone(),
                _ => return None,
            };
            Some(LoopAction::AccAdd {
                name,
                value: locals.substitute(value),
            })
        }
        Expr::Assign { target, value, .. } => {
            let name = match target.as_ref() {
                Expr::Path(n, _) => n.clone(),
                _ => return None,
            };
            Some(LoopAction::Assign {
                name,
                value: locals.substitute(value),
            })
        }
        Expr::MethodCall {
            receiver,
            method,
            args,
            ..
        } if method == "push" && args.len() == 1 => {
            let _ = receiver;
            Some(LoopAction::Push(locals.substitute(&args[0])))
        }
        Expr::Return(Some(v), _) => {
            if let Some(inner) = as_some_call(v) {
                Some(LoopAction::EarlySome {
                    cond: None,
                    value: locals.substitute(&inner),
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

fn is_continue_only(b: &Block) -> bool {
    matches!(b.stmts.first(), Some(Stmt::Continue(_)))
        || matches!(b.expr.as_deref(), Some(Expr::Unsupported { .. })) // unlikely
        || (b.stmts.is_empty() && b.expr.is_none() && false)
        || (b.stmts.len() == 1 && matches!(b.stmts[0], Stmt::Continue(_)))
}

fn is_return_false(b: &Block) -> bool {
    matches!(
        b.stmts.first(),
        Some(Stmt::Return(Some(Expr::Lit(Lit::Bool(false), _)), _))
    ) || matches!(
        b.expr.as_deref(),
        Some(Expr::Return(Some(v), _)) if matches!(v.as_ref(), Expr::Lit(Lit::Bool(false), _))
    ) || matches!(b.expr.as_deref(), Some(Expr::Lit(Lit::Bool(false), _)))
}

fn is_return_some(b: &Block) -> Option<Expr> {
    match b.stmts.first() {
        Some(Stmt::Return(Some(v), _)) => as_some_call(v),
        _ => match b.expr.as_deref() {
            Some(Expr::Return(Some(v), _)) => as_some_call(v),
            Some(e) => as_some_call(e),
            None => None,
        },
    }
}

fn as_some_call(expr: &Expr) -> Option<Expr> {
    match expr {
        Expr::Call { func, args, .. }
            if matches!(func.as_ref(), Expr::Path(n, _) if n == "Some" || n.ends_with("::Some"))
                && args.len() == 1 =>
        {
            Some(args[0].clone())
        }
        _ => None,
    }
}

fn is_acc_add(b: &Block) -> Option<(String, Expr)> {
    let e = b
        .stmts
        .first()
        .and_then(|s| match s {
            Stmt::Expr(e) => Some(e),
            _ => None,
        })
        .or(b.expr.as_deref())?;
    match e {
        Expr::AssignOp {
            op: BinOp::Add,
            target,
            value,
            ..
        } => {
            let name = match target.as_ref() {
                Expr::Path(n, _) => n.clone(),
                _ => return None,
            };
            Some((name, value.as_ref().clone()))
        }
        _ => None,
    }
}

fn is_push(b: &Block) -> Option<Expr> {
    let e = b
        .stmts
        .first()
        .and_then(|s| match s {
            Stmt::Expr(e) => Some(e),
            _ => None,
        })
        .or(b.expr.as_deref())?;
    match e {
        Expr::MethodCall {
            method,
            args,
            ..
        } if method == "push" && args.len() == 1 => Some(args[0].clone()),
        _ => None,
    }
}

/// Push receiver name if the then-branch / push is `name.push(...)`.
pub fn push_receiver(b: &Block) -> Option<String> {
    let e = b
        .stmts
        .first()
        .and_then(|s| match s {
            Stmt::Expr(e) => Some(e),
            _ => None,
        })
        .or(b.expr.as_deref())?;
    match e {
        Expr::MethodCall {
            receiver,
            method,
            ..
        } if method == "push" => match receiver.as_ref() {
            Expr::Path(n, _) => Some(n.clone()),
            _ => None,
        },
        _ => None,
    }
}

// ── High-level semantic shapes ──────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FilterMapBuild {
    pub result: String,
    pub pat: String,
    pub collection: String,
    pub predicate: Expr,
    pub mapped: Expr,
}

/// Recognise building a Vec via filter+map, including continue-style skips and temps.
pub fn analyze_filter_map_build(block: &Block) -> Option<FilterMapBuild> {
    let skip = crate::translate::patterns::skip_setup_lets(block);
    if block.stmts.len().saturating_sub(skip) < 2 {
        return None;
    }
    let result = match &block.stmts[skip] {
        Stmt::Let {
            name,
            is_mut: true,
            value: Some(v),
            ..
        } if is_vec_new(v) => name.clone(),
        _ => return None,
    };
    let (pat, collection, body) = match &block.stmts[skip + 1] {
        Stmt::Expr(Expr::For {
            pat, iter, body, ..
        }) => (pat.clone(), collection_name(iter)?, body),
        _ => return None,
    };
    if !matches!(&block.expr, Some(e) if matches!(e.as_ref(), Expr::Path(n, _) if n == &result)) {
        return None;
    }

    let (locals, exprs, trailing) = peel_loop_lets(body);
    let actions = classify_actions(&locals, &exprs, trailing.as_ref())?;

    // Form 1: if COND { result.push(V) }
    if actions.len() == 1 {
        if let LoopAction::GuardedPush { cond, value } = &actions[0] {
            // Verify push target is `result` — re-check from original body
            if let Some(recv) = guarded_push_receiver(body) {
                if recv != result {
                    return None;
                }
            }
            return Some(FilterMapBuild {
                result,
                pat,
                collection,
                predicate: cond.clone(),
                mapped: value.clone(),
            });
        }
    }

    // Form 2: if SKIP { continue }; result.push(V)
    if actions.len() == 2 {
        if let (LoopAction::SkipIf(skip_cond), LoopAction::Push(value)) =
            (&actions[0], &actions[1])
        {
            // Confirm push was on result — look at original non-let exprs
            if !body_pushes_to(body, &result) {
                return None;
            }
            return Some(FilterMapBuild {
                result,
                pat,
                collection,
                predicate: negate_pred(skip_cond),
                mapped: value.clone(),
            });
        }
    }

    None
}

#[derive(Debug, Clone)]
pub struct FilteredFold {
    pub acc: String,
    pub acc_init: Expr,
    pub pat: String,
    pub collection: String,
    pub predicate: Option<Expr>,
    pub addend: Expr,
}

/// Recognise `acc += f(x)` optionally under a filter / continue-skip.
pub fn analyze_filtered_fold(block: &Block) -> Option<FilteredFold> {
    let skip = crate::translate::patterns::skip_setup_lets(block);
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
    // Only one mut let before the for (multi-accum handled elsewhere)
    let (pat, collection, body) = match &block.stmts[skip + 1] {
        Stmt::Expr(Expr::For {
            pat, iter, body, ..
        }) => (pat.clone(), collection_name(iter)?, body),
        _ => return None,
    };
    if !matches!(&block.expr, Some(e) if matches!(e.as_ref(), Expr::Path(n, _) if n == &acc)) {
        return None;
    }

    let (locals, exprs, trailing) = peel_loop_lets(body);
    let actions = classify_actions(&locals, &exprs, trailing.as_ref())?;

    match actions.as_slice() {
        [LoopAction::GuardedAccAdd {
            cond,
            name,
            value,
        }] if name == &acc => Some(FilteredFold {
            acc,
            acc_init,
            pat,
            collection,
            predicate: Some(cond.clone()),
            addend: value.clone(),
        }),
        [LoopAction::AccAdd { name, value }] if name == &acc => Some(FilteredFold {
            acc,
            acc_init,
            pat,
            collection,
            predicate: None,
            addend: value.clone(),
        }),
        [LoopAction::SkipIf(skip_cond), LoopAction::AccAdd { name, value }]
            if name == &acc =>
        {
            Some(FilteredFold {
                acc,
                acc_init,
                pat,
                collection,
                predicate: Some(negate_pred(skip_cond)),
                addend: value.clone(),
            })
        }
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct EarlyFind {
    pub pat: String,
    pub collection: String,
    pub predicate: Expr,
    pub value: Expr,
}

/// `for x in xs { if p { return Some(v); } } None` → find
pub fn analyze_early_find(block: &Block) -> Option<EarlyFind> {
    let skip = crate::translate::patterns::skip_setup_lets(block);
    // May start with mut index (indexed search) — that is handled separately.
    // Here: for is first meaningful stmt (or after setup lets only).
    let for_idx = skip;
    // Trailing None
    match &block.expr {
        Some(e) if matches!(e.as_ref(), Expr::Path(n, _) if n == "None" || n.ends_with("::None")) => {
        }
        _ => return None,
    }

    // Reject if there's a mut counter before the for (indexed search)
    if matches!(
        block.stmts.get(for_idx),
        Some(Stmt::Let { is_mut: true, .. })
    ) {
        return None;
    }

    let (pat, collection, body) = match block.stmts.get(for_idx) {
        Some(Stmt::Expr(Expr::For {
            pat, iter, body, ..
        })) => (pat.clone(), collection_name(iter)?, body),
        _ => return None,
    };

    let (locals, exprs, trailing) = peel_loop_lets(body);
    let actions = classify_actions(&locals, &exprs, trailing.as_ref())?;
    if actions.len() != 1 {
        return None;
    }
    match &actions[0] {
        LoopAction::EarlySome {
            cond: Some(cond),
            value,
        } => Some(EarlyFind {
            pat,
            collection,
            predicate: cond.clone(),
            value: value.clone(),
        }),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct FilteredMultiAccum {
    pub inits: Vec<(String, Expr)>,
    pub pat: String,
    pub collection: String,
    pub predicate: Option<Expr>,
    /// Updates applied when the element is kept: (acc_name, op, value)
    pub updates: Vec<(String, BinOp, Expr)>,
    /// Trailing expression after the loop (may reference acc names).
    pub trailing: Expr,
}

/// Multi-accumulator fold with optional continue-filter and a non-tuple trailing expr.
pub fn analyze_filtered_multi_accum(block: &Block) -> Option<FilteredMultiAccum> {
    let mut inits = Vec::new();
    let mut i = crate::translate::patterns::skip_setup_lets(block);
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
    let (pat, collection, body) = match block.stmts.get(i) {
        Some(Stmt::Expr(Expr::For {
            pat, iter, body, ..
        })) => (pat.clone(), collection_name(iter)?, body),
        _ => return None,
    };
    // Must have a trailing expression that is NOT just the tuple of acc names
    // (plain multi-accum already covers that).
    let trailing = block.expr.as_deref()?.clone();
    if is_exact_acc_tuple(&trailing, &inits) {
        return None;
    }

    let (locals, exprs, trail_body) = peel_loop_lets(body);
    let actions = classify_actions(&locals, &exprs, trail_body.as_ref())?;
    let acc_names: Vec<&str> = inits.iter().map(|(n, _)| n.as_str()).collect();

    let mut predicate = None;
    let mut updates = Vec::new();
    let mut idx = 0;
    if let Some(LoopAction::SkipIf(c)) = actions.first() {
        predicate = Some(negate_pred(c));
        idx = 1;
    }
    for action in &actions[idx..] {
        match action {
            LoopAction::AccAdd { name, value } if acc_names.contains(&name.as_str()) => {
                updates.push((name.clone(), BinOp::Add, value.clone()));
            }
            LoopAction::GuardedAccAdd { .. } => return None, // mix of forms — decline
            LoopAction::SkipIf(_) => return None,
            _ => return None,
        }
    }
    if updates.is_empty() {
        return None;
    }
    Some(FilteredMultiAccum {
        inits,
        pat,
        collection,
        predicate,
        updates,
        trailing,
    })
}

fn is_exact_acc_tuple(expr: &Expr, inits: &[(String, Expr)]) -> bool {
    match expr {
        Expr::Tuple(es, _) if es.len() == inits.len() => es.iter().zip(inits.iter()).all(
            |(e, (n, _))| matches!(e, Expr::Path(p, _) if p == n),
        ),
        _ => false,
    }
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

fn collection_name(iter: &Expr) -> Option<String> {
    match strip_ref_noise_expr(iter) {
        Expr::Path(n, _) => Some(n),
        Expr::MethodCall {
            receiver,
            method,
            ..
        } if matches!(method.as_str(), "iter" | "into_iter" | "iter_mut") => {
            collection_name(receiver.as_ref())
        }
        _ => None,
    }
}

fn guarded_push_receiver(body: &Block) -> Option<String> {
    // Find if/push in body (after lets)
    let (_locals, exprs, trailing) = peel_loop_lets(body);
    let e = exprs.first().or(trailing.as_ref())?;
    match e {
        Expr::If {
            then_branch,
            else_branch: None,
            ..
        } => push_receiver(then_branch),
        _ => None,
    }
}

fn body_pushes_to(body: &Block, result: &str) -> bool {
    let (_locals, exprs, trailing) = peel_loop_lets(body);
    let mut all: Vec<&Expr> = exprs.iter().collect();
    if let Some(t) = trailing.as_ref() {
        all.push(t);
    }
    for e in all {
        if let Expr::MethodCall {
            receiver,
            method,
            ..
        } = e
        {
            if method == "push" {
                if matches!(receiver.as_ref(), Expr::Path(n, _) if n == result) {
                    return true;
                }
            }
        }
    }
    false
}

/// Negate a predicate expression for continue→filter conversion.
pub fn negate_pred(cond: &Expr) -> Expr {
    // Prefer algebraic negation of comparisons
    match cond {
        Expr::Binary {
            op,
            left,
            right,
            span,
        } => {
            let flipped = match op {
                BinOp::Eq => Some(BinOp::Ne),
                BinOp::Ne => Some(BinOp::Eq),
                BinOp::Lt => Some(BinOp::Ge),
                BinOp::Le => Some(BinOp::Gt),
                BinOp::Gt => Some(BinOp::Le),
                BinOp::Ge => Some(BinOp::Lt),
                _ => None,
            };
            if let Some(op) = flipped {
                return Expr::Binary {
                    op,
                    left: left.clone(),
                    right: right.clone(),
                    span: *span,
                };
            }
        }
        Expr::Unary {
            op: UnOp::Not,
            expr,
            ..
        } => {
            return strip_ref_noise_expr(expr);
        }
        _ => {}
    }
    Expr::Unary {
        op: UnOp::Not,
        expr: Box::new(cond.clone()),
        span: Span::default(),
    }
}

pub fn subst_path(expr: &Expr, name: &str, replacement: &Expr) -> Expr {
    match expr {
        Expr::Path(n, _) if n == name => replacement.clone(),
        Expr::Path(n, s) => Expr::Path(n.clone(), *s),
        Expr::Lit(l, s) => Expr::Lit(l.clone(), *s),
        Expr::Binary {
            op,
            left,
            right,
            span,
        } => Expr::Binary {
            op: *op,
            left: Box::new(subst_path(left, name, replacement)),
            right: Box::new(subst_path(right, name, replacement)),
            span: *span,
        },
        Expr::Unary { op, expr, span } => Expr::Unary {
            op: *op,
            expr: Box::new(subst_path(expr, name, replacement)),
            span: *span,
        },
        Expr::Call { func, args, span } => Expr::Call {
            func: Box::new(subst_path(func, name, replacement)),
            args: args
                .iter()
                .map(|a| subst_path(a, name, replacement))
                .collect(),
            span: *span,
        },
        Expr::MethodCall {
            receiver,
            method,
            args,
            span,
        } => Expr::MethodCall {
            receiver: Box::new(subst_path(receiver, name, replacement)),
            method: method.clone(),
            args: args
                .iter()
                .map(|a| subst_path(a, name, replacement))
                .collect(),
            span: *span,
        },
        Expr::Field { base, field, span } => Expr::Field {
            base: Box::new(subst_path(base, name, replacement)),
            field: field.clone(),
            span: *span,
        },
        Expr::Index { base, index, span } => Expr::Index {
            base: Box::new(subst_path(base, name, replacement)),
            index: Box::new(subst_path(index, name, replacement)),
            span: *span,
        },
        Expr::Deref { expr, span } => Expr::Deref {
            expr: Box::new(subst_path(expr, name, replacement)),
            span: *span,
        },
        Expr::Reference { is_mut, expr, span } => Expr::Reference {
            is_mut: *is_mut,
            expr: Box::new(subst_path(expr, name, replacement)),
            span: *span,
        },
        Expr::Tuple(es, s) => Expr::Tuple(
            es.iter()
                .map(|e| subst_path(e, name, replacement))
                .collect(),
            *s,
        ),
        Expr::If {
            cond,
            then_branch,
            else_branch,
            span,
        } => Expr::If {
            cond: Box::new(subst_path(cond, name, replacement)),
            then_branch: subst_block(then_branch, name, replacement),
            else_branch: else_branch
                .as_ref()
                .map(|e| Box::new(subst_path(e, name, replacement))),
            span: *span,
        },
        other => other.clone(),
    }
}

fn subst_block(block: &Block, name: &str, replacement: &Expr) -> Block {
    Block {
        stmts: block
            .stmts
            .iter()
            .map(|s| match s {
                Stmt::Expr(e) => Stmt::Expr(subst_path(e, name, replacement)),
                Stmt::Let {
                    name: n,
                    is_mut,
                    ty,
                    value,
                    span,
                } => Stmt::Let {
                    name: n.clone(),
                    is_mut: *is_mut,
                    ty: ty.clone(),
                    value: value.as_ref().map(|v| subst_path(v, name, replacement)),
                    span: *span,
                },
                other => other.clone(),
            })
            .collect(),
        expr: block
            .expr
            .as_ref()
            .map(|e| Box::new(subst_path(e, name, replacement))),
        span: block.span,
    }
}

pub fn strip_ref_noise_expr(expr: &Expr) -> Expr {
    match expr {
        Expr::Deref { expr, .. } | Expr::Reference { expr, .. } => strip_ref_noise_expr(expr),
        Expr::Binary {
            op,
            left,
            right,
            span,
        } => Expr::Binary {
            op: *op,
            left: Box::new(strip_ref_noise_expr(left)),
            right: Box::new(strip_ref_noise_expr(right)),
            span: *span,
        },
        Expr::Unary { op, expr, span } => Expr::Unary {
            op: *op,
            expr: Box::new(strip_ref_noise_expr(expr)),
            span: *span,
        },
        Expr::Call { func, args, span } => Expr::Call {
            func: Box::new(strip_ref_noise_expr(func)),
            args: args.iter().map(strip_ref_noise_expr).collect(),
            span: *span,
        },
        other => other.clone(),
    }
}
