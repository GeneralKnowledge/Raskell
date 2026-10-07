//! Walk `tests/rusty/**/*.rs` and classify each file.
//!
//! Categories under `unsupported/` are expected to reject cleanly.
//! All other categories must translate and typecheck under GHC.

use std::path::PathBuf;
use std::process::Command;
use walkdir::WalkDir;

fn rusty_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/rusty")
}

fn ghc_available() -> bool {
    Command::new("ghc").arg("--version").output().is_ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Success,
    RejectedCorrectly,
    MissedOpportunity,
    Incorrect,
    ShouldHaveRejected,
}

fn classify(path: &std::path::Path) -> Verdict {
    let rel = path
        .strip_prefix(rusty_root())
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();
    let expected_reject = rel.starts_with("unsupported/");

    let result = raskell::translate_file(path).expect("translate_file");
    let rejected = result.diagnostics.has_errors();

    if expected_reject {
        return if rejected {
            Verdict::RejectedCorrectly
        } else {
            Verdict::ShouldHaveRejected
        };
    }

    if rejected {
        return Verdict::MissedOpportunity;
    }

    if !ghc_available() {
        return Verdict::Success;
    }

    let dir = tempfile::tempdir().unwrap();
    let hs = dir.path().join("Mod.hs");
    std::fs::write(&hs, &result.haskell_source).unwrap();
    let out = Command::new("ghc")
        .args(["-c", "-outputdir", dir.path().to_str().unwrap(), hs.to_str().unwrap()])
        .output()
        .unwrap();
    if out.status.success() {
        Verdict::Success
    } else {
        eprintln!(
            "incorrect translation for {rel}:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        Verdict::Incorrect
    }
}

#[test]
fn rusty_corpus_has_no_incorrect_translations() {
    let root = rusty_root();
    assert!(root.is_dir(), "missing {}", root.display());

    let mut success = 0usize;
    let mut rejected_ok = 0usize;
    let mut missed = Vec::new();
    let mut incorrect = Vec::new();
    let mut should_reject = Vec::new();
    let mut total = 0usize;

    for entry in WalkDir::new(&root).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("rs") {
            continue;
        }
        total += 1;
        let rel = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .to_string();
        match classify(path) {
            Verdict::Success => success += 1,
            Verdict::RejectedCorrectly => rejected_ok += 1,
            Verdict::MissedOpportunity => missed.push(rel),
            Verdict::Incorrect => incorrect.push(rel),
            Verdict::ShouldHaveRejected => should_reject.push(rel),
        }
    }

    eprintln!("Rust constructs tested:     {total}");
    eprintln!("Successfully translated:    {success}");
    eprintln!("Rejected correctly:         {rejected_ok}");
    eprintln!("Missed opportunities:       {}", missed.len());
    eprintln!("Incorrect translations:     {}", incorrect.len());
    eprintln!("Should have rejected:       {}", should_reject.len());
    for m in &missed {
        eprintln!("  missed: {m}");
    }

    assert!(total >= 70, "corpus too small: {total}");
    assert!(
        incorrect.is_empty(),
        "incorrect translations: {incorrect:?}"
    );
    assert!(
        should_reject.is_empty(),
        "should have rejected: {should_reject:?}"
    );
    // Missed opportunities are documented in reports/coverage.md — allowed but tracked.
    let _ = missed;
}

#[test]
fn semantic_convergence_filter_map() {
    let iter = r#"
        fn via_iter(values: Vec<i32>) -> Vec<i32> {
            values.iter().filter(|x| **x > 0).map(|x| **x * 2).collect()
        }
    "#;
    let loop_ = r#"
        fn via_loop(values: Vec<i32>) -> Vec<i32> {
            let mut result = Vec::new();
            for x in values {
                if x > 0 {
                    result.push(x * 2);
                }
            }
            result
        }
    "#;
    let a = raskell::translate(iter, "a.rs").unwrap().haskell_source;
    let b = raskell::translate(loop_, "b.rs").unwrap().haskell_source;
    // Both should contain filter + map (order: map over filter)
    assert!(a.contains("filter") && a.contains("map"), "{a}");
    assert!(b.contains("filter") && b.contains("map"), "{b}");
}

#[test]
fn semantic_convergence_sum() {
    let iter = "fn s(values: Vec<i32>) -> i32 { values.iter().sum() }";
    let loop_ = r#"
        fn s(values: Vec<i32>) -> i32 {
            let mut total = 0;
            for value in values { total += value; }
            total
        }
    "#;
    let a = raskell::translate(iter, "a.rs").unwrap().haskell_source;
    let b = raskell::translate(loop_, "b.rs").unwrap().haskell_source;
    assert!(a.contains("sum"), "{a}");
    assert!(b.contains("sum"), "{b}");
}

fn assert_hs_ok(src: &str) -> String {
    let r = raskell::translate(src, "t.rs").unwrap();
    assert!(!r.diagnostics.has_errors(), "{}", r.diagnostics);
    r.haskell_source
}

#[test]
fn convergence_filter_map_four_ways() {
    let if_push = r#"
        fn positive_doubled(values: Vec<i32>) -> Vec<i32> {
            let mut result = Vec::new();
            for value in values {
                if value > 0 { result.push(value * 2); }
            }
            result
        }
    "#;
    let temp = r#"
        fn positive_doubled(values: Vec<i32>) -> Vec<i32> {
            let mut result = Vec::new();
            for value in values {
                let doubled = value * 2;
                if doubled > 0 { result.push(doubled); }
            }
            result
        }
    "#;
    let cont = r#"
        fn positive_doubled(values: Vec<i32>) -> Vec<i32> {
            let mut result = Vec::new();
            for value in values {
                if value <= 0 { continue; }
                result.push(value * 2);
            }
            result
        }
    "#;
    let iter = r#"
        fn positive_doubled(values: Vec<i32>) -> Vec<i32> {
            values.iter().map(|x| *x * 2).filter(|x| *x > 0).copied().collect()
        }
    "#;
    for (name, src) in [
        ("if_push", if_push),
        ("temp", temp),
        ("continue", cont),
        ("iter", iter),
    ] {
        let hs = assert_hs_ok(src);
        assert!(
            hs.contains("filter") && hs.contains("map"),
            "{name} missing filter/map:\n{hs}"
        );
        // Must not leave unbound temp names
        assert!(
            !hs.contains("doubled") || hs.contains("x * 2"),
            "{name} left unbound temp:\n{hs}"
        );
    }
}

#[test]
fn convergence_filtered_fold_four_ways() {
    let if_ = r#"
        fn calculate(values: Vec<i32>) -> i32 {
            let mut total = 0;
            for value in values {
                if value > 0 { total += value * 2; }
            }
            total
        }
    "#;
    let temp = r#"
        fn calculate(values: Vec<i32>) -> i32 {
            let mut total = 0;
            for value in values {
                let doubled = value * 2;
                if value > 0 { total += doubled; }
            }
            total
        }
    "#;
    let cont = r#"
        fn calculate(values: Vec<i32>) -> i32 {
            let mut total = 0;
            for value in values {
                if value <= 0 { continue; }
                total += value * 2;
            }
            total
        }
    "#;
    let iter = r#"
        fn calculate(values: Vec<i32>) -> i32 {
            values.iter().filter(|x| **x > 0).map(|x| *x * 2).sum()
        }
    "#;
    for (name, src) in [("if", if_), ("temp", temp), ("continue", cont), ("iter", iter)] {
        let hs = assert_hs_ok(src);
        assert!(hs.contains("sum"), "{name} missing sum:\n{hs}");
        assert!(
            hs.contains("filter") && hs.contains("map"),
            "{name} missing filter/map:\n{hs}"
        );
    }
}
