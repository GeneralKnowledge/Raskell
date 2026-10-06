fn describe(n: i32) -> i32 {
    match n {
        0 => 0,
        1 => 1,
        _ => n * 2,
    }
}
