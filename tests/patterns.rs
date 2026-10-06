//! Pattern recognition unit tests.

use raskell::ast::*;
use raskell::parser;
use raskell::translate::patterns;

#[test]
fn detects_sum_positive() {
    let src = r#"
fn sum_positive(values: Vec<i32>) -> i32 {
    let mut total = 0;
    for value in values {
        if value > 0 {
            total += value;
        }
    }
    total
}
"#;
    let prog = parser::parse(src, "t.rs").expect("parse");
    let Item::Function(f) = &prog.items[0] else {
        panic!("expected function");
    };
    assert!(
        patterns::detect_conditional_sum(&f.body).is_some(),
        "expected conditional sum pattern"
    );
}

#[test]
fn detects_scalar_mutation() {
    let src = r#"
fn scale(x: i32) -> i32 {
    let mut y = x;
    y += 5;
    y *= 2;
    y
}
"#;
    let prog = parser::parse(src, "t.rs").expect("parse");
    let Item::Function(f) = &prog.items[0] else {
        panic!("expected function");
    };
    assert!(patterns::detect_scalar_mutation(&f.body).is_some());
}

#[test]
fn detects_map_push() {
    let src = r#"
fn double_push(values: Vec<i32>) -> Vec<i32> {
    let mut result = Vec::new();
    for value in values {
        result.push(value * 2);
    }
    result
}
"#;
    let prog = parser::parse(src, "t.rs").expect("parse");
    let Item::Function(f) = &prog.items[0] else {
        panic!("expected function");
    };
    assert!(patterns::detect_map_push(&f.body).is_some());
}

#[test]
fn detects_filter_map_push() {
    let src = r#"
fn positives_doubled(values: Vec<i32>) -> Vec<i32> {
    let mut result = Vec::new();
    for x in values {
        if x > 0 {
            result.push(x * 2);
        }
    }
    result
}
"#;
    let prog = parser::parse(src, "t.rs").expect("parse");
    let Item::Function(f) = &prog.items[0] else {
        panic!("expected function");
    };
    assert!(patterns::detect_filter_map_push(&f.body).is_some());
}

#[test]
fn detects_continue_break_fold() {
    let src = r#"
fn calculate(values: Vec<i32>) -> i32 {
    let mut total = 0;
    for value in values {
        if value < 0 {
            continue;
        }
        total += value;
        if total > 100 {
            break;
        }
    }
    total
}
"#;
    let (prog, diags) = parser::parse_with_diagnostics(src, "t.rs").unwrap();
    assert!(!diags.has_errors(), "{diags}");
    let Item::Function(func) = &prog.items[0] else {
        panic!("expected function");
    };
    let Stmt::Expr(Expr::For { body, .. }) = &func.body.stmts[1] else {
        panic!("expected for");
    };
    for (i, s) in body.stmts.iter().enumerate() {
        eprintln!("STMT {i}: {s:?}");
    }
    eprintln!("TRAILING: {:?}", body.expr);
    let fold = patterns::detect_continue_break_fold(&func.body)
        .expect("expected continue/break fold");
    assert!(fold.skip_cond.is_some(), "skip");
    assert!(fold.break_cond.is_some(), "break");
}

#[test]
fn detects_record_update() {
    let src = r#"
struct User { name: String, age: i32 }
fn rename(mut user: User, name: String) -> User {
    user.name = name;
    user
}
"#;
    let prog = parser::parse(src, "t.rs").expect("parse");
    let f = prog
        .items
        .iter()
        .find_map(|i| match i {
            Item::Function(f) => Some(f),
            _ => None,
        })
        .expect("fn");
    assert!(patterns::detect_record_updates(&f.body).is_some());
}

#[test]
fn detects_indexed_search() {
    let src = r#"
fn index_of(values: Vec<i32>, target: i32) -> Option<i32> {
    let mut i = 0;
    for v in values {
        if v == target { return Some(i); }
        i += 1;
    }
    None
}
"#;
    let prog = parser::parse(src, "t.rs").expect("parse");
    let Item::Function(f) = &prog.items[0] else {
        panic!("expected function");
    };
    assert!(patterns::detect_indexed_search(&f.body).is_some());
}

#[test]
fn detects_adjacent_order_scan() {
    let src = r#"
fn is_sorted(values: Vec<i32>) -> bool {
    let mut prev = values[0];
    for v in values {
        if v < prev { return false; }
        prev = v;
    }
    true
}
"#;
    let prog = parser::parse(src, "t.rs").expect("parse");
    let Item::Function(f) = &prog.items[0] else {
        panic!("expected function");
    };
    assert!(patterns::detect_adjacent_order_scan(&f.body).is_some());
}

#[test]
fn detects_euclid_while() {
    let src = r#"
fn gcd(mut a: i32, mut b: i32) -> i32 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}
"#;
    let prog = parser::parse(src, "t.rs").expect("parse");
    let Item::Function(f) = &prog.items[0] else {
        panic!("expected function");
    };
    assert!(patterns::detect_euclid_while(&f.body).is_some());
}

#[test]
fn detects_while_accum_either_order() {
    let src = r#"
fn count_up(n: i32) -> i32 {
    let mut c = 0;
    let mut i = 0;
    while i < n {
        c += 1;
        i += 1;
    }
    c
}
"#;
    let prog = parser::parse(src, "t.rs").expect("parse");
    let Item::Function(f) = &prog.items[0] else {
        panic!("expected function");
    };
    assert!(patterns::detect_while_accum(&f.body).is_some());
}
