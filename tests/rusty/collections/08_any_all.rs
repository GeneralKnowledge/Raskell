fn has_negative(values: Vec<i32>) -> bool {
    values.iter().any(|x| **x < 0)
}
fn all_positive(values: Vec<i32>) -> bool {
    values.iter().all(|x| **x > 0)
}
