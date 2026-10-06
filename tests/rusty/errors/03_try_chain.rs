fn parse_i32(s: String) -> Result<i32, String> {
    if s.is_empty() { Err(String::from("empty")) } else { Ok(s.len() as i32) }
}
fn add_parsed(a: String, b: String) -> Result<i32, String> {
    let x = parse_i32(a)?;
    let y = parse_i32(b)?;
    Ok(x + y)
}
