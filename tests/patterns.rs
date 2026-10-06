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
    eprintln!("stmts={:?} expr={:?}", f.body.stmts, f.body.expr);
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
