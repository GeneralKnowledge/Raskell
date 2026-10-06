fn parse_positive(n: i32) -> Result<i32, String> {
    if n > 0 { Ok(n) } else { Err(String::from("non-positive")) }
}
