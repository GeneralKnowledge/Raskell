//! GHC compilation tests for generated Haskell.

use std::process::Command;
use tempfile::tempdir;

fn ghc_available() -> bool {
    Command::new("ghc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn compile_hs(source: &str) {
    if !ghc_available() {
        eprintln!("skipping: ghc not available");
        return;
    }
    let result = raskell::translate(source, "Example.rs").expect("translate");
    assert!(
        !result.diagnostics.has_errors(),
        "{}",
        result.diagnostics
    );
    let dir = tempdir().unwrap();
    let path = dir.path().join("Example.hs");
    std::fs::write(&path, &result.haskell_source).unwrap();
    let output = Command::new("ghc")
        .args(["-c", path.to_str().unwrap()])
        .output()
        .expect("run ghc");
    assert!(
        output.status.success(),
        "GHC failed:\n{}\n\nSource:\n{}",
        String::from_utf8_lossy(&output.stderr),
        result.haskell_source
    );
}

#[test]
fn compiles_arithmetic() {
    compile_hs("fn add(a: i32, b: i32) -> i32 { a + b }");
}

#[test]
fn compiles_fibonacci() {
    compile_hs(
        r#"
        fn fibonacci(n: i32) -> i32 {
            if n <= 1 { n } else { fibonacci(n - 1) + fibonacci(n - 2) }
        }
        "#,
    );
}

#[test]
fn compiles_sum_positive() {
    compile_hs(
        r#"
        fn sum_positive(values: Vec<i32>) -> i32 {
            let mut total = 0;
            for value in values {
                if value > 0 { total += value; }
            }
            total
        }
        "#,
    );
}

#[test]
fn compiles_iterators() {
    compile_hs(
        r#"
        fn double_all(values: Vec<i32>) -> Vec<i32> {
            values.iter().map(|x| x * 2).collect()
        }
        "#,
    );
}

#[test]
fn compiles_option() {
    compile_hs(
        r#"
        fn f(x: Option<i32>) -> Option<i32> {
            match x { Some(v) => Some(v * 2), None => None }
        }
        "#,
    );
}

#[test]
fn compiles_enum() {
    compile_hs(
        r#"
        enum Shape { Circle(f64), Rectangle(f64, f64) }
        fn area(s: Shape) -> f64 {
            match s {
                Shape::Circle(r) => r * r,
                Shape::Rectangle(w, h) => w * h,
            }
        }
        "#,
    );
}
