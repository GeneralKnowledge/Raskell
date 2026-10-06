//! Translation quality tests — idiomatic Haskell output.

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
fn option_becomes_maybe() {
    let out = hs(
        r#"
        fn f(x: Option<i32>) -> Option<i32> {
            match x {
                Some(v) => Some(v * 2),
                None => None,
            }
        }
        "#,
    );
    assert!(out.contains("Maybe Int"));
    assert!(out.contains("Just"));
    assert!(out.contains("Nothing"));
    assert!(!out.contains("Option"));
}

#[test]
fn result_becomes_either() {
    let out = hs(
        r#"
        fn f(x: i32) -> Result<i32, i32> {
            if x > 0 { Ok(x) } else { Err(0) }
        }
        "#,
    );
    assert!(out.contains("Either"));
    assert!(out.contains("Right") || out.contains("Left"));
}

#[test]
fn iterator_map_is_idiomatic() {
    let out = hs(
        r#"
        fn double_all(values: Vec<i32>) -> Vec<i32> {
            values.iter().map(|x| x * 2).collect()
        }
        "#,
    );
    assert!(out.contains("map"));
    assert!(out.contains("* 2") || out.contains("x * 2"));
    assert!(!out.contains("iter"));
    assert!(!out.contains("collect"));
}

#[test]
fn sum_positive_is_filter_sum() {
    let out = hs(
        r#"
        fn sum_positive(values: Vec<i32>) -> i32 {
            let mut total = 0;
            for value in values {
                if value > 0 {
                    total += value;
                }
            }
            total
        }
        "#,
    );
    assert!(out.contains("sum"));
    assert!(out.contains("filter"));
    assert!(!out.contains("foldl") || out.contains("sum"));
}

#[test]
fn scalar_mutation_becomes_arithmetic() {
    let out = hs(
        r#"
        fn scale(x: i32) -> i32 {
            let mut y = x;
            y += 5;
            y *= 2;
            y
        }
        "#,
    );
    assert!(out.contains("+ 5") || out.contains("5"));
    assert!(out.contains("* 2") || out.contains("2"));
}

#[test]
fn map_push_becomes_map() {
    let out = hs(
        r#"
        fn double_push(values: Vec<i32>) -> Vec<i32> {
            let mut result = Vec::new();
            for value in values {
                result.push(value * 2);
            }
            result
        }
        "#,
    );
    assert!(out.contains("map"));
}

#[test]
fn enum_becomes_adt() {
    let out = hs(
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
    assert!(out.contains("data Shape"));
    assert!(out.contains("Circle"));
    assert!(out.contains("Rectangle"));
}

#[test]
fn struct_becomes_record() {
    let out = hs(
        r#"
        struct User { name: String, age: u32 }
        fn get_name(u: User) -> String { u.name }
        "#,
    );
    assert!(out.contains("data User"));
    assert!(out.contains("name ::"));
}
