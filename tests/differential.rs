//! Differential tests: Rust reference behaviour vs generated Haskell.

use std::process::Command;
use tempfile::tempdir;

fn tools_available() -> bool {
    let rustc = Command::new("rustc").arg("--version").output().is_ok();
    let ghc = Command::new("ghc").arg("--version").output().is_ok();
    rustc && ghc
}

/// Compile and run a tiny Rust program that prints a value.
fn run_rust(body: &str) -> String {
    let dir = tempdir().unwrap();
    let rs = dir.path().join("prog.rs");
    let bin = dir.path().join("prog_rs");
    let src = format!(
        r#"
        #![allow(unused)]
        {body}
        fn main() {{
            print!("{{}}", run());
        }}
        "#
    );
    std::fs::write(&rs, src).unwrap();
    let output = Command::new("rustc")
        .args([rs.to_str().unwrap(), "-o", bin.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "rustc failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let out = Command::new(&bin).output().unwrap();
    assert!(out.status.success(), "rust binary failed");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Translate functions, then compile as module Main and print `run`.
fn run_haskell(rust_fns: &str) -> String {
    let result = raskell::translate(rust_fns, "Main.rs").expect("translate");
    assert!(
        !result.diagnostics.has_errors(),
        "{}",
        result.diagnostics
    );

    let dir = tempdir().unwrap();
    let hs = dir.path().join("Main.hs");
    let bin = dir.path().join("prog_hs");

    // Ensure module Main and a printable main
    let mut module = result.haskell_source.clone();
    // Force module name Main
    if let Some(rest) = module.strip_prefix("module ") {
        if let Some(idx) = rest.find(" where") {
            module = format!("module Main where{}", &rest[idx + 6..]);
        }
    }
    // Replace or append main that prints run
    if module.contains("\nmain ") || module.contains("\nmain=") || module.contains("main ::") {
        // Strip existing main bindings (simple approach: append override via new main is hard)
        // Instead inject print at the end by renaming generated main if any.
        module = strip_main_binds(&module);
    }
    module.push_str("\nmain :: IO ()\nmain = print run\n");

    std::fs::write(&hs, &module).unwrap();

    let output = Command::new("ghc")
        .args(["-O0", "-o", bin.to_str().unwrap(), hs.to_str().unwrap()])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ghc failed:\n{}\n\n{}",
        String::from_utf8_lossy(&output.stderr),
        module
    );
    let out = Command::new(&bin).output().unwrap();
    assert!(
        out.status.success(),
        "haskell binary failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn strip_main_binds(src: &str) -> String {
    let mut out = String::new();
    let mut skipping = false;
    for line in src.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("main ::") || trimmed.starts_with("main =") {
            skipping = true;
            continue;
        }
        if skipping {
            // continue skipping until a blank line or a new top-level bind (no leading space)
            if line.is_empty() {
                skipping = false;
                continue;
            }
            if !line.starts_with(' ') && !line.starts_with('\t') && !trimmed.is_empty() {
                skipping = false;
                // fall through to push this line
            } else {
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn assert_same(rust_body: &str, haskell_fns: &str) {
    if !tools_available() {
        eprintln!("skipping differential test: rustc/ghc missing");
        return;
    }
    let r = run_rust(rust_body);
    let h = run_haskell(haskell_fns);
    assert_eq!(r, h, "Rust={r} Haskell={h}");
}

#[test]
fn diff_sum_positive() {
    assert_same(
        r#"
        fn sum_positive(values: Vec<i32>) -> i32 {
            let mut total = 0;
            for value in values {
                if value > 0 { total += value; }
            }
            total
        }
        fn run() -> i32 { sum_positive(vec![1, -2, 3, -4, 5]) }
        "#,
        r#"
        fn sum_positive(values: Vec<i32>) -> i32 {
            let mut total = 0;
            for value in values {
                if value > 0 { total += value; }
            }
            total
        }
        fn run() -> i32 { sum_positive(vec![1, -2, 3, -4, 5]) }
        "#,
    );
}

#[test]
fn diff_fibonacci() {
    assert_same(
        r#"
        fn fibonacci(n: i32) -> i32 {
            if n <= 1 { n } else { fibonacci(n - 1) + fibonacci(n - 2) }
        }
        fn run() -> i32 { fibonacci(10) }
        "#,
        r#"
        fn fibonacci(n: i32) -> i32 {
            if n <= 1 { n } else { fibonacci(n - 1) + fibonacci(n - 2) }
        }
        fn run() -> i32 { fibonacci(10) }
        "#,
    );
}

#[test]
fn diff_scale_mutation() {
    assert_same(
        r#"
        fn scale(x: i32) -> i32 {
            let mut y = x;
            y += 5;
            y *= 2;
            y
        }
        fn run() -> i32 { scale(10) }
        "#,
        r#"
        fn scale(x: i32) -> i32 {
            let mut y = x;
            y += 5;
            y *= 2;
            y
        }
        fn run() -> i32 { scale(10) }
        "#,
    );
}

#[test]
fn diff_map_double() {
    assert_same(
        r#"
        fn double_all(values: Vec<i32>) -> Vec<i32> {
            values.iter().map(|x| *x * 2).collect()
        }
        fn run() -> i32 {
            double_all(vec![1, 2, 3]).iter().sum()
        }
        "#,
        r#"
        fn double_all(values: Vec<i32>) -> Vec<i32> {
            values.iter().map(|x| *x * 2).collect()
        }
        fn run() -> i32 {
            double_all(vec![1, 2, 3]).iter().sum()
        }
        "#,
    );
}

#[test]
fn diff_factorial() {
    assert_same(
        r#"
        fn factorial(n: i32) -> i32 {
            if n <= 1 { 1 } else { n * factorial(n - 1) }
        }
        fn run() -> i32 { factorial(5) }
        "#,
        r#"
        fn factorial(n: i32) -> i32 {
            if n <= 1 { 1 } else { n * factorial(n - 1) }
        }
        fn run() -> i32 { factorial(5) }
        "#,
    );
}

#[test]
fn diff_empty_and_negatives() {
    assert_same(
        r#"
        fn sum_positive(values: Vec<i32>) -> i32 {
            let mut total = 0;
            for value in values {
                if value > 0 { total += value; }
            }
            total
        }
        fn run() -> i32 { sum_positive(vec![]) + sum_positive(vec![-1, -2]) }
        "#,
        r#"
        fn sum_positive(values: Vec<i32>) -> i32 {
            let mut total = 0;
            for value in values {
                if value > 0 { total += value; }
            }
            total
        }
        fn run() -> i32 { sum_positive(vec![]) + sum_positive(vec![-1, -2]) }
        "#,
    );
}
