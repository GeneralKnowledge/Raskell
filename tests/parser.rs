//! Parser tests for supported Rust constructs.

use raskell::parser;

fn parse_ok(src: &str) {
    parser::parse(src, "test.rs").expect("should parse");
}

#[test]
fn parses_function() {
    parse_ok("fn add(a: i32, b: i32) -> i32 { a + b }");
}

#[test]
fn parses_struct() {
    parse_ok(
        r#"
        struct Point { x: i32, y: i32 }
        fn origin() -> Point { Point { x: 0, y: 0 } }
        "#,
    );
}

#[test]
fn parses_enum() {
    parse_ok(
        r#"
        enum Opt { Yes(i32), No }
        fn f(o: Opt) -> i32 {
            match o {
                Opt::Yes(n) => n,
                Opt::No => 0,
            }
        }
        "#,
    );
}

#[test]
fn parses_closures() {
    parse_ok("fn f(xs: Vec<i32>) -> Vec<i32> { xs.iter().map(|x| x * 2).collect() }");
}

#[test]
fn parses_loops() {
    parse_ok(
        r#"
        fn sum(xs: Vec<i32>) -> i32 {
            let mut t = 0;
            for x in xs { t += x; }
            t
        }
        "#,
    );
}

#[test]
fn parses_option_result() {
    parse_ok("fn f(x: Option<i32>) -> Result<i32, i32> { match x { Some(v) => Ok(v), None => Err(0) } }");
}

#[test]
fn parses_references() {
    parse_ok("fn len(xs: &Vec<i32>) -> i32 { xs.len() as i32 }");
}

#[test]
fn parses_nested() {
    parse_ok(
        r#"
        fn f(x: i32) -> i32 {
            if x > 0 {
                if x > 10 { x * 2 } else { x + 1 }
            } else {
                0
            }
        }
        "#,
    );
}
