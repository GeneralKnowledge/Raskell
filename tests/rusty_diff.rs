//! Differential tests drawn from the rusty stress corpus.

use std::process::Command;
use tempfile::tempdir;

fn tools_available() -> bool {
    Command::new("rustc").arg("--version").output().is_ok()
        && Command::new("ghc").arg("--version").output().is_ok()
}

fn run_rust(body: &str) -> String {
    let dir = tempdir().unwrap();
    let rs = dir.path().join("prog.rs");
    let bin = dir.path().join("prog_rs");
    let src = format!(
        r#"
        #![allow(unused)]
        {body}
        fn main() {{ print!("{{:?}}", run()); }}
        "#
    );
    std::fs::write(&rs, src).unwrap();
    let output = Command::new("rustc")
        .args([rs.to_str().unwrap(), "-o", bin.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "rustc: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let out = Command::new(&bin).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

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
    let mut module = result.haskell_source;
    if let Some(rest) = module.strip_prefix("module ") {
        if let Some(idx) = rest.find(" where") {
            module = format!("module Main where{}", &rest[idx + 6..]);
        }
    }
    module = strip_main(&module);
    module.push_str("\nmain :: IO ()\nmain = print run\n");
    std::fs::write(&hs, &module).unwrap();
    let output = Command::new("ghc")
        .args(["-O0", "-o", bin.to_str().unwrap(), hs.to_str().unwrap()])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ghc: {}\n{module}",
        String::from_utf8_lossy(&output.stderr)
    );
    let out = Command::new(&bin).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn strip_main(src: &str) -> String {
    let mut out = String::new();
    let mut skipping = false;
    for line in src.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("main ::") || trimmed.starts_with("main =") {
            skipping = true;
            continue;
        }
        if skipping {
            if line.is_empty() {
                skipping = false;
                continue;
            }
            if !line.starts_with(' ') && !line.starts_with('\t') && !trimmed.is_empty() {
                skipping = false;
            } else {
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Normalize Rust `Debug` vs Haskell `Show` list formatting (`[1, 2]` vs `[1,2]`).
fn normalize_output(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn assert_same(body: &str) {
    if !tools_available() {
        eprintln!("skipping differential — rustc/ghc missing");
        return;
    }
    let r = run_rust(body);
    let h = run_haskell(body);
    assert_eq!(
        normalize_output(&r),
        normalize_output(&h),
        "Rust={r} Haskell={h}\n{body}"
    );
}

#[test]
fn diff_filter_map_loop() {
    assert_same(
        r#"
        fn run() -> Vec<i32> {
            let values = vec![-2, -1, 0, 1, 2, 3];
            let mut result = Vec::new();
            for x in values {
                if x > 0 { result.push(x * 2); }
            }
            result
        }
        "#,
    );
}

#[test]
fn diff_continue_break_fold() {
    assert_same(
        r#"
        fn run() -> i32 {
            let values = vec![-5, 10, 20, 30, 40, 50];
            let mut total = 0;
            for value in values {
                if value < 0 { continue; }
                total += value;
                if total > 100 { break; }
            }
            total
        }
        "#,
    );
}

#[test]
fn diff_record_rename() {
    assert_same(
        r#"
        struct User { name: String, age: i32 }
        fn run() -> i32 {
            let u = User { name: String::from("a"), age: 1 };
            let u2 = rename(u, String::from("b"));
            u2.name.len() as i32 + u2.age
        }
        fn rename(mut user: User, name: String) -> User {
            user.name = name;
            user
        }
        "#,
    );
}

#[test]
fn diff_early_return() {
    assert_same(
        r#"
        fn first_nonzero(a: i32, b: i32, c: i32) -> i32 {
            if a != 0 { return a; }
            if b != 0 { return b; }
            c
        }
        fn run() -> i32 { first_nonzero(0, 0, 7) + first_nonzero(0, 3, 9) + first_nonzero(1, 2, 3) }
        "#,
    );
}

#[test]
fn diff_max_manual() {
    assert_same(
        r#"
        fn max_of(values: Vec<i32>) -> i32 {
            let mut best = values[0];
            for v in values {
                if v > best { best = v; }
            }
            best
        }
        fn run() -> i32 { max_of(vec![3, 1, 4, 1, 5, 9, 2]) }
        "#,
    );
}

#[test]
fn diff_sum_of_squares() {
    assert_same(
        r#"
        fn sum_of_squares(values: Vec<i32>) -> i32 {
            let mut total = 0;
            for x in values { total += x * x; }
            total
        }
        fn run() -> i32 { sum_of_squares(vec![1, 2, 3, 4]) }
        "#,
    );
}

#[test]
fn diff_euclid_gcd() {
    assert_same(
        r#"
        fn gcd(mut a: i32, mut b: i32) -> i32 {
            while b != 0 {
                let t = b;
                b = a % b;
                a = t;
            }
            a
        }
        fn run() -> i32 { gcd(48, 18) + gcd(7, 0) + gcd(0, 5) }
        "#,
    );
}

#[test]
fn diff_indexed_search() {
    assert_same(
        r#"
        fn index_of(values: Vec<i32>, target: i32) -> Option<i32> {
            let mut i = 0;
            for v in values {
                if v == target { return Some(i); }
                i += 1;
            }
            None
        }
        fn run() -> i32 {
            (match index_of(vec![10, 20, 30], 20) {
                Some(i) => i,
                None => -1,
            }) + (match index_of(vec![1, 2], 9) {
                Some(i) => i,
                None => -1,
            })
        }
        "#,
    );
}

#[test]
fn diff_is_sorted() {
    assert_same(
        r#"
        fn is_sorted(values: Vec<i32>) -> bool {
            let mut prev = values[0];
            for v in values {
                if v < prev { return false; }
                prev = v;
            }
            true
        }
        fn run() -> i32 {
            (if is_sorted(vec![1, 2, 2, 5]) { 1 } else { 0 })
                + (if is_sorted(vec![3, 1]) { 1 } else { 0 })
                + (if is_sorted(vec![7]) { 1 } else { 0 })
        }
        "#,
    );
}

#[test]
fn diff_count_up_while() {
    assert_same(
        r#"
        fn count_up(n: i32) -> i32 {
            let mut c = 0;
            let mut i = 0;
            while i < n {
                c += 1;
                i += 1;
            }
            c
        }
        fn run() -> i32 { count_up(0) + count_up(1) + count_up(5) }
        "#,
    );
}

#[test]
fn diff_fold_temp_and_continue() {
    assert_same(
        r#"
        fn calculate(values: Vec<i32>) -> i32 {
            let mut total = 0;
            for value in values {
                let doubled = value * 2;
                if value > 0 { total += doubled; }
            }
            total
        }
        fn run() -> i32 { calculate(vec![-2, -1, 0, 1, 2, 3]) }
        "#,
    );
}

#[test]
fn diff_filter_map_continue() {
    assert_same(
        r#"
        fn positive_doubled(values: Vec<i32>) -> Vec<i32> {
            let mut result = Vec::new();
            for value in values {
                if value <= 0 { continue; }
                result.push(value * 2);
            }
            result
        }
        fn run() -> Vec<i32> { positive_doubled(vec![-2, -1, 0, 1, 2, 3]) }
        "#,
    );
}

#[test]
fn diff_first_positive_borrow() {
    assert_same(
        r#"
        fn first_positive(values: &Vec<i32>) -> Option<i32> {
            for value in values {
                if *value > 0 { return Some(*value); }
            }
            None
        }
        fn run() -> i32 {
            (match first_positive(&vec![-2, -1, 4, 5]) { Some(v) => v, None => -1 })
                + (match first_positive(&vec![-3, -2]) { Some(v) => v, None => -1 })
        }
        "#,
    );
}

#[test]
fn diff_ugly_average() {
    assert_same(
        r#"
        fn process(values: Vec<i32>) -> i32 {
            let mut total = 0;
            let mut count = 0;
            for value in values {
                if value < 0 { continue; }
                let adjusted = value + 10;
                total += adjusted;
                count += 1;
            }
            if count == 0 { 0 } else { total / count }
        }
        fn run() -> i32 {
            process(vec![-5, 0, 5, 10]) + process(vec![-1, -2]) + process(vec![2])
        }
        "#,
    );
}

/// Lightweight property-style differential: several generated inputs for one pure fn.
#[test]
fn diff_property_filter_sum() {
    if !tools_available() {
        return;
    }
    let rust_fn = r#"
        fn sum_positive(values: Vec<i32>) -> i32 {
            let mut total = 0;
            for value in values {
                if value > 0 { total += value; }
            }
            total
        }
    "#;
    let cases: &[&[i32]] = &[
        &[],
        &[0],
        &[1],
        &[-1, -2],
        &[1, 2, 3],
        &[-5, 0, 5, 10],
        &[i32::MIN / 2, -1, 0, 1, i32::MAX / 2],
    ];
    for (i, values) in cases.iter().enumerate() {
        let lit = format!("{:?}", values);
        let body = format!(
            "{rust_fn}\nfn run() -> i32 {{ sum_positive(vec!{lit}) }}\n"
        );
        let r = run_rust(&body);
        let h = run_haskell(&body);
        assert_eq!(
            normalize_output(&r),
            normalize_output(&h),
            "case {i}: Rust={r} Haskell={h}\n{body}"
        );
    }
}
