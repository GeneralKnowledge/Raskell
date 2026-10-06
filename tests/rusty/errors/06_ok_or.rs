fn require(x: Option<i32>) -> Result<i32, String> {
    x.ok_or(String::from("missing"))
}
