//! Tests for newly supported gaps: generics, traits, `?`, while, iterators.

use raskell::translate;

fn hs(src: &str) -> String {
    let result = translate(src, "t.rs").expect("translate");
    assert!(
        !result.diagnostics.has_errors(),
        "{}",
        result.diagnostics
    );
    result.haskell_source
}

#[test]
fn generics_become_type_variables() {
    let out = hs(
        r#"
        fn identity<T>(x: T) -> T { x }
        "#,
    );
    assert!(out.contains("identity ::"));
    assert!(out.contains("t -> t") || out.contains("a -> a") || out.contains(" identity"));
    assert!(out.contains("identity x = x"));
}

#[test]
fn trait_bounds_become_constraints() {
    let out = hs(
        r#"
        trait Showable { fn show_it(&self) -> String; }
        fn display<T: Showable>(x: T) -> String { x.show_it() }
        "#,
    );
    assert!(out.contains("class Showable"));
    assert!(out.contains("Showable") && out.contains("=>"));
    assert!(out.contains("showIt"));
}

#[test]
fn trait_impl_becomes_instance() {
    let out = hs(
        r#"
        trait Greet { fn greet(&self) -> String; }
        struct Person { name: String }
        impl Greet for Person {
            fn greet(&self) -> String { self.name.clone() }
        }
        "#,
    );
    assert!(out.contains("class Greet"));
    assert!(out.contains("instance Greet Person"));
    assert!(out.contains("greet"));
}

#[test]
fn try_operator_becomes_do() {
    let out = hs(
        r#"
        fn parse(s: String) -> Result<i32, String> {
            if s.is_empty() { Err(String::from("e")) } else { Ok(1) }
        }
        fn add(a: String, b: String) -> Result<i32, String> {
            let x = parse(a)?;
            let y = parse(b)?;
            Ok(x + y)
        }
        "#,
    );
    assert!(out.contains("do"));
    assert!(out.contains("<-"));
    assert!(out.contains("return") || out.contains("pure") || out.contains("Right") || out.contains("x + y"));
}

#[test]
fn while_accum_becomes_go() {
    let out = hs(
        r#"
        fn triangle(n: i32) -> i32 {
            let mut i = n;
            let mut total = 0;
            while i > 0 {
                total += i;
                i -= 1;
            }
            total
        }
        "#,
    );
    assert!(out.contains("go") || out.contains("if"));
    assert!(!out.contains("unsupported"));
}

#[test]
fn chain_iterator() {
    let out = hs(
        r#"
        fn combine(a: Vec<i32>, b: Vec<i32>) -> i32 {
            a.iter().chain(b.iter()).copied().sum()
        }
        "#,
    );
    assert!(out.contains("++") || out.contains("sum"));
}

#[test]
fn string_from_is_erased() {
    let out = hs(
        r#"
        fn label() -> String { String::from("hi") }
        "#,
    );
    assert!(out.contains("\"hi\""));
    assert!(!out.contains("id \"hi\""));
}

#[test]
fn stdlib_methods_map() {
    let out = hs(
        r#"
        fn check(s: String, o: Option<i32>, r: Result<i32, String>) -> bool {
            let a = s.is_empty();
            let b = s.starts_with(String::from("x"));
            let c = o.is_some();
            let d = r.is_ok();
            a && b && c && d
        }
        "#,
    );
    assert!(out.contains("null"));
    assert!(out.contains("isPrefixOf"));
    assert!(out.contains("isJust"));
    assert!(out.contains("isRight"));
}

#[test]
fn format_concatenates_shown_args() {
    let out = hs(
        r#"
        fn label(n: i32) -> String { format!("n={}", n) }
        "#,
    );
    assert!(out.contains("show"));
}
