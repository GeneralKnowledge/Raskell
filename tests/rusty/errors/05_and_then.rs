fn pipeline(x: Option<i32>) -> Option<i32> {
    x.and_then(|n| if n > 0 { Some(n * 2) } else { None })
}
