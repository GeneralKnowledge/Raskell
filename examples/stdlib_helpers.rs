//! Everyday stdlib-shaped helpers a Rust developer might reach for.

fn describe(name: String) -> String {
    if name.is_empty() {
        String::from("anonymous")
    } else if name.starts_with("A") {
        format!("vip-{}", name.len())
    } else {
        name
    }
}

fn has_needle(hay: String, needle: String) -> bool {
    hay.contains(needle)
}

fn option_default(x: Option<i32>) -> i32 {
    if x.is_some() {
        x.unwrap_or(0)
    } else {
        0
    }
}

fn result_ok(x: Result<i32, String>) -> bool {
    x.is_ok()
}

fn abs_diff(a: i32, b: i32) -> i32 {
    (a - b).abs()
}

fn main() {
    let _ = describe(String::from("Ada"));
    let _ = has_needle(String::from("hello"), String::from("ell"));
    let _ = option_default(Some(1));
    let _ = result_ok(Ok(1));
    let _ = abs_diff(3, 10);
}
