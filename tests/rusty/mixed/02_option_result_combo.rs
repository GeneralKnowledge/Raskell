fn lookup(xs: Vec<i32>, i: i32) -> Option<i32> {
    if i < 0 { None }
    else if (i as usize) < xs.len() { Some(xs[i as usize]) }
    else { None }
}
fn require_positive(x: Option<i32>) -> Result<i32, String> {
    match x {
        Some(n) if n > 0 => Ok(n),
        Some(_) => Err(String::from("non-positive")),
        None => Err(String::from("missing")),
    }
}
