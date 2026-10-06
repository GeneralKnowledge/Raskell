//! Semantic / diagnostic tests.

use raskell::{check, translate};

#[test]
fn rejects_unsafe() {
    let src = r#"
        fn bad(x: i32) -> i32 {
            unsafe { x }
        }
    "#;
    let diags = check(src, "bad.rs").expect("check");
    assert!(diags.has_errors());
    let text = format!("{diags}");
    assert!(text.contains("unsafe") || text.contains("E0421"));
}

#[test]
fn rejects_unknown_macro() {
    let src = r#"
        fn bad() {
            fancy_macro!();
        }
    "#;
    let result = translate(src, "bad.rs").expect("translate");
    assert!(result.diagnostics.has_errors());
}

#[test]
fn accepts_sum_positive() {
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
    let diags = check(src, "ok.rs").expect("check");
    assert!(!diags.has_errors(), "{diags}");
}

#[test]
fn rejects_ffi() {
    let src = r#"
        extern "C" {
            fn puts(s: *const i8) -> i32;
        }
    "#;
    let result = translate(src, "ffi.rs").expect("translate");
    assert!(result.diagnostics.has_errors());
}
