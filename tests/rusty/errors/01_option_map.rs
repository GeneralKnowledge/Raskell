fn double_opt(x: Option<i32>) -> Option<i32> {
    match x {
        Some(n) => Some(n * 2),
        None => None,
    }
}
