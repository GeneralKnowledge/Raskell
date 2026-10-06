//! Invalid-program rejection tests.

use raskell::translate;

fn assert_rejects(src: &str, needle: &str) {
    let result = translate(src, "bad.rs").expect("translate");
    assert!(
        result.diagnostics.has_errors(),
        "expected errors for: {src}"
    );
    let text = format!("{}", result.diagnostics);
    assert!(
        text.contains(needle) || text.contains("E0421") || text.contains("unsupported"),
        "diagnostic should mention '{needle}':\n{text}"
    );
}

#[test]
fn rejects_unsafe_block() {
    assert_rejects("fn f() { unsafe { } }", "unsafe");
}

#[test]
fn rejects_raw_pointers_via_extern() {
    assert_rejects(
        r#"extern "C" { fn foo(p: *mut i32); }"#,
        "FFI",
    );
}

#[test]
fn rejects_traits() {
    assert_rejects("trait Foo { fn bar(&self); }", "trait");
}

#[test]
fn rejects_async() {
    assert_rejects("async fn f() {}", "async");
}

#[test]
fn rejects_unknown_macro() {
    assert_rejects("fn f() { mystery!(); }", "macro");
}
